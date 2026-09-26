//! Reading dictionary: literal replacements applied to the text before synthesis.

pub type Entry = (String, String);

/// Replaces every match, preferring the longest word at each position.
/// Replacement output is never processed again.
pub fn apply(text: &str, entries: &[Entry]) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(ch) = rest.chars().next() {
        let longest = entries
            .iter()
            .filter(|(word, _)| rest.starts_with(word.as_str()))
            .max_by_key(|(word, _)| word.len());
        if let Some((word, reading)) = longest {
            out.push_str(reading);
            rest = &rest[word.len()..];
        } else {
            out.push(ch);
            rest = &rest[ch.len_utf8()..];
        }
    }
    out
}

/// Trims and checks one entry, mirroring the Gradio version's limits.
pub fn validate(word: &str, reading: &str) -> Result<Entry, &'static str> {
    let (word, reading) = (word.trim(), reading.trim());
    if word.is_empty() || reading.is_empty() {
        return Err("「表記」と「読み方」の両方を入力してください。");
    }
    let multiline = |s: &str| s.contains(['\n', '\r']);
    if word.chars().count() > 100
        || reading.chars().count() > 200
        || multiline(word)
        || multiline(reading)
    {
        return Err("1行で入力してください。表記は100文字、読み方は200文字までです。");
    }
    Ok((word.to_owned(), reading.to_owned()))
}

/// Reads the phase-1 `config/reading_dictionary.json` object, keeping its order.
pub fn parse_legacy_json(json: &str) -> Result<Vec<Entry>, &'static str> {
    const INVALID: &str = "config/reading_dictionary.json is not an object of non-empty strings";
    let value: serde_json::Value =
        serde_json::from_str(json.trim_start_matches('\u{feff}')).map_err(|_| INVALID)?;
    let object = value.as_object().ok_or(INVALID)?;
    object
        .iter()
        .map(|(word, reading)| match reading.as_str() {
            Some(reading) if !word.trim().is_empty() && !reading.trim().is_empty() => {
                Ok((word.clone(), reading.to_owned()))
            }
            _ => Err(INVALID),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries(pairs: &[(&str, &str)]) -> Vec<Entry> {
        pairs
            .iter()
            .map(|(w, r)| ((*w).to_owned(), (*r).to_owned()))
            .collect()
    }

    #[test]
    fn longest_word_wins_and_output_is_not_replaced_again() {
        let dict = entries(&[
            ("日本", "にほん"),
            ("日本橋", "にほんばし"),
            ("にほん", "ニッポン"),
        ]);
        assert_eq!(apply("日本橋と日本", &dict), "にほんばしとにほん");
    }

    #[test]
    fn replacement_is_case_sensitive_and_literal() {
        let dict = entries(&[("Irodori", "いろどり"), ("a.b", "えーびー")]);
        assert_eq!(
            apply("irodori Irodori axb a.b", &dict),
            "irodori いろどり axb えーびー"
        );
    }

    #[test]
    fn empty_dictionary_keeps_text() {
        assert_eq!(apply("そのまま", &[]), "そのまま");
    }

    #[test]
    fn validate_trims_and_rejects_blank_or_multiline() {
        assert_eq!(
            validate(" TTS ", " てぃーてぃーえす "),
            Ok(entries(&[("TTS", "てぃーてぃーえす")]).remove(0))
        );
        assert!(validate("", "よみ").is_err());
        assert!(validate("語", "  ").is_err());
        assert!(validate("a\nb", "よみ").is_err());
        assert!(validate("語", "よ\rみ").is_err());
    }

    #[test]
    fn validate_limits_length_in_characters() {
        assert!(validate(&"語".repeat(100), &"よ".repeat(200)).is_ok());
        assert!(validate(&"語".repeat(101), "よみ").is_err());
        assert!(validate("語", &"よ".repeat(201)).is_err());
    }

    #[test]
    fn legacy_json_keeps_order_and_rejects_invalid_entries() {
        let parsed =
            parse_legacy_json("\u{feff}{\"TTS\": \"てぃーてぃーえす\", \"Irodori\": \"いろどり\"}");
        assert_eq!(
            parsed,
            Ok(entries(&[
                ("TTS", "てぃーてぃーえす"),
                ("Irodori", "いろどり")
            ]))
        );
        for invalid in [
            "[]",
            "{\"a\": 1}",
            "{\" \": \"よみ\"}",
            "{\"a\": \"\"}",
            "not json",
        ] {
            assert!(parse_legacy_json(invalid).is_err(), "accepted {invalid}");
        }
    }
}
