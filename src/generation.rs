//! Generation requests: validation, the engine's `_run_generation` arguments, and messages.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// What the server, the screen and the engine need to know about a model; nothing else repeats it.
#[derive(Debug, PartialEq, Serialize)]
pub struct Model {
    pub id: &'static str,
    pub label: &'static str,
    /// Steps used when the request leaves `num_steps` blank.
    pub steps: u32,
    /// `MeanFlow` checkpoints ignore CFG and the Sway schedule.
    pub meanflow: bool,
    /// Quantized checkpoints run only on CUDA in bf16.
    pub quantized: bool,
    /// Lowest CUDA compute capability the checkpoint runs on.
    pub min_cuda_capability: Option<(u32, u32)>,
}

const fn model(id: &'static str, label: &'static str, steps: u32) -> Model {
    Model {
        id,
        label,
        steps,
        meanflow: false,
        quantized: false,
        min_cuda_capability: None,
    }
}

pub const MODELS: &[Model] = &[
    model("Aratako/Irodori-TTS-v4.1-Small", "標準", 40),
    model("phasefield-audio/Irodori-TTS-v4.1-Anime", "Anime", 40),
    Model {
        meanflow: true,
        ..model("Aratako/Irodori-TTS-v4.1-Small-MF", "MF（高速）", 4)
    },
    model("Aratako/Irodori-TTS-v4-Large", "Large（通常版）", 40),
    Model {
        quantized: true,
        ..model(
            "Aratako/Irodori-TTS-v4-Large-Quantized/int8-weight-only",
            "Large（省メモリ・INT8）",
            40,
        )
    },
    Model {
        quantized: true,
        min_cuda_capability: Some((8, 0)),
        ..model(
            "Aratako/Irodori-TTS-v4-Large-Quantized/int4-weight-only",
            "Large（省メモリ・INT4）",
            40,
        )
    },
];

pub fn find_model(id: &str) -> Option<&'static Model> {
    MODELS.iter().find(|m| m.id == id)
}
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
pub const MAX_TEXT_CHARS: usize = 10_000;
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
    /// A registered voice; its model, speaking style and reference replace the fields above.
    pub voice_id: Option<i64>,
    pub text: String,
    pub dictionary_enabled: bool,
    pub speed: f64,
    /// Seconds of silence appended to the end.
    pub silence: f64,
    /// Blank takes the model's own step count.
    pub num_steps: Option<u32>,
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
            model: MODELS[0].id.into(),
            mode: "design".into(),
            caption: String::new(),
            reference_ids: Vec::new(),
            voice_id: None,
            text: String::new(),
            dictionary_enabled: true,
            speed: 1.0,
            silence: 0.0,
            num_steps: None,
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

/// Device choices reported by the engine.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Runtime {
    pub devices: Vec<String>,
    pub precisions: std::collections::BTreeMap<String, Vec<String>>,
    pub max_candidates: u32,
    /// Compute capability of the engine's CUDA device, when it has one.
    #[serde(default)]
    pub cuda_capability: Option<(u32, u32)>,
}

impl GenerateRequest {
    pub fn uses_references(&self) -> bool {
        matches!(self.mode.as_str(), "clone" | "both")
    }

    fn uses_caption(&self) -> bool {
        matches!(self.mode.as_str(), "design" | "both")
    }

    /// Takes the model, speaking style and reference from a registered voice.
    pub fn voiced_by(&mut self, model: &str, caption: &str) {
        model.clone_into(&mut self.model);
        caption.clone_into(&mut self.caption);
        self.mode = if caption.trim().is_empty() {
            "clone"
        } else {
            "both"
        }
        .into();
        self.reference_ids.clear();
    }

    /// Checks the request before any work starts and settles the model precision;
    /// the message is shown to the user. `references` counts the audio the engine will get.
    pub fn validate(&mut self, runtime: &Runtime, references: usize) -> Result<(), String> {
        let Some(model) = find_model(&self.model) else {
            return Err("モデルと声の作り方を一覧から選んでください。".into());
        };
        if !MODES.iter().any(|(id, _)| *id == self.mode) {
            return Err("モデルと声の作り方を一覧から選んでください。".into());
        }
        if self.text.trim().is_empty() {
            return Err("文章が空欄です。文章欄に読み上げたい文章を入力してください。".into());
        }
        if self.uses_references() && references == 0 {
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
        if !(0.0..=10.0).contains(&self.silence) {
            return Err("末尾の無音は0～10秒で指定してください。".into());
        }
        let ranges = [
            (
                "生成の計算回数",
                f64::from(self.num_steps.unwrap_or(model.steps)),
                1.0,
                120.0,
            ),
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
        self.model_precision =
            resolve_model_precision(model, &self.model_device, &self.model_precision, runtime)?;
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

    /// Keyword arguments for upstream `_run_generation`, except `ref_wavs`, which the engine
    /// fills with its copies of the references sent beside them.
    pub fn engine_params(&self, text_applied: &str) -> Value {
        let steps = self
            .num_steps
            .or_else(|| find_model(&self.model).map(|m| m.steps));
        json!({
            "checkpoint": self.model,
            "model_device": self.model_device,
            "model_precision": self.model_precision,
            "codec_device": self.codec_device,
            "codec_precision": self.codec_precision,
            "text": text_applied,
            "caption": if self.uses_caption() { self.caption.as_str() } else { "" },
            "num_steps": steps,
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

/// The model precision to run with. Quantized checkpoints need CUDA with bf16, and some a
/// newer GPU (`EasyIrodoriTTS` v2.0 `resolve_model_precision`); other models keep the request.
pub fn resolve_model_precision(
    model: &Model,
    device: &str,
    requested: &str,
    runtime: &Runtime,
) -> Result<String, String> {
    if !model.quantized {
        return Ok(requested.to_owned());
    }
    let bf16 = runtime
        .precisions
        .get(device)
        .is_some_and(|p| p.iter().any(|p| p == "bf16"));
    if device != "cuda" || !runtime.devices.iter().any(|d| d == "cuda") {
        return Err("量子化版はNVIDIA GPU専用です。通常版を選ぶか、音声生成に使う機器をcudaにしてください。".into());
    }
    if !bf16 {
        return Err("量子化版にはBF16対応のNVIDIA GPUが必要です。通常版を選んでください。".into());
    }
    if let Some(minimum) = model.min_cuda_capability
        && runtime.cuda_capability.is_none_or(|have| have < minimum)
    {
        return Err(format!(
            "{}にはAmpere以降（RTX 30シリーズ以降が目安）のNVIDIA GPUが必要です。INT8版または通常版を選んでください。",
            model.label
        ));
    }
    Ok("bf16".into())
}

/// Trims and checks a voice name.
pub fn voice_name(raw: &str) -> Result<String, &'static str> {
    let name = raw.trim();
    if name.is_empty() || name.chars().count() > 80 || name.chars().any(char::is_control) {
        return Err("話者名を1行・1～80文字で入力してください。");
    }
    Ok(name.to_owned())
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

/// Maps an engine exception to the Gradio version's user message.
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
            cuda_capability: Some((8, 6)),
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
        assert!(req.validate(&runtime(), 1).is_err(), "accepted {req:?}");
    }

    #[test]
    fn defaults_are_valid() {
        assert_eq!(request().validate(&runtime(), 0), Ok(()));
    }

    #[test]
    fn rejects_blank_text_and_missing_references() {
        rejects(|r| r.text = " \n".into());
        for mode in ["clone", "both"] {
            let mut req = request();
            req.mode = mode.into();
            assert!(req.validate(&runtime(), 0).is_err(), "{mode}");
            assert_eq!(req.validate(&runtime(), 1), Ok(()), "{mode}");
        }
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
        rejects(|r| r.silence = -0.1);
        rejects(|r| r.silence = 10.5);
        rejects(|r| r.silence = f64::NAN);
        rejects(|r| r.num_steps = Some(0));
        rejects(|r| r.num_steps = Some(121));
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
        edge.silence = 10.0;
        edge.num_steps = Some(120);
        edge.num_candidates = 32;
        edge.model_device = "cuda".into();
        edge.model_precision = "bf16".into();
        assert_eq!(edge.validate(&runtime(), 0), Ok(()));
    }

    #[test]
    fn engine_params_match_the_upstream_signature_without_ref_wavs() {
        let params = request().engine_params("こんにちは。");
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
    fn blank_steps_take_the_model_default() {
        let mut req = request();
        assert_eq!(req.engine_params("a")["num_steps"], 40);
        req.model = "Aratako/Irodori-TTS-v4.1-Small-MF".into();
        assert_eq!(req.engine_params("a")["num_steps"], 4);
        req.num_steps = Some(8);
        assert_eq!(req.engine_params("a")["num_steps"], 8);
    }

    #[test]
    fn every_model_has_one_entry_and_meanflow_runs_four_steps() {
        let ids: std::collections::HashSet<_> = MODELS.iter().map(|m| m.id).collect();
        assert_eq!(ids.len(), MODELS.len());
        let mf = find_model("Aratako/Irodori-TTS-v4.1-Small-MF").unwrap();
        assert!(mf.meanflow && mf.steps == 4);
        assert!(find_model("nobody/model").is_none());
    }

    #[test]
    fn quantized_models_resolve_to_bf16_on_capable_cuda_only() {
        let int8 = find_model("Aratako/Irodori-TTS-v4-Large-Quantized/int8-weight-only").unwrap();
        let int4 = find_model("Aratako/Irodori-TTS-v4-Large-Quantized/int4-weight-only").unwrap();
        let large = find_model("Aratako/Irodori-TTS-v4-Large").unwrap();
        let rt = runtime();
        assert_eq!(
            resolve_model_precision(int8, "cuda", "fp32", &rt),
            Ok("bf16".into())
        );
        assert_eq!(
            resolve_model_precision(int4, "cuda", "fp32", &rt),
            Ok("bf16".into())
        );
        assert_eq!(
            resolve_model_precision(large, "cpu", "fp32", &rt),
            Ok("fp32".into()),
            "unquantized models keep the request"
        );
        assert!(resolve_model_precision(int8, "cpu", "fp32", &rt).is_err());

        let turing = Runtime {
            cuda_capability: Some((7, 5)),
            ..runtime()
        };
        assert_eq!(
            resolve_model_precision(int8, "cuda", "fp32", &turing),
            Ok("bf16".into())
        );
        assert!(resolve_model_precision(int4, "cuda", "fp32", &turing).is_err());
        let unknown = Runtime {
            cuda_capability: None,
            ..runtime()
        };
        assert!(resolve_model_precision(int4, "cuda", "fp32", &unknown).is_err());

        let no_bf16 = Runtime {
            precisions: [("cuda".into(), vec!["fp32".into()])].into_iter().collect(),
            ..runtime()
        };
        assert!(resolve_model_precision(int8, "cuda", "fp32", &no_bf16).is_err());
        let cpu_only = Runtime {
            devices: vec!["cpu".into()],
            precisions: [("cpu".into(), vec!["fp32".into()])].into_iter().collect(),
            max_candidates: 32,
            cuda_capability: None,
        };
        assert!(resolve_model_precision(int8, "cpu", "fp32", &cpu_only).is_err());
    }

    #[test]
    fn validation_settles_the_quantized_precision() {
        let mut req = request();
        req.model = "Aratako/Irodori-TTS-v4-Large-Quantized/int8-weight-only".into();
        req.model_device = "cuda".into();
        req.model_precision = "fp32".into();
        assert_eq!(req.validate(&runtime(), 0), Ok(()));
        assert_eq!(req.model_precision, "bf16");
    }

    #[test]
    fn a_voice_sets_model_style_and_mode() {
        let mut req = request();
        req.reference_ids = vec![1];
        req.voiced_by("Aratako/Irodori-TTS-v4.1-Small-MF", "明るく");
        assert_eq!(
            (req.model.as_str(), req.caption.as_str(), req.mode.as_str()),
            ("Aratako/Irodori-TTS-v4.1-Small-MF", "明るく", "both")
        );
        assert!(req.reference_ids.is_empty());
        req.voiced_by("Aratako/Irodori-TTS-v4.1-Small", " ");
        assert_eq!(req.mode, "clone");
    }

    #[test]
    fn voice_names_are_trimmed_single_line_and_limited() {
        assert_eq!(voice_name("  ずんだ  "), Ok("ずんだ".to_owned()));
        assert_eq!(voice_name(&"あ".repeat(80)), Ok("あ".repeat(80)));
        for bad in ["", "   ", "a\nb", "a\tb", &"あ".repeat(81)] {
            assert!(voice_name(bad).is_err(), "accepted {bad:?}");
        }
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
    fn engine_params_drop_the_unused_caption() {
        let mut req = request();
        req.caption = "低い声".into();
        req.reference_ids = vec![1];
        for (mode, caption) in [
            ("design", json!("低い声")),
            ("clone", json!("")),
            ("both", json!("低い声")),
            ("auto", json!("")),
        ] {
            req.mode = mode.into();
            let params = req.engine_params("いろどり");
            assert_eq!(params["caption"], caption, "{mode}");
            assert_eq!(params["text"], "いろどり");
            assert_eq!(params["checkpoint"], MODELS[0].id);
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
