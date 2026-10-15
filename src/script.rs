//! The 音声作成 script: cards of text spoken by registered voices, kept on the server.
//! Everything here is pure; the router stores the script and runs its generations.

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    db::CardResult,
    generation::{GenerateRequest, MAX_TEXT_CHARS},
};

pub const MAX_CARDS: usize = 1000;
pub const DEFAULT_PATTERN: &str = "$連番$_$話者名$_$日付$";

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct Card {
    pub id: String,
    pub text: String,
    pub voice_id: Option<i64>,
    pub speed: f64,
    pub silence: f64,
}

impl Default for Card {
    fn default() -> Self {
        Self {
            id: String::new(),
            text: String::new(),
            voice_id: None,
            speed: 1.0,
            silence: 0.0,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct Options {
    /// A card added after the selected one copies its voice, speed and silence.
    pub inherit: bool,
    /// File names for downloads and the ZIP, with `$連番$` `$話者名$` `$テキスト$` `$日付$`.
    pub pattern: String,
    /// The ZIP carries each card's text beside its WAV.
    pub txt: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            inherit: true,
            pattern: DEFAULT_PATTERN.into(),
            txt: false,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct Script {
    pub cards: Vec<Card>,
    /// Generation settings shared by every card; per-card fields are cleared.
    pub settings: GenerateRequest,
    pub dictionary_enabled: bool,
    pub options: Options,
}

impl Default for Script {
    fn default() -> Self {
        Self {
            cards: Vec::new(),
            settings: GenerateRequest::default(),
            dictionary_enabled: true,
            options: Options::default(),
        }
    }
}

impl Script {
    /// Checks a script sent by the browser and clears the settings each card decides.
    pub fn validate(&mut self) -> Result<(), String> {
        if self.cards.is_empty() || self.cards.len() > MAX_CARDS {
            return Err(format!("セリフは1～{MAX_CARDS}枚にしてください。"));
        }
        let mut ids = std::collections::HashSet::new();
        for card in &self.cards {
            if card.id.is_empty() || card.id.len() > 64 || !ids.insert(card.id.as_str()) {
                return Err("セリフの識別子が不正です。画面を読み直してください。".into());
            }
            if card.text.chars().count() > MAX_TEXT_CHARS {
                return Err(format!("1枚のセリフは{MAX_TEXT_CHARS}文字までです。"));
            }
            if !(0.75..=1.5).contains(&card.speed) {
                return Err("話速は0.75～1.50倍で指定してください。".into());
            }
            if !(0.0..=10.0).contains(&card.silence) {
                return Err("末尾の無音は0～10秒で指定してください。".into());
            }
        }
        let pattern = self.options.pattern.trim();
        if pattern.is_empty() || pattern.chars().count() > 200 {
            return Err("ファイル名の付け方を1～200文字で入力してください。".into());
        }
        self.settings = self.request(&Card::default());
        Ok(())
    }

    /// The generation request for one card.
    pub fn request(&self, card: &Card) -> GenerateRequest {
        GenerateRequest {
            text: card.text.clone(),
            voice_id: card.voice_id,
            speed: card.speed,
            silence: card.silence,
            dictionary_enabled: self.dictionary_enabled,
            num_candidates: 1,
            model: GenerateRequest::default().model,
            mode: GenerateRequest::default().mode,
            caption: String::new(),
            reference_ids: Vec::new(),
            ..self.settings.clone()
        }
    }
}

/// What a card's audio was made from. Equal signatures mean the audio is still current:
/// the text after the reading dictionary, the voice, speed, silence and generation settings.
pub fn signature(card: &Card, text_applied: &str, settings: &GenerateRequest) -> String {
    let shared = GenerateRequest {
        text: String::new(),
        ..settings.clone()
    };
    json!([
        text_applied,
        card.voice_id,
        card.speed,
        card.silence,
        shared
    ])
    .to_string()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// No text: skipped by generation, playback and export.
    Empty,
    /// The card's voice was deleted or never chosen.
    NoVoice,
    /// Not generated yet.
    New,
    Queued,
    Running,
    Done,
    /// Generated from different input than the card holds now.
    Changed,
    Failed,
}

/// Where the card's generation job stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Job {
    Queued,
    Running,
}

pub fn status(
    card: &Card,
    voice_exists: bool,
    job: Option<Job>,
    outcome: Option<&CardResult>,
    current: &str,
) -> Status {
    match job {
        Some(Job::Running) => return Status::Running,
        Some(Job::Queued) => return Status::Queued,
        None => {}
    }
    if card.text.trim().is_empty() {
        return Status::Empty;
    }
    if !voice_exists {
        return Status::NoVoice;
    }
    match outcome {
        Some(o) if o.audio && o.signature == current => Status::Done,
        Some(o) if o.audio => Status::Changed,
        Some(o) if !o.error.is_empty() => Status::Failed,
        _ => Status::New,
    }
}

/// Indexes of the cards to play or export in order, or why the script is not ready.
pub fn ready(statuses: &[Status]) -> Result<Vec<usize>, String> {
    let spoken = || {
        statuses
            .iter()
            .enumerate()
            .filter(|(_, s)| **s != Status::Empty)
    };
    let missing: Vec<String> = spoken()
        .filter(|(_, s)| **s != Status::Done)
        .map(|(i, _)| (i + 1).to_string())
        .collect();
    if !missing.is_empty() {
        let shown = missing
            .iter()
            .take(30)
            .cloned()
            .collect::<Vec<_>>()
            .join("・");
        let more = if missing.len() > 30 { " ほか" } else { "" };
        return Err(format!(
            "セリフ {shown}{more} に、現在の入力内容に対応する音声がありません。未生成・変更分を生成してから操作してください。"
        ));
    }
    let ready: Vec<usize> = spoken().map(|(i, _)| i).collect();
    if ready.is_empty() {
        return Err(
            "再生・保存するセリフがありません。文章を入力して音声を作成してください。".into(),
        );
    }
    Ok(ready)
}

/// A download file name without extension, from the pattern's tokens.
pub fn file_name(pattern: &str, ordinal: usize, voice: &str, text: &str, date: &str) -> String {
    let text: String = text.chars().take(40).collect();
    let filled = pattern
        .replace("$連番$", &format!("{ordinal:03}"))
        .replace("$話者名$", voice)
        .replace("$テキスト$", &text)
        .replace("$日付$", date);
    if filled.trim().is_empty() {
        return "音声".into();
    }
    let safe: String = filled
        .chars()
        .map(|c| {
            if c.is_control() || r#"<>:"/\|?*"#.contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let trimmed = |s: &str| s.trim().trim_end_matches(['.', ' ']).to_owned();
    let mut name = trimmed(&safe.chars().take(100).collect::<String>());
    // Downloads may land on Windows, where these names are devices.
    let stem = name.split('.').next().unwrap_or_default().to_uppercase();
    let reserved = ["CON", "PRN", "AUX", "NUL"].contains(&stem.as_str())
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.ends_with(|c: char| ('1'..='9').contains(&c)));
    if reserved {
        name.insert(0, '_');
    }
    if name.is_empty() {
        "音声".into()
    } else {
        name
    }
}

/// Suffixes `_001`, `_002` … to names that repeat, ignoring case, so none is overwritten.
pub fn unique_names(names: &[String]) -> Vec<String> {
    let mut seen = std::collections::HashMap::new();
    names
        .iter()
        .map(|name| {
            let count = seen.entry(name.to_lowercase()).or_insert(0);
            let unique = if *count == 0 {
                name.clone()
            } else {
                format!("{name}_{count:03}")
            };
            *count += 1;
            unique
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(id: &str, text: &str) -> Card {
        Card {
            id: id.into(),
            text: text.into(),
            voice_id: Some(1),
            ..Card::default()
        }
    }

    fn script(cards: Vec<Card>) -> Script {
        Script {
            cards,
            ..Script::default()
        }
    }

    #[test]
    fn a_valid_script_clears_the_per_card_settings() {
        let mut doc = script(vec![card("a", "こんにちは"), card("b", "")]);
        doc.settings.text = "残り".into();
        doc.settings.voice_id = Some(9);
        doc.settings.speed = 1.2;
        doc.settings.num_candidates = 4;
        doc.settings.seed = "7".into();
        assert_eq!(doc.validate(), Ok(()));
        assert_eq!(doc.settings.text, "");
        assert_eq!(doc.settings.voice_id, None);
        assert!((doc.settings.speed - 1.0).abs() < f64::EPSILON);
        assert_eq!(doc.settings.num_candidates, 1);
        assert_eq!(doc.settings.seed, "7", "shared settings are kept");
    }

    #[test]
    fn invalid_scripts_are_rejected() {
        let rejects = |doc: Script| {
            let mut doc = doc;
            assert!(doc.validate().is_err(), "accepted {doc:?}");
        };
        rejects(script(vec![]));
        rejects(script(vec![card("a", ""), card("a", "")]));
        rejects(script(vec![card("", "")]));
        rejects(script(vec![card(&"x".repeat(65), "")]));
        rejects(script(
            (0..=MAX_CARDS).map(|i| card(&i.to_string(), "")).collect(),
        ));
        rejects(script(vec![card("a", &"あ".repeat(MAX_TEXT_CHARS + 1))]));
        rejects(script(vec![Card {
            speed: 1.6,
            ..card("a", "")
        }]));
        rejects(script(vec![Card {
            silence: 10.5,
            ..card("a", "")
        }]));
        rejects(script(vec![Card {
            silence: f64::NAN,
            ..card("a", "")
        }]));
        let mut blank_pattern = script(vec![card("a", "")]);
        blank_pattern.options.pattern = " ".into();
        rejects(blank_pattern);
        let mut long_pattern = script(vec![card("a", "")]);
        long_pattern.options.pattern = "x".repeat(201);
        rejects(long_pattern);
        let full = script((0..MAX_CARDS).map(|i| card(&i.to_string(), "")).collect());
        let mut full = full;
        assert_eq!(full.validate(), Ok(()));
    }

    #[test]
    fn a_card_request_carries_its_text_voice_speed_and_silence() {
        let mut doc = script(vec![Card {
            speed: 1.25,
            silence: 2.0,
            ..card("a", "おはよう")
        }]);
        doc.dictionary_enabled = false;
        doc.settings.seed = "5".into();
        let req = doc.request(&doc.cards[0]);
        assert_eq!(req.text, "おはよう");
        assert_eq!(req.voice_id, Some(1));
        assert!((req.speed - 1.25).abs() < f64::EPSILON);
        assert!((req.silence - 2.0).abs() < f64::EPSILON);
        assert!(!req.dictionary_enabled);
        assert_eq!(req.num_candidates, 1);
        assert_eq!(req.seed, "5");
    }

    #[test]
    fn signatures_follow_the_applied_text_voice_speed_silence_and_settings() {
        let settings = GenerateRequest::default();
        let base = card("a", "Irodori");
        let sig = signature(&base, "いろどり", &settings);
        assert_eq!(sig, signature(&base, "いろどり", &settings));
        assert_eq!(
            sig,
            signature(&card("a", "いろどり"), "いろどり", &settings),
            "a dictionary edit that leaves the applied text alone needs no regeneration"
        );
        assert_ne!(sig, signature(&base, "イロドリ", &settings));
        for changed in [
            Card {
                voice_id: Some(2),
                ..base.clone()
            },
            Card {
                speed: 1.1,
                ..base.clone()
            },
            Card {
                silence: 0.5,
                ..base.clone()
            },
        ] {
            assert_ne!(sig, signature(&changed, "いろどり", &settings));
        }
        let other = GenerateRequest {
            seed: "1".into(),
            ..settings
        };
        assert_ne!(sig, signature(&base, "いろどり", &other));
    }

    #[test]
    fn statuses_combine_the_job_the_outcome_and_the_input() {
        let c = card("a", "text");
        let done = CardResult {
            generation_id: Some(1),
            audio: true,
            signature: "s".into(),
            error: String::new(),
        };
        let st = |card: &Card, voice: bool, job, outcome: Option<&CardResult>| {
            status(card, voice, job, outcome, "s")
        };
        assert_eq!(st(&c, true, None, None), Status::New);
        assert_eq!(st(&c, true, Some(Job::Queued), Some(&done)), Status::Queued);
        assert_eq!(st(&c, true, Some(Job::Running), None), Status::Running);
        assert_eq!(st(&c, true, None, Some(&done)), Status::Done);
        assert_eq!(
            status(&c, true, None, Some(&done), "other"),
            Status::Changed
        );
        assert_eq!(
            st(&card("a", " \n"), true, None, Some(&done)),
            Status::Empty
        );
        assert_eq!(st(&c, false, None, Some(&done)), Status::NoVoice);
        let failed_first = CardResult {
            error: "失敗".into(),
            ..CardResult::default()
        };
        assert_eq!(st(&c, true, None, Some(&failed_first)), Status::Failed);
        let failed_retry = CardResult {
            error: "失敗".into(),
            ..done.clone()
        };
        assert_eq!(
            status(&c, true, None, Some(&failed_retry), "other"),
            Status::Changed,
            "a failed retry keeps the earlier audio"
        );
        let deleted = CardResult {
            audio: false,
            ..done
        };
        assert_eq!(st(&c, true, None, Some(&deleted)), Status::New);
    }

    #[test]
    fn ready_lists_cards_in_order_or_names_the_missing_ones() {
        use Status::{Changed, Done, Empty, New};
        assert_eq!(ready(&[Done, Empty, Done]), Ok(vec![0, 2]));
        let missing = ready(&[Done, New, Empty, Changed]).unwrap_err();
        assert!(missing.contains("2・4"), "{missing}");
        assert!(ready(&[Empty, Empty]).is_err());
    }

    #[test]
    fn file_names_fill_the_pattern_and_are_safe() {
        assert_eq!(
            file_name(DEFAULT_PATTERN, 7, "ずんだ", "こんにちは", "20261015"),
            "007_ずんだ_20261015"
        );
        assert_eq!(
            file_name("$テキスト$", 1, "a", &"あ".repeat(50), "d"),
            "あ".repeat(40)
        );
        assert_eq!(
            file_name("$話者名$", 1, "a/b:c*?\"<>|", "", "d"),
            "a_b_c______"
        );
        assert_eq!(file_name("$話者名$. ", 1, "x", "", "d"), "x");
        assert_eq!(file_name("$テキスト$", 1, "x", "\n", "d"), "音声");
        assert_eq!(file_name("$話者名$", 1, "CON", "", "d"), "_CON");
        assert_eq!(file_name("固定", 1, "x", "", "d"), "固定");
        assert_eq!(
            file_name(&"$連番$".repeat(60), 1, "x", "", "d")
                .chars()
                .count(),
            100
        );
    }

    #[test]
    fn repeated_names_get_numbered_suffixes() {
        let names: Vec<String> = ["a", "A", "b", "a"].map(String::from).to_vec();
        assert_eq!(unique_names(&names), ["a", "A_001", "b", "a_002"]);
    }
}
