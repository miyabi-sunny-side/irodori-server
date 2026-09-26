//! `SQLite` records for generations, reference audio and the reading dictionary.
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

/// A generation is kept by bulk deletion while it has been saved as a reference.
const DELETABLE: &str =
    "NOT EXISTS (SELECT 1 FROM references_audio r WHERE r.source_generation_id = generations.id)";

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
    /// Saved as a reference, so bulk deletion keeps it.
    pub saved: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Reference {
    pub id: i64,
    pub created_at: String,
    pub name: String,
    pub character: Option<String>,
    pub source_generation_id: Option<i64>,
    pub audio_url: String,
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
        if version < 2 {
            let tx = conn.transaction()?;
            tx.execute_batch(MIGRATE_V2)?;
            tx.commit()?;
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
                 reference_ids, model, seed, speed, params, log, path)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
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
                ],
            )?;
            let id = tx.last_insert_rowid();
            saved.push(Generation {
                id,
                audio_url: audio_url(id),
                saved: false,
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

    /// Records a copy of a generation's WAV as a named reference.
    pub fn save_reference(
        &self,
        character: &str,
        source_generation_id: i64,
        path: &str,
    ) -> rusqlite::Result<Reference> {
        let name = format!("生成 {source_generation_id}");
        self.0.execute(
            "INSERT INTO references_audio (created_at, name, path, character, source_generation_id)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![now(), name, path, character, source_generation_id],
        )?;
        let id = self.0.last_insert_rowid();
        self.0.query_row(
            &format!("{SELECT_REFERENCE} WHERE id = ?1"),
            [id],
            reference_row,
        )
    }

    /// References saved from the history, grouped by character.
    pub fn references(&self) -> rusqlite::Result<Vec<Reference>> {
        let mut stmt = self.0.prepare(&format!(
            "{SELECT_REFERENCE} WHERE character IS NOT NULL ORDER BY character, id"
        ))?;
        stmt.query_map([], reference_row)?.collect()
    }

    /// Deletes a reference record and returns its file path for the caller to remove.
    pub fn delete_reference(&self, id: i64) -> rusqlite::Result<Option<String>> {
        self.0
            .query_row(
                "DELETE FROM references_audio WHERE id = ?1 RETURNING path",
                [id],
                |row| row.get(0),
            )
            .optional()
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
    EXISTS (SELECT 1 FROM references_audio r WHERE r.source_generation_id = generations.id)
    FROM generations";

const SELECT_REFERENCE: &str =
    "SELECT id, created_at, name, character, source_generation_id FROM references_audio";

fn reference_row(row: &rusqlite::Row) -> rusqlite::Result<Reference> {
    let id = row.get(0)?;
    Ok(Reference {
        id,
        created_at: row.get(1)?,
        name: row.get(2)?,
        character: row.get(3)?,
        source_generation_id: row.get(4)?,
        audio_url: format!("/api/references/{id}/audio"),
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
        }
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
            db.references().unwrap().is_empty(),
            "unnamed uploads are not listed as saved references"
        );
        let version: i64 =
            db.0.pragma_query_value(None, "user_version", |row| row.get(0))
                .unwrap();
        assert_eq!(version, 2);
    }

    #[test]
    fn bulk_deletion_keeps_generations_saved_as_references() {
        let dir = tempfile::tempdir().unwrap();
        let mut db =
            Db::open(&dir.path().join("r.sqlite3"), &dir.path().join("none.json")).unwrap();
        let rows = db
            .insert_generations(&[sample("b1", 1), sample("b1", 2), sample("b1", 3)])
            .unwrap();
        let reference = db
            .save_reference("ずんだ", rows[1].id, "references/copy.wav")
            .unwrap();
        assert_eq!(reference.character.as_deref(), Some("ずんだ"));
        assert_eq!(reference.source_generation_id, Some(rows[1].id));
        assert!(db.generation(rows[1].id).unwrap().unwrap().saved);

        let removed = db.delete_unsaved_generations().unwrap();
        assert_eq!(
            removed,
            vec!["audio/b1_01.wav".to_owned(), "audio/b1_03.wav".to_owned()]
        );
        let left: Vec<i64> = db.generations().unwrap().iter().map(|g| g.id).collect();
        assert_eq!(left, vec![rows[1].id]);
        assert_eq!(db.references().unwrap(), vec![reference.clone()]);

        assert_eq!(
            db.delete_reference(reference.id).unwrap(),
            Some("references/copy.wav".into())
        );
        assert!(
            !db.generation(rows[1].id).unwrap().unwrap().saved,
            "deleting the reference releases the generation"
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
