//! Generation requests: validation, the worker's `_run_generation` arguments, and messages.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const MODELS: &[(&str, &str)] = &[
    ("Aratako/Irodori-TTS-v4.1-Small", "ベース（標準モデル）"),
    (
        "phasefield-audio/Irodori-TTS-v4.1-Anime",
        "Anime（派生版のアニメ声強化モデル）",
    ),
];
pub const MODES: &[(&str, &str)] = &[
    ("design", "言葉で声を作る（ボイスデザイン）"),
    ("clone", "お手本の声に似せる（ボイスクローン）"),
    ("both", "お手本の声＋話し方を指定"),
    ("auto", "文章だけでおまかせ"),
];
const EMOJI_CATEGORIES: &[(&str, &[&str])] = &[
    (
        "ポジティブ",
        &[
            "楽しげ",
            "笑い",
            "喜び",
            "優しく",
            "安堵",
            "得意げ",
            "力強く",
        ],
    ),
    (
        "ネガティブ",
        &[
            "泣き声",
            "怒り",
            "心配",
            "慌てる",
            "震え声",
            "苦しげ",
            "悲鳴",
            "呆れ",
            "舌打ち",
        ],
    ),
    (
        "その他の感情",
        &["驚き", "疑問", "照れ", "からかう", "懇願", "眠そう", "酔う"],
    ),
    (
        "話し方・演出",
        &[
            "囁き",
            "早口",
            "ゆっくり",
            "勢いよく",
            "朗読",
            "間",
            "エコー",
            "電話越し",
            "相槌",
            "寝言",
            "口を塞ぐ",
        ],
    ),
    (
        "息・口などの音",
        &[
            "吐息",
            "息切れ",
            "息をのむ",
            "あくび",
            "喘ぎ",
            "咳・鼻",
            "リップノイズ",
            "舐める音",
            "飲み込む",
            "嗅ぐ音",
            "鼻歌",
        ],
    ),
];
const MAX_TEXT_CHARS: usize = 10_000;
const MAX_RAW_CHARS: usize = 500;
pub const MAX_REFERENCES: usize = 16;

/// What the browser sends. Missing fields take the Gradio version's initial values.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct GenerateRequest {
    pub model: String,
    pub mode: String,
    pub caption: String,
    pub reference_ids: Vec<i64>,
    pub text: String,
    pub dictionary_enabled: bool,
    pub speed: f64,
    pub num_steps: u32,
    pub num_candidates: u32,
    pub duration_scale: f64,
    pub seed: String,
    pub seconds: String,
    pub cfg_scale_text: f64,
    pub cfg_scale_caption: f64,
    pub cfg_scale_speaker: f64,
    pub model_device: String,
    pub model_precision: String,
    pub codec_device: String,
    pub codec_precision: String,
    pub t_schedule_mode: String,
    pub sway_coeff: f64,
    pub cfg_guidance_mode: String,
    pub context_kv_cache: bool,
    pub cfg_scale_raw: String,
    pub speaker_kv_scale_raw: String,
    pub max_text_len_raw: String,
    pub max_caption_len_raw: String,
    pub truncation_factor_raw: String,
    pub rescale_k_raw: String,
    pub rescale_sigma_raw: String,
    pub lora_adapter_raw: String,
    pub cfg_min_t: f64,
    pub cfg_max_t: f64,
}

impl Default for GenerateRequest {
    fn default() -> Self {
        Self {
            model: MODELS[0].0.into(),
            mode: "design".into(),
            caption: String::new(),
            reference_ids: Vec::new(),
            text: String::new(),
            dictionary_enabled: true,
            speed: 1.0,
            num_steps: 40,
            num_candidates: 1,
            duration_scale: 1.0,
            seed: String::new(),
            seconds: String::new(),
            cfg_scale_text: 3.0,
            cfg_scale_caption: 4.0,
            cfg_scale_speaker: 5.0,
            model_device: "cpu".into(),
            model_precision: "fp32".into(),
            codec_device: "cpu".into(),
            codec_precision: "fp32".into(),
            t_schedule_mode: "linear".into(),
            sway_coeff: -1.0,
            cfg_guidance_mode: "independent".into(),
            context_kv_cache: true,
            cfg_scale_raw: String::new(),
            speaker_kv_scale_raw: String::new(),
            max_text_len_raw: String::new(),
            max_caption_len_raw: String::new(),
            truncation_factor_raw: String::new(),
            rescale_k_raw: String::new(),
            rescale_sigma_raw: String::new(),
            lora_adapter_raw: String::new(),
            cfg_min_t: 0.5,
            cfg_max_t: 1.0,
        }
    }
}

/// Device choices reported by the worker.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Runtime {
    pub devices: Vec<String>,
    pub precisions: std::collections::BTreeMap<String, Vec<String>>,
    pub max_candidates: u32,
}

impl GenerateRequest {
    pub fn uses_references(&self) -> bool {
        matches!(self.mode.as_str(), "clone" | "both")
    }

    fn uses_caption(&self) -> bool {
        matches!(self.mode.as_str(), "design" | "both")
    }

    /// Checks the request before any work starts; the message is shown to the user.
    pub fn validate(&self, runtime: &Runtime) -> Result<(), String> {
        let known =
            |choices: &[(&str, &str)], value: &str| choices.iter().any(|(id, _)| *id == value);
        if !known(MODELS, &self.model) || !known(MODES, &self.mode) {
            return Err("モデルと声の作り方を一覧から選んでください。".into());
        }
        if self.text.trim().is_empty() {
            return Err("文章が空欄です。文章欄に読み上げたい文章を入力してください。".into());
        }
        if self.uses_references() && self.reference_ids.is_empty() {
            return Err(
                "お手本の音声がありません。声の設定に音声ファイルを追加してください。".into(),
            );
        }
        if self.reference_ids.len() > MAX_REFERENCES {
            return Err(format!("お手本の音声は{MAX_REFERENCES}件までです。"));
        }
        if self.text.chars().count() > MAX_TEXT_CHARS
            || self.caption.chars().count() > MAX_TEXT_CHARS
        {
            return Err(format!("文章と声の説明は{MAX_TEXT_CHARS}文字までです。"));
        }
        let raw = [
            &self.seed,
            &self.seconds,
            &self.cfg_scale_raw,
            &self.speaker_kv_scale_raw,
            &self.max_text_len_raw,
            &self.max_caption_len_raw,
            &self.truncation_factor_raw,
            &self.rescale_k_raw,
            &self.rescale_sigma_raw,
            &self.lora_adapter_raw,
        ];
        if raw
            .iter()
            .any(|value| value.chars().count() > MAX_RAW_CHARS)
        {
            return Err(format!("任意の入力欄は{MAX_RAW_CHARS}文字までです。"));
        }
        if !(0.75..=1.5).contains(&self.speed) {
            return Err("話速は0.75～1.50倍で指定してください。".into());
        }
        let ranges = [
            ("生成の計算回数", f64::from(self.num_steps), 1.0, 120.0),
            (
                "生成する候補数",
                f64::from(self.num_candidates),
                1.0,
                f64::from(runtime.max_candidates),
            ),
            ("生成する長さの倍率", self.duration_scale, 0.5, 1.5),
            ("文章への忠実さ", self.cfg_scale_text, 0.0, 10.0),
            ("声の説明の反映度", self.cfg_scale_caption, 0.0, 10.0),
            ("お手本の声の反映度", self.cfg_scale_speaker, 0.0, 10.0),
            ("Swayの調整値", self.sway_coeff, -1.0, 1.5),
        ];
        for (label, value, min, max) in ranges {
            if !(min..=max).contains(&value) {
                return Err(format!("{label}は{min}～{max}で指定してください。"));
            }
        }
        if !self.cfg_min_t.is_finite() || !self.cfg_max_t.is_finite() {
            return Err("条件反映の開始位置と終了位置は数値で指定してください。".into());
        }
        let supports = |device: &str, precision: &str| {
            runtime.devices.iter().any(|d| d == device)
                && runtime
                    .precisions
                    .get(device)
                    .is_some_and(|p| p.iter().any(|p| p == precision))
        };
        if !supports(&self.model_device, &self.model_precision)
            || !supports(&self.codec_device, &self.codec_precision)
        {
            return Err("使う機器と計算精度を一覧から選んでください。".into());
        }
        if !["linear", "sway"].contains(&self.t_schedule_mode.as_str())
            || !["independent", "joint", "alternating"].contains(&self.cfg_guidance_mode.as_str())
        {
            return Err("計算の進め方と条件を反映する方式を一覧から選んでください。".into());
        }
        Ok(())
    }

    /// Keyword arguments for upstream `_run_generation`.
    pub fn worker_params(&self, text_applied: &str, reference_paths: &[String]) -> Value {
        json!({
            "checkpoint": self.model,
            "model_device": self.model_device,
            "model_precision": self.model_precision,
            "codec_device": self.codec_device,
            "codec_precision": self.codec_precision,
            "text": text_applied,
            "caption": if self.uses_caption() { self.caption.as_str() } else { "" },
            "ref_wavs": if self.uses_references() { json!(reference_paths) } else { Value::Null },
            "num_steps": self.num_steps,
            "num_candidates": self.num_candidates,
            "seed_raw": self.seed,
            "seconds_raw": self.seconds,
            "duration_scale": self.duration_scale,
            "t_schedule_mode": self.t_schedule_mode,
            "sway_coeff": self.sway_coeff,
            "cfg_guidance_mode": self.cfg_guidance_mode,
            "cfg_scale_text": self.cfg_scale_text,
            "cfg_scale_caption": self.cfg_scale_caption,
            "cfg_scale_speaker": self.cfg_scale_speaker,
            "cfg_scale_raw": self.cfg_scale_raw,
            "cfg_min_t": self.cfg_min_t,
            "cfg_max_t": self.cfg_max_t,
            "context_kv_cache": self.context_kv_cache,
            "speaker_kv_scale_raw": self.speaker_kv_scale_raw,
            "max_text_len_raw": self.max_text_len_raw,
            "max_caption_len_raw": self.max_caption_len_raw,
            "truncation_factor_raw": self.truncation_factor_raw,
            "rescale_k_raw": self.rescale_k_raw,
            "rescale_sigma_raw": self.rescale_sigma_raw,
            "lora_adapter_raw": self.lora_adapter_raw,
        })
    }
}

/// Puts bf16 first wherever a device offers it, making it the default precision.
pub fn prefer_bf16(runtime: &mut Runtime) {
    for precisions in runtime.precisions.values_mut() {
        if let Some(index) = precisions.iter().position(|p| p == "bf16") {
            let bf16 = precisions.remove(index);
            precisions.insert(0, bf16);
        }
    }
}

/// Fills omitted device and precision fields from the runtime's first choices.
pub fn with_runtime_defaults(mut body: Value, runtime: &Runtime) -> Value {
    let Some(fields) = body.as_object_mut() else {
        return body;
    };
    for (device_key, precision_key) in [
        ("model_device", "model_precision"),
        ("codec_device", "codec_precision"),
    ] {
        if !fields.contains_key(device_key)
            && let Some(device) = runtime.devices.first()
        {
            fields.insert(device_key.into(), device.as_str().into());
        }
        let device = fields[device_key].as_str().unwrap_or_default().to_owned();
        if !fields.contains_key(precision_key)
            && let Some(precision) = runtime.precisions.get(&device).and_then(|p| p.first())
        {
            fields.insert(precision_key.into(), precision.as_str().into());
        }
    }
    body
}

/// Maps a worker exception to the Gradio version's user message.
pub fn error_message(error_type: &str, error: &str) -> &'static str {
    let error = error.to_lowercase();
    if error.contains("out of memory") {
        "GPUメモリが不足しました。他のAIアプリ等を閉じるか、候補数や文章の長さを減らして再度お試しください。"
    } else if ["connection", "download", "offline", "timeout"]
        .iter()
        .any(|word| error.contains(word))
    {
        "モデルの取得または通信でエラーが発生しました。インターネット接続を確認して再度お試しください。"
    } else if error_type == "ValueError" {
        "入力値を確認してください。数値欄には数値を入力し、任意の項目は空欄にできます。"
    } else {
        "生成できませんでした。入力や設定を確認してください。原因は下の「詳しい実行記録」で確認できます。"
    }
}

/// Groups upstream palette items into the Gradio version's tabs; unknown labels go last.
pub fn emoji_groups(items: &[Value]) -> Vec<Value> {
    let label = |item: &Value| item["label"].as_str().unwrap_or_default().to_owned();
    let mut used = std::collections::HashSet::new();
    let mut groups = Vec::new();
    for (title, labels) in EMOJI_CATEGORIES {
        let group: Vec<&Value> = labels
            .iter()
            .filter_map(|wanted| items.iter().find(|item| label(item) == *wanted))
            .filter(|item| used.insert(label(item)))
            .collect();
        if !group.is_empty() {
            groups.push(json!({"title": title, "items": group}));
        }
    }
    let rest: Vec<&Value> = items
        .iter()
        .filter(|item| !used.contains(&label(item)))
        .collect();
    if !rest.is_empty() {
        groups.push(json!({"title": "その他", "items": rest}));
    }
    groups
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runtime() -> Runtime {
        Runtime {
            devices: vec!["cuda".into(), "cpu".into()],
            precisions: [
                ("cuda".into(), vec!["fp32".into(), "bf16".into()]),
                ("cpu".into(), vec!["fp32".into()]),
            ]
            .into_iter()
            .collect(),
            max_candidates: 32,
        }
    }

    fn request() -> GenerateRequest {
        GenerateRequest {
            text: "こんにちは。".into(),
            ..GenerateRequest::default()
        }
    }

    fn rejects(change: impl FnOnce(&mut GenerateRequest)) {
        let mut req = request();
        change(&mut req);
        assert!(req.validate(&runtime()).is_err(), "accepted {req:?}");
    }

    #[test]
    fn defaults_are_valid() {
        assert_eq!(request().validate(&runtime()), Ok(()));
    }

    #[test]
    fn rejects_blank_text_and_missing_references() {
        rejects(|r| r.text = " \n".into());
        rejects(|r| r.mode = "clone".into());
        rejects(|r| r.mode = "both".into());
        let mut ok = request();
        ok.mode = "clone".into();
        ok.reference_ids = vec![1];
        assert_eq!(ok.validate(&runtime()), Ok(()));
    }

    #[test]
    fn rejects_unknown_choices() {
        rejects(|r| r.model = "someone/else".into());
        rejects(|r| r.mode = "sing".into());
        rejects(|r| r.model_device = "mps".into());
        rejects(|r| r.codec_precision = "bf16".into());
        rejects(|r| r.t_schedule_mode = "cosine".into());
        rejects(|r| r.cfg_guidance_mode = "mixed".into());
    }

    #[test]
    fn rejects_values_outside_the_slider_ranges() {
        rejects(|r| r.speed = 0.7);
        rejects(|r| r.speed = 1.55);
        rejects(|r| r.speed = f64::NAN);
        rejects(|r| r.num_steps = 0);
        rejects(|r| r.num_steps = 121);
        rejects(|r| r.num_candidates = 0);
        rejects(|r| r.num_candidates = 33);
        rejects(|r| r.duration_scale = 1.6);
        rejects(|r| r.cfg_scale_speaker = -0.1);
        rejects(|r| r.sway_coeff = 2.0);
        rejects(|r| r.cfg_min_t = f64::INFINITY);
        rejects(|r| r.text = "あ".repeat(MAX_TEXT_CHARS + 1));
        rejects(|r| r.lora_adapter_raw = "x".repeat(MAX_RAW_CHARS + 1));
        rejects(|r| r.reference_ids = vec![1; MAX_REFERENCES + 1]);
        let mut edge = request();
        edge.speed = 1.5;
        edge.num_steps = 120;
        edge.num_candidates = 32;
        edge.model_device = "cuda".into();
        edge.model_precision = "bf16".into();
        assert_eq!(edge.validate(&runtime()), Ok(()));
    }

    #[test]
    fn worker_params_match_the_upstream_signature() {
        let params = request().worker_params("こんにちは。", &[]);
        let mut keys: Vec<_> = params.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        let mut expected = vec![
            "checkpoint",
            "model_device",
            "model_precision",
            "codec_device",
            "codec_precision",
            "text",
            "caption",
            "ref_wavs",
            "num_steps",
            "num_candidates",
            "seed_raw",
            "seconds_raw",
            "duration_scale",
            "t_schedule_mode",
            "sway_coeff",
            "cfg_guidance_mode",
            "cfg_scale_text",
            "cfg_scale_caption",
            "cfg_scale_speaker",
            "cfg_scale_raw",
            "cfg_min_t",
            "cfg_max_t",
            "context_kv_cache",
            "speaker_kv_scale_raw",
            "max_text_len_raw",
            "max_caption_len_raw",
            "truncation_factor_raw",
            "rescale_k_raw",
            "rescale_sigma_raw",
            "lora_adapter_raw",
        ];
        expected.sort_unstable();
        assert_eq!(keys, expected);
    }

    #[test]
    fn bf16_becomes_the_first_precision_where_offered() {
        let mut rt = runtime();
        prefer_bf16(&mut rt);
        assert_eq!(rt.precisions["cuda"], ["bf16", "fp32"]);
        assert_eq!(rt.precisions["cpu"], ["fp32"]);
    }

    #[test]
    fn omitted_device_fields_take_the_runtime_defaults() {
        let mut rt = runtime();
        prefer_bf16(&mut rt);
        let filled = with_runtime_defaults(json!({"text": "a"}), &rt);
        assert_eq!(filled["model_device"], "cuda");
        assert_eq!(filled["codec_device"], "cuda");
        assert_eq!(filled["model_precision"], "bf16");
        assert_eq!(filled["codec_precision"], "bf16");
        let explicit = with_runtime_defaults(
            json!({"text": "a", "model_device": "cpu", "codec_precision": "fp32"}),
            &rt,
        );
        assert_eq!(explicit["model_device"], "cpu");
        assert_eq!(
            explicit["model_precision"], "fp32",
            "precision follows the chosen device"
        );
        assert_eq!(explicit["codec_device"], "cuda");
        assert_eq!(explicit["codec_precision"], "fp32");
        assert_eq!(
            with_runtime_defaults(json!("not an object"), &rt),
            json!("not an object")
        );
    }

    #[test]
    fn worker_params_drop_the_unused_voice_inputs() {
        let refs = vec!["/data/references/a.wav".to_owned()];
        let mut req = request();
        req.caption = "低い声".into();
        req.reference_ids = vec![1];
        for (mode, caption, ref_wavs) in [
            ("design", json!("低い声"), Value::Null),
            ("clone", json!(""), json!(refs)),
            ("both", json!("低い声"), json!(refs)),
            ("auto", json!(""), Value::Null),
        ] {
            req.mode = mode.into();
            let params = req.worker_params("いろどり", &refs);
            assert_eq!(params["caption"], caption, "{mode}");
            assert_eq!(params["ref_wavs"], ref_wavs, "{mode}");
            assert_eq!(params["text"], "いろどり");
            assert_eq!(params["checkpoint"], MODELS[0].0);
        }
    }

    #[test]
    fn error_messages_follow_the_gradio_version() {
        assert!(
            error_message("RuntimeError", "CUDA out of memory. Tried")
                .starts_with("GPUメモリが不足")
        );
        assert!(error_message("OSError", "Connection reset by peer").starts_with("モデルの取得"));
        assert!(error_message("ValueError", "seed must be an integer").starts_with("入力値を確認"));
        assert!(error_message("RuntimeError", "boom").starts_with("生成できませんでした"));
    }

    #[test]
    fn emoji_groups_follow_the_categories_and_keep_leftovers() {
        let items = vec![
            json!({"emoji": "🥱", "label": "あくび", "description": ""}),
            json!({"emoji": "😊", "label": "楽しげ", "description": ""}),
            json!({"emoji": "🆕", "label": "新顔", "description": ""}),
        ];
        let groups = emoji_groups(&items);
        let titles: Vec<_> = groups
            .iter()
            .map(|g| g["title"].as_str().unwrap())
            .collect();
        assert_eq!(titles, ["ポジティブ", "息・口などの音", "その他"]);
        assert_eq!(groups[0]["items"][0]["label"], "楽しげ");
        assert_eq!(groups[2]["items"][0]["emoji"], "🆕");
    }
}
