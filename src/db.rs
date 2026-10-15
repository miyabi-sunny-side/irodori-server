//! `SQLite` records for generations, voices, reference audio, the script and the reading dictionary.
//! Paths are stored relative to the data directory; only the server writes here.

use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use serde_json::Value;

use crate::dictionary::{self, Entry};

const SCHEMA: &str = "
CREATE TABLE generations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    batch TEXT NOT NULL,
    candidate INTEGER NOT NULL,
    created_at TEXT NOT NULL,
    text TEXT NOT NULL,
    text_applied TEXT NOT NULL,
    mode TEXT NOT NULL,
    caption TEXT NOT NULL,
    reference_ids TEXT NOT NULL,
    model TEXT NOT NULL,
    seed TEXT,
    speed REAL NOT NULL,
    params TEXT NOT NULL,
    log TEXT NOT NULL,
    path TEXT NOT NULL
);
CREATE TABLE references_audio (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    created_at TEXT NOT NULL,
    name TEXT NOT NULL,
    path TEXT NOT NULL
);
CREATE TABLE dictionary (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    word TEXT NOT NULL UNIQUE,
    reading TEXT NOT NULL
);
CREATE TABLE file_errors (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    created_at TEXT NOT NULL,
    path TEXT NOT NULL,
    error TEXT NOT NULL
);
PRAGMA user_version = 1;
";

/// Version 2: references saved from the history carry a character name and their source generation.
const MIGRATE_V2: &str = "
ALTER TABLE references_audio ADD COLUMN character TEXT;
ALTER TABLE references_audio ADD COLUMN source_generation_id INTEGER;
PRAGMA user_version = 2;
";

/// Version 3: generations carry the character whose references voiced them, and a favourite mark.
const MIGRATE_V3: &str = "
ALTER TABLE generations ADD COLUMN character TEXT;
ALTER TABLE generations ADD COLUMN favorite INTEGER NOT NULL DEFAULT 0;
PRAGMA user_version = 3;
";

/// Version 4 (`EasyIrodoriTTS` v2.0): a voice fixes a name, model, speaking style and one reference.
/// Character references become voices, taking the model and style of the generation they were
/// saved from (the default model and no style without one); unnamed uploads stay references.
/// The script lives in one row; each card's latest generation is kept beside it.
const MIGRATE_V4: &str = "
CREATE TABLE voices (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    created_at TEXT NOT NULL,
    name TEXT NOT NULL,
    model TEXT NOT NULL,
    caption TEXT NOT NULL,
    path TEXT NOT NULL,
    source_generation_id INTEGER,
    revision INTEGER NOT NULL DEFAULT 1,
    deleted INTEGER NOT NULL DEFAULT 0
);
INSERT INTO voices (created_at, name, model, caption, path, source_generation_id)
    SELECT r.created_at, r.character, COALESCE(g.model, 'Aratako/Irodori-TTS-v4.1-Small'),
        CASE WHEN g.mode IN ('design', 'both') THEN g.caption ELSE '' END,
        r.path, r.source_generation_id
    FROM references_audio r LEFT JOIN generations g ON g.id = r.source_generation_id
    WHERE r.character IS NOT NULL ORDER BY r.id;
DELETE FROM references_audio WHERE character IS NOT NULL;
ALTER TABLE generations ADD COLUMN voice_id INTEGER;
CREATE TABLE script (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    revision INTEGER NOT NULL,
    doc TEXT NOT NULL
);
CREATE TABLE script_results (
    card_id TEXT PRIMARY KEY,
    generation_id INTEGER,
    signature TEXT NOT NULL DEFAULT '',
    error TEXT NOT NULL DEFAULT ''
);
PRAGMA user_version = 4;
";

/// Bulk deletion keeps favourites, generations registered as voices and script audio.
const DELETABLE: &str = "favorite = 0
    AND NOT EXISTS (SELECT 1 FROM voices v WHERE v.source_generation_id = generations.id AND v.deleted = 0)
    AND NOT EXISTS (SELECT 1 FROM script_results s WHERE s.generation_id = generations.id)";

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Generation {
    pub id: i64,
    pub batch: String,
    pub candidate: i64,
    pub created_at: String,
    pub text: String,
    pub text_applied: String,
    pub mode: String,
    pub caption: String,
    pub reference_ids: Vec<i64>,
    pub model: String,
    pub seed: Option<String>,
    pub speed: f64,
    pub params: Value,
    pub log: String,
    #[serde(skip)]
    pub path: String,
    pub audio_url: String,
    /// Registered as a voice, so bulk deletion keeps it.
    pub saved: bool,
    /// The voice's name, for generations spoken by a registered voice.
    pub character: Option<String>,
    pub voice_id: Option<i64>,
    /// A script card's current audio, so bulk deletion keeps it.
    pub scripted: bool,
    /// Marked with ★, so bulk deletion keeps it.
    pub favorite: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Voice {
    pub id: i64,
    pub created_at: String,
    pub name: String,
    pub model: String,
    pub caption: String,
    pub source_generation_id: Option<i64>,
    /// Raised by renaming; a rename sent with an older revision is a conflict.
    pub revision: i64,
    pub audio_url: String,
    #[serde(skip)]
    pub path: String,
}

/// Why a rename was not applied.
#[derive(Debug, PartialEq)]
pub enum Rename {
    Missing,
    Conflict,
}

/// A card's latest generation and the input it was made from.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CardResult {
    pub generation_id: Option<i64>,
    /// The generation still exists.
    pub audio: bool,
    pub signature: String,
    pub error: String,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct FileError {
    pub created_at: String,
    pub path: String,
    pub error: String,
}

pub struct Db(Connection);

pub fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

impl Db {
    /// Opens the database. A new database imports the phase-1 dictionary JSON once;
    /// the JSON file itself is never modified.
    pub fn open(path: &Path, legacy_dictionary: &Path) -> rusqlite::Result<Self> {
        let mut conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        let version: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if version == 0 {
            let tx = conn.transaction()?;
            tx.execute_batch(SCHEMA)?;
            for (word, reading) in legacy_entries(legacy_dictionary) {
                tx.execute(
                    "INSERT INTO dictionary (word, reading) VALUES (?1, ?2)",
                    params![word, reading],
                )?;
            }
            tx.commit()?;
        }
        let version: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
        for (target, migration) in [(2, MIGRATE_V2), (3, MIGRATE_V3), (4, MIGRATE_V4)] {
            if version < target {
                let tx = conn.transaction()?;
                tx.execute_batch(migration)?;
                tx.commit()?;
            }
        }
        Ok(Self(conn))
    }

    pub fn dictionary(&self) -> rusqlite::Result<Vec<Entry>> {
        let mut stmt = self
            .0
            .prepare("SELECT word, reading FROM dictionary ORDER BY id")?;
        stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect()
    }

    /// Returns true when an existing word was updated.
    pub fn put_word(&self, word: &str, reading: &str) -> rusqlite::Result<bool> {
        let updated = self.0.execute(
            "UPDATE dictionary SET reading = ?2 WHERE word = ?1",
            params![word, reading],
        )? > 0;
        if !updated {
            self.0.execute(
                "INSERT INTO dictionary (word, reading) VALUES (?1, ?2)",
                params![word, reading],
            )?;
        }
        Ok(updated)
    }

    pub fn delete_word(&self, word: &str) -> rusqlite::Result<bool> {
        Ok(self
            .0
            .execute("DELETE FROM dictionary WHERE word = ?1", params![word])?
            > 0)
    }

    pub fn add_reference(&self, name: &str, path: &str) -> rusqlite::Result<i64> {
        self.0.execute(
            "INSERT INTO references_audio (created_at, name, path) VALUES (?1, ?2, ?3)",
            params![now(), name, path],
        )?;
        Ok(self.0.last_insert_rowid())
    }

    pub fn set_favorite(&self, id: i64, favorite: bool) -> rusqlite::Result<bool> {
        Ok(self.0.execute(
            "UPDATE generations SET favorite = ?2 WHERE id = ?1",
            params![id, favorite],
        )? > 0)
    }

    pub fn reference_path(&self, id: i64) -> rusqlite::Result<Option<String>> {
        self.0
            .query_row(
                "SELECT path FROM references_audio WHERE id = ?1",
                [id],
                |row| row.get(0),
            )
            .optional()
    }

    /// Records one generation's candidates atomically.
    pub fn insert_generations(&mut self, rows: &[Generation]) -> rusqlite::Result<Vec<Generation>> {
        let tx = self.0.transaction()?;
        let mut saved = Vec::with_capacity(rows.len());
        for row in rows {
            tx.execute(
                "INSERT INTO generations (batch, candidate, created_at, text, text_applied, mode, caption,
                 reference_ids, model, seed, speed, params, log, path, character, voice_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
                params![
                    row.batch,
                    row.candidate,
                    row.created_at,
                    row.text,
                    row.text_applied,
                    row.mode,
                    row.caption,
                    Value::from(row.reference_ids.clone()).to_string(),
                    row.model,
                    row.seed,
                    row.speed,
                    row.params.to_string(),
                    row.log,
                    row.path,
                    row.character,
                    row.voice_id,
                ],
            )?;
            let id = tx.last_insert_rowid();
            saved.push(Generation {
                id,
                audio_url: audio_url(id),
                saved: false,
                favorite: false,
                ..row.clone()
            });
        }
        tx.commit()?;
        Ok(saved)
    }

    pub fn generations(&self) -> rusqlite::Result<Vec<Generation>> {
        let mut stmt = self
            .0
            .prepare(&format!("{SELECT_GENERATION} ORDER BY id DESC"))?;
        stmt.query_map([], generation_row)?.collect()
    }

    pub fn generation(&self, id: i64) -> rusqlite::Result<Option<Generation>> {
        self.0
            .query_row(
                &format!("{SELECT_GENERATION} WHERE id = ?1"),
                [id],
                generation_row,
            )
            .optional()
    }

    /// Deletes the record and returns its file path for the caller to remove.
    pub fn delete_generation(&self, id: i64) -> rusqlite::Result<Option<String>> {
        self.0
            .query_row(
                "DELETE FROM generations WHERE id = ?1 RETURNING path",
                [id],
                |row| row.get(0),
            )
            .optional()
    }

    pub fn add_voice(
        &self,
        name: &str,
        model: &str,
        caption: &str,
        path: &str,
        source_generation_id: Option<i64>,
    ) -> rusqlite::Result<Voice> {
        self.0.execute(
            "INSERT INTO voices (created_at, name, model, caption, path, source_generation_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![now(), name, model, caption, path, source_generation_id],
        )?;
        Ok(self
            .voice(self.0.last_insert_rowid())?
            .expect("inserted above"))
    }

    /// Registered voices, oldest first. Deleted voices are hidden.
    pub fn voices(&self) -> rusqlite::Result<Vec<Voice>> {
        let mut stmt = self
            .0
            .prepare(&format!("{SELECT_VOICE} WHERE deleted = 0 ORDER BY id"))?;
        stmt.query_map([], voice_row)?.collect()
    }

    pub fn voice(&self, id: i64) -> rusqlite::Result<Option<Voice>> {
        self.0
            .query_row(
                &format!("{SELECT_VOICE} WHERE id = ?1 AND deleted = 0"),
                [id],
                voice_row,
            )
            .optional()
    }

    /// Renames a voice whose revision matches; only the name of a voice can change.
    pub fn rename_voice(
        &self,
        id: i64,
        name: &str,
        revision: i64,
    ) -> rusqlite::Result<Result<Voice, Rename>> {
        let Some(voice) = self.voice(id)? else {
            return Ok(Err(Rename::Missing));
        };
        if voice.revision != revision {
            return Ok(Err(Rename::Conflict));
        }
        if voice.name != name {
            self.0.execute(
                "UPDATE voices SET name = ?2, revision = revision + 1 WHERE id = ?1",
                params![id, name],
            )?;
        }
        Ok(Ok(self.voice(id)?.expect("checked above")))
    }

    /// Hides a voice. Its reference stays on disk for generations already queued with it.
    pub fn delete_voice(&self, id: i64) -> rusqlite::Result<bool> {
        Ok(self.0.execute(
            "UPDATE voices SET deleted = 1 WHERE id = ?1 AND deleted = 0",
            [id],
        )? > 0)
    }

    /// The script's revision and JSON, or `None` before one is stored.
    pub fn script(&self) -> rusqlite::Result<Option<(i64, String)>> {
        self.0
            .query_row("SELECT revision, doc FROM script WHERE id = 1", [], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .optional()
    }

    /// Stores the script when `revision` is the stored one (0 before any) and returns the new
    /// revision, or `None` when another edit came first.
    pub fn put_script(&self, revision: i64, doc: &str) -> rusqlite::Result<Option<i64>> {
        let changed = if revision == 0 {
            self.0.execute(
                "INSERT OR IGNORE INTO script (id, revision, doc) VALUES (1, 1, ?1)",
                [doc],
            )?
        } else {
            self.0.execute(
                "UPDATE script SET revision = revision + 1, doc = ?2 WHERE id = 1 AND revision = ?1",
                params![revision, doc],
            )?
        };
        Ok((changed > 0).then_some(revision + 1))
    }

    /// The latest generation of every card that has one.
    pub fn card_results(&self) -> rusqlite::Result<std::collections::HashMap<String, CardResult>> {
        let mut stmt = self.0.prepare(
            "SELECT s.card_id, s.generation_id, g.id IS NOT NULL, s.signature, s.error
             FROM script_results s LEFT JOIN generations g ON g.id = s.generation_id",
        )?;
        stmt.query_map([], |row| {
            Ok((
                row.get(0)?,
                CardResult {
                    generation_id: row.get(1)?,
                    audio: row.get(2)?,
                    signature: row.get(3)?,
                    error: row.get(4)?,
                },
            ))
        })?
        .collect()
    }

    /// Records a card's new audio and clears its error.
    pub fn set_card_audio(
        &self,
        card_id: &str,
        generation_id: i64,
        signature: &str,
    ) -> rusqlite::Result<()> {
        self.0.execute(
            "INSERT INTO script_results (card_id, generation_id, signature, error) VALUES (?1, ?2, ?3, '')
             ON CONFLICT (card_id) DO UPDATE SET generation_id = ?2, signature = ?3, error = ''",
            params![card_id, generation_id, signature],
        )?;
        Ok(())
    }

    /// Records a failed attempt, keeping the card's earlier audio.
    pub fn set_card_error(&self, card_id: &str, error: &str) -> rusqlite::Result<()> {
        self.0.execute(
            "INSERT INTO script_results (card_id, error) VALUES (?1, ?2)
             ON CONFLICT (card_id) DO UPDATE SET error = ?2",
            params![card_id, error],
        )?;
        Ok(())
    }

    /// Deletes every generation that bulk deletion may remove and returns their file paths.
    pub fn delete_unsaved_generations(&mut self) -> rusqlite::Result<Vec<String>> {
        let tx = self.0.transaction()?;
        let paths = tx
            .prepare(&format!(
                "DELETE FROM generations WHERE {DELETABLE} RETURNING path"
            ))?
            .query_map([], |row| row.get(0))?
            .collect::<rusqlite::Result<Vec<String>>>()?;
        tx.commit()?;
        Ok(paths)
    }

    pub fn add_file_error(&self, path: &str, error: &str) -> rusqlite::Result<()> {
        self.0.execute(
            "INSERT INTO file_errors (created_at, path, error) VALUES (?1, ?2, ?3)",
            params![now(), path, error],
        )?;
        Ok(())
    }

    pub fn file_errors(&self) -> rusqlite::Result<Vec<FileError>> {
        let mut stmt = self
            .0
            .prepare("SELECT created_at, path, error FROM file_errors ORDER BY id DESC")?;
        stmt.query_map([], |row| {
            Ok(FileError {
                created_at: row.get(0)?,
                path: row.get(1)?,
                error: row.get(2)?,
            })
        })?
        .collect()
    }
}

const SELECT_GENERATION: &str =
    "SELECT id, batch, candidate, created_at, text, text_applied, mode, caption,
    reference_ids, model, seed, speed, params, log, path,
    EXISTS (SELECT 1 FROM voices v WHERE v.source_generation_id = generations.id AND v.deleted = 0),
    character, favorite, voice_id,
    EXISTS (SELECT 1 FROM script_results s WHERE s.generation_id = generations.id)
    FROM generations";

const SELECT_VOICE: &str =
    "SELECT id, created_at, name, model, caption, source_generation_id, revision, path FROM voices";

fn voice_row(row: &rusqlite::Row) -> rusqlite::Result<Voice> {
    let id = row.get(0)?;
    Ok(Voice {
        id,
        created_at: row.get(1)?,
        name: row.get(2)?,
        model: row.get(3)?,
        caption: row.get(4)?,
        source_generation_id: row.get(5)?,
        revision: row.get(6)?,
        audio_url: format!("/api/voices/{id}/audio"),
        path: row.get(7)?,
    })
}

fn generation_row(row: &rusqlite::Row) -> rusqlite::Result<Generation> {
    let id = row.get(0)?;
    let json = |index: usize| -> rusqlite::Result<Value> {
        Ok(serde_json::from_str(&row.get::<_, String>(index)?).unwrap_or(Value::Null))
    };
    Ok(Generation {
        id,
        batch: row.get(1)?,
        candidate: row.get(2)?,
        created_at: row.get(3)?,
        text: row.get(4)?,
        text_applied: row.get(5)?,
        mode: row.get(6)?,
        caption: row.get(7)?,
        reference_ids: serde_json::from_value(json(8)?).unwrap_or_default(),
        model: row.get(9)?,
        seed: row.get(10)?,
        speed: row.get(11)?,
        params: json(12)?,
        log: row.get(13)?,
        path: row.get(14)?,
        audio_url: audio_url(id),
        saved: row.get(15)?,
        character: row.get(16)?,
        favorite: row.get(17)?,
        voice_id: row.get(18)?,
        scripted: row.get(19)?,
    })
}

fn audio_url(id: i64) -> String {
    format!("/api/generations/{id}/audio")
}

fn legacy_entries(path: &Path) -> Vec<Entry> {
    let Ok(json) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    dictionary::parse_legacy_json(&json).unwrap_or_else(|error| {
        tracing::warn!(path = %path.display(), error, "reading dictionary was not imported");
        Vec::new()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(batch: &str, candidate: i64) -> Generation {
        Generation {
            id: 0,
            batch: batch.into(),
            candidate,
            created_at: now(),
            text: "Irodoriです。".into(),
            text_applied: "いろどりです。".into(),
            mode: "clone".into(),
            caption: String::new(),
            reference_ids: vec![2, 1],
            model: "Aratako/Irodori-TTS-v4.1-Small".into(),
            seed: Some("18446744073709551615".into()),
            speed: 1.25,
            params: serde_json::json!({"num_steps": 8}),
            log: "log".into(),
            path: format!("audio/{batch}_{candidate:02}.wav"),
            audio_url: String::new(),
            saved: false,
            character: None,
            favorite: false,
            voice_id: None,
            scripted: false,
        }
    }

    fn open(dir: &tempfile::TempDir, name: &str) -> Db {
        Db::open(&dir.path().join(name), &dir.path().join("none.json")).unwrap()
    }

    #[test]
    fn a_new_database_imports_the_legacy_dictionary_once_and_leaves_the_json() {
        let dir = tempfile::tempdir().unwrap();
        let json = dir.path().join("reading_dictionary.json");
        let original = "{\"TTS\": \"てぃーてぃーえす\", \"Irodori\": \"いろどり\"}\n";
        std::fs::write(&json, original).unwrap();
        let db_path = dir.path().join("irodori.sqlite3");

        let db = Db::open(&db_path, &json).unwrap();
        assert_eq!(
            db.dictionary().unwrap(),
            vec![
                ("TTS".to_owned(), "てぃーてぃーえす".to_owned()),
                ("Irodori".to_owned(), "いろどり".to_owned())
            ]
        );
        db.delete_word("TTS").unwrap();
        db.delete_word("Irodori").unwrap();
        drop(db);

        let db = Db::open(&db_path, &json).unwrap();
        assert!(
            db.dictionary().unwrap().is_empty(),
            "an emptied dictionary must not be re-imported"
        );
        assert_eq!(std::fs::read_to_string(&json).unwrap(), original);
    }

    #[test]
    fn a_missing_or_broken_legacy_dictionary_starts_empty() {
        let dir = tempfile::tempdir().unwrap();
        let broken = dir.path().join("broken.json");
        std::fs::write(&broken, "[1]").unwrap();
        let db = Db::open(
            &dir.path().join("a.sqlite3"),
            &dir.path().join("missing.json"),
        )
        .unwrap();
        assert!(db.dictionary().unwrap().is_empty());
        let db = Db::open(&dir.path().join("b.sqlite3"), &broken).unwrap();
        assert!(db.dictionary().unwrap().is_empty());
        assert_eq!(std::fs::read_to_string(&broken).unwrap(), "[1]");
    }

    #[test]
    fn put_word_updates_in_place_and_keeps_order() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(&dir.path().join("d.sqlite3"), &dir.path().join("none.json")).unwrap();
        assert!(!db.put_word("a", "1").unwrap());
        assert!(!db.put_word("b", "2").unwrap());
        assert!(db.put_word("a", "3").unwrap());
        assert_eq!(
            db.dictionary().unwrap(),
            vec![("a".into(), "3".into()), ("b".into(), "2".into())]
        );
        assert!(db.delete_word("a").unwrap());
        assert!(!db.delete_word("a").unwrap());
    }

    #[test]
    fn generations_round_trip_newest_first_and_delete_returns_the_path() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("g.sqlite3");
        let mut db = Db::open(&db_path, &dir.path().join("none.json")).unwrap();
        let saved = db
            .insert_generations(&[sample("b1", 1), sample("b1", 2)])
            .unwrap();
        assert_eq!(
            saved[1].audio_url,
            format!("/api/generations/{}/audio", saved[1].id)
        );
        drop(db);

        let db = Db::open(&db_path, &dir.path().join("none.json")).unwrap();
        let listed = db.generations().unwrap();
        assert_eq!(
            listed.iter().map(|g| g.candidate).collect::<Vec<_>>(),
            [2, 1]
        );
        assert_eq!(listed[1], saved[0]);
        assert_eq!(
            db.delete_generation(saved[0].id).unwrap(),
            Some("audio/b1_01.wav".into())
        );
        assert_eq!(db.delete_generation(saved[0].id).unwrap(), None);
        assert_eq!(db.generation(saved[0].id).unwrap(), None);
    }

    #[test]
    fn a_version_1_database_is_migrated_without_losing_rows() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("v1.sqlite3");
        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute_batch(SCHEMA).unwrap();
            conn.execute("INSERT INTO references_audio (created_at, name, path) VALUES ('t', 'up.m4a', 'references/a.m4a')", [])
                .unwrap();
            conn.execute(
                "INSERT INTO dictionary (word, reading) VALUES ('a', 'b')",
                [],
            )
            .unwrap();
        }
        let mut db = Db::open(&db_path, &dir.path().join("none.json")).unwrap();
        db.insert_generations(&[sample("b1", 1)]).unwrap();
        assert_eq!(
            db.reference_path(1).unwrap(),
            Some("references/a.m4a".into())
        );
        assert_eq!(db.dictionary().unwrap(), vec![("a".into(), "b".into())]);
        assert!(
            db.voices().unwrap().is_empty(),
            "unnamed uploads are not voices"
        );
        let version: i64 =
            db.0.pragma_query_value(None, "user_version", |row| row.get(0))
                .unwrap();
        assert_eq!(version, 4);
    }

    /// A version 3 database as the previous release left it.
    fn version_3(path: &Path) {
        let conn = Connection::open(path).unwrap();
        conn.execute_batch(SCHEMA).unwrap();
        conn.execute_batch(MIGRATE_V2).unwrap();
        conn.execute_batch(MIGRATE_V3).unwrap();
        conn.execute_batch(
            "INSERT INTO generations (batch, candidate, created_at, text, text_applied, mode, caption,
                reference_ids, model, seed, speed, params, log, path, character, favorite)
             VALUES ('b', 1, 't', 'あ', 'あ', 'design', '明るい声', '[]',
                'phasefield-audio/Irodori-TTS-v4.1-Anime', '1', 1.0, '{}', '', 'audio/b_01.wav', NULL, 0),
                    ('b', 2, 't', 'い', 'い', 'clone', '使われない説明', '[1]',
                'Aratako/Irodori-TTS-v4.1-Small', '2', 1.0, '{}', '', 'audio/b_02.wav', NULL, 1);
             INSERT INTO references_audio (created_at, name, path, character, source_generation_id)
             VALUES ('t1', 'up.wav', 'references/up.wav', NULL, NULL),
                    ('t2', '生成 1', 'references/zunda.wav', 'ずんだ', 1),
                    ('t3', '生成 2', 'references/zunda2.wav', 'ずんだ', 2),
                    ('t4', '生成 9', 'references/gone.wav', 'あかり', 9);",
        )
        .unwrap();
    }

    #[test]
    fn a_version_3_database_turns_character_references_into_voices() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("v3.sqlite3");
        version_3(&db_path);
        let db = Db::open(&db_path, &dir.path().join("none.json")).unwrap();
        let voices: Vec<_> = db
            .voices()
            .unwrap()
            .into_iter()
            .map(|v| (v.name, v.model, v.caption, v.path, v.source_generation_id))
            .collect();
        assert_eq!(
            voices,
            [
                (
                    "ずんだ".into(),
                    "phasefield-audio/Irodori-TTS-v4.1-Anime".into(),
                    "明るい声".into(),
                    "references/zunda.wav".into(),
                    Some(1)
                ),
                (
                    "ずんだ".into(),
                    "Aratako/Irodori-TTS-v4.1-Small".into(),
                    String::new(),
                    "references/zunda2.wav".into(),
                    Some(2)
                ),
                (
                    "あかり".into(),
                    "Aratako/Irodori-TTS-v4.1-Small".into(),
                    String::new(),
                    "references/gone.wav".into(),
                    Some(9)
                ),
            ],
            "the style of a clone generation was not used, and a lost source takes the defaults"
        );
        assert_eq!(
            db.reference_path(1).unwrap(),
            Some("references/up.wav".into()),
            "uploads stay references"
        );
        assert_eq!(
            db.reference_path(2).unwrap(),
            None,
            "no reference is kept twice"
        );
        let history = db.generations().unwrap();
        assert_eq!(history.len(), 2);
        assert!(
            history.iter().all(|g| g.saved),
            "registered generations stay protected"
        );
        assert!(history.iter().any(|g| g.favorite));
    }

    #[test]
    fn bulk_deletion_keeps_voice_sources_and_script_audio() {
        let dir = tempfile::tempdir().unwrap();
        let mut db = open(&dir, "r.sqlite3");
        let rows = db
            .insert_generations(&[
                sample("b1", 1),
                sample("b1", 2),
                sample("b1", 3),
                sample("b1", 4),
            ])
            .unwrap();
        let voice = db
            .add_voice("ずんだ", "m", "明るく", "voices/a.wav", Some(rows[1].id))
            .unwrap();
        assert_eq!(voice.source_generation_id, Some(rows[1].id));
        assert!(db.generation(rows[1].id).unwrap().unwrap().saved);
        db.set_card_audio("card", rows[2].id, "sig").unwrap();
        assert!(db.generation(rows[2].id).unwrap().unwrap().scripted);

        let removed = db.delete_unsaved_generations().unwrap();
        assert_eq!(
            removed,
            vec!["audio/b1_01.wav".to_owned(), "audio/b1_04.wav".to_owned()]
        );
        let left: Vec<i64> = db.generations().unwrap().iter().map(|g| g.id).collect();
        assert_eq!(left, vec![rows[2].id, rows[1].id]);

        assert!(db.delete_voice(voice.id).unwrap());
        assert!(!db.delete_voice(voice.id).unwrap());
        assert!(
            !db.generation(rows[1].id).unwrap().unwrap().saved,
            "deleting the voice releases the generation"
        );
    }

    #[test]
    fn voices_are_renamed_by_revision_and_hidden_when_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let db = open(&dir, "v.sqlite3");
        let voice = db.add_voice("a", "m", "", "voices/a.wav", None).unwrap();
        assert_eq!(voice.revision, 1);
        let renamed = db.rename_voice(voice.id, "b", 1).unwrap().unwrap();
        assert_eq!((renamed.name.as_str(), renamed.revision), ("b", 2));
        assert_eq!(
            db.rename_voice(voice.id, "c", 1).unwrap(),
            Err(Rename::Conflict)
        );
        assert_eq!(
            db.rename_voice(voice.id, "b", 2).unwrap().unwrap().revision,
            2,
            "an unchanged name keeps the revision"
        );
        assert!(db.delete_voice(voice.id).unwrap());
        assert_eq!(db.voice(voice.id).unwrap(), None);
        assert!(db.voices().unwrap().is_empty());
        assert_eq!(
            db.rename_voice(voice.id, "d", 2).unwrap(),
            Err(Rename::Missing)
        );
    }

    #[test]
    fn the_script_is_stored_by_revision() {
        let dir = tempfile::tempdir().unwrap();
        let db = open(&dir, "s.sqlite3");
        assert_eq!(db.script().unwrap(), None);
        assert_eq!(db.put_script(0, "{\"a\":1}").unwrap(), Some(1));
        assert_eq!(db.put_script(0, "{}").unwrap(), None);
        assert_eq!(db.put_script(1, "{\"a\":2}").unwrap(), Some(2));
        assert_eq!(
            db.put_script(1, "{}").unwrap(),
            None,
            "a stale edit conflicts"
        );
        assert_eq!(db.script().unwrap(), Some((2, "{\"a\":2}".into())));
    }

    #[test]
    fn card_results_keep_the_last_audio_through_failures() {
        let dir = tempfile::tempdir().unwrap();
        let mut db = open(&dir, "c.sqlite3");
        let rows = db.insert_generations(&[sample("b1", 1)]).unwrap();
        db.set_card_error("new", "失敗").unwrap();
        db.set_card_audio("a", rows[0].id, "s1").unwrap();
        db.set_card_error("a", "再生成の失敗").unwrap();
        let results = db.card_results().unwrap();
        assert_eq!(
            results["new"],
            CardResult {
                generation_id: None,
                audio: false,
                signature: String::new(),
                error: "失敗".into()
            }
        );
        assert_eq!(
            results["a"],
            CardResult {
                generation_id: Some(rows[0].id),
                audio: true,
                signature: "s1".into(),
                error: "再生成の失敗".into()
            }
        );
        db.set_card_audio("a", rows[0].id, "s2").unwrap();
        assert_eq!(db.card_results().unwrap()["a"].error, "");
        db.delete_generation(rows[0].id).unwrap();
        assert!(!db.card_results().unwrap()["a"].audio);
    }

    #[test]
    fn favourites_survive_bulk_deletion_and_characters_are_stored() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("f.sqlite3");
        let mut db = Db::open(&db_path, &dir.path().join("none.json")).unwrap();
        let mut voiced = sample("b1", 1);
        voiced.character = Some("ずんだ".into());
        let rows = db.insert_generations(&[voiced, sample("b1", 2)]).unwrap();
        assert!(db.set_favorite(rows[0].id, true).unwrap());
        assert!(!db.set_favorite(9999, true).unwrap());
        drop(db);

        let mut db = Db::open(&db_path, &dir.path().join("none.json")).unwrap();
        let kept = db.generation(rows[0].id).unwrap().unwrap();
        assert!(kept.favorite);
        assert_eq!(kept.character.as_deref(), Some("ずんだ"));
        assert_eq!(
            db.delete_unsaved_generations().unwrap(),
            vec!["audio/b1_02.wav".to_owned()]
        );
        db.set_favorite(rows[0].id, false).unwrap();
        assert_eq!(
            db.delete_unsaved_generations().unwrap(),
            vec!["audio/b1_01.wav".to_owned()]
        );
    }

    #[test]
    fn file_errors_are_listed() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(&dir.path().join("f.sqlite3"), &dir.path().join("none.json")).unwrap();
        db.add_file_error("audio/x.wav", "permission denied")
            .unwrap();
        let errors = db.file_errors().unwrap();
        assert_eq!(
            (errors[0].path.as_str(), errors[0].error.as_str()),
            ("audio/x.wav", "permission denied")
        );
    }
}
