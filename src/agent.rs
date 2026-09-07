//! Headless application service shared by CLI and MCP. No GUI, audio device or shell.
use crate::api::{DEFAULT_ENDPOINT, DEFAULT_MODEL, OpenRouterClient, PRO_MODEL, SpeechRequest};
use crate::config::AppConfig;
use crate::{models, storytelling};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

pub const MAX_INPUT_BYTES: u64 = 1_048_576;
pub const MAX_TEXT_CHARS: usize = 4_000;
pub const MAX_BATCH_ITEMS: usize = 100;

#[derive(Debug, Clone, Serialize)]
pub struct AgentError {
    pub code: &'static str,
    pub message: String,
}
impl AgentError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }
    pub fn invalid(message: impl Into<String>) -> Self { Self::new("invalid_input", message) }
    pub fn exit_code(&self) -> i32 {
        match self.code {
            "missing_api_key" => 3,
            "upstream_error" | "invalid_audio" => 4,
            "io_error" | "output_exists" => 5,
            _ => 2,
        }
    }
}
impl std::fmt::Display for AgentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for AgentError {}
pub type Result<T> = std::result::Result<T, AgentError>;
fn io_error(e: std::io::Error) -> AgentError { AgentError::new("io_error", e.to_string()) }

/// Secret fields are deliberately absent; keys belong to the trusted launch environment.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpeechOptions {
    pub text: String,
    pub output: String,
    pub model: Option<String>,
    pub voice: Option<String>,
    pub format: Option<String>,
    pub speed: Option<f32>,
    pub character: Option<String>,
    pub preset: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BatchOptions { pub items: Vec<SpeechOptions> }

#[derive(Clone, Debug, Serialize)]
pub struct PreparedSpeech {
    pub request: SpeechRequest,
    pub output: String,
    pub text_chars: usize,
}

/// Injectable for offline tests; the executable never offers a mock transport switch.
pub trait SpeechBackend: Send + Sync {
    fn synthesize(&self, key: &str, request: &SpeechRequest) -> std::result::Result<Vec<u8>, String>;
}
struct OpenRouterBackend;
impl SpeechBackend for OpenRouterBackend {
    fn synthesize(&self, key: &str, request: &SpeechRequest) -> std::result::Result<Vec<u8>, String> {
        OpenRouterClient::new().synthesize(key, request)
    }
}

pub struct AgentService {
    root: PathBuf,
    config: AppConfig,
    key: String,
    backend: Arc<dyn SpeechBackend>,
}
impl AgentService {
    pub fn from_environment(output_dir: Option<PathBuf>, config_path: Option<&Path>) -> Result<Self> {
        // Never silently discover a plaintext key in an agent's arbitrary current directory.
        let config: AppConfig = match config_path {
            Some(path) => serde_json::from_str(&read_utf8(path)?)
                .map_err(|_| AgentError::invalid("Invalid GUI configuration JSON"))?,
            None => AppConfig::default(),
        };
        let key = std::env::var("OPENROUTER_API_KEY").ok()
            .filter(|s| !s.trim().is_empty()).unwrap_or_else(|| config.api_key.clone());
        let root = output_dir.or_else(|| std::env::var_os("FISH_TTS_OUTPUT_DIR").map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("fish_tts_output"));
        Self::with_backend(root, config, key, Arc::new(OpenRouterBackend))
    }

    pub fn with_backend(root: PathBuf, mut config: AppConfig, key: String, backend: Arc<dyn SpeechBackend>) -> Result<Self> {
        fs::create_dir_all(&root).map_err(io_error)?;
        let root = root.canonicalize().map_err(io_error)?;
        config.api_key.clear();
        Ok(Self { root, config, key: key.trim().to_owned(), backend })
    }

    pub fn status(&self) -> Value {
        json!({"version": env!("CARGO_PKG_VERSION"), "agent_api_version": 1,
            "api_key_configured": !self.key.is_empty(), "output_dir": self.root,
            "endpoint": DEFAULT_ENDPOINT, "network_checked": false,
            "max_text_chars": MAX_TEXT_CHARS, "max_batch_items": MAX_BATCH_ITEMS,
            "gui_required": false, "automatic_retry": false})
    }

    pub fn catalog(&self) -> Value {
        let tones: Vec<Value> = models::get_tone_tags().into_iter().map(|t|
            json!({"tag": t.tag, "label": t.label, "description": t.description,
                "category": format!("{:?}", t.category)})).collect();
        json!({"models": [DEFAULT_MODEL, PRO_MODEL], "models_source": "built_in_not_live",
            "characters": models::get_default_characters(), "tones": tones,
            "story_presets": storytelling::get_story_presets(), "formats": ["mp3", "wav", "pcm"],
            "note": "Presets are prompting hints, not guaranteed provider voices. Model/format availability depends on the provider."})
    }

    pub fn prepare(&self, options: &SpeechOptions) -> Result<PreparedSpeech> {
        if options.text.trim().is_empty() { return Err(AgentError::invalid("text must not be empty")); }
        let format = options.format.as_deref().unwrap_or(&self.config.response_format);
        if !["mp3", "wav", "pcm"].contains(&format) {
            return Err(AgentError::invalid("format must be mp3, wav or pcm"));
        }
        validate_output_name(&options.output, format)?;
        let mut input = options.text.clone();
        let mut speed = options.speed.unwrap_or(self.config.speed);
        let mut voice = options.voice.clone().or_else(|| {
            let v = self.config.custom_voice_id.trim();
            (!v.is_empty()).then(|| v.to_owned())
        });
        if let Some(name) = &options.character {
            let character = models::get_default_characters().into_iter().find(|c| &c.name == name)
                .ok_or_else(|| AgentError::invalid("Unknown character; use catalog"))?;
            if !character.prompt_tag.is_empty() && !input.contains(&character.prompt_tag) {
                input = format!("{} {}", character.prompt_tag, input);
            }
            if options.speed.is_none() { speed = character.recommended_speed; }
            if options.voice.is_none() && character.voice_id.is_some() { voice = character.voice_id; }
        }
        if let Some(id) = &options.preset {
            let preset = storytelling::get_story_presets().into_iter().find(|p| &p.id == id)
                .ok_or_else(|| AgentError::invalid("Unknown story preset; use catalog"))?;
            let tags = preset.base_tags.iter().map(|t| format!("[{t}]")).collect::<Vec<_>>().join(" ");
            input = format!("{tags} {input}");
            if options.speed.is_none() { speed = preset.recommended_speed; }
        }
        let text_chars = input.chars().count();
        if text_chars > MAX_TEXT_CHARS {
            return Err(AgentError::invalid("Prepared text exceeds 4000 characters; split into batch items"));
        }
        if !speed.is_finite() || !(0.25..=4.0).contains(&speed) {
            return Err(AgentError::invalid("speed must be finite and between 0.25 and 4.0"));
        }
        let model = options.model.as_deref().unwrap_or(&self.config.model).trim();
        if model.is_empty() || model.len() > 200 || model.chars().any(char::is_control) {
            return Err(AgentError::invalid("Invalid model identifier"));
        }
        if let Some(v) = &voice {
            if v.trim().is_empty() || v.len() > 512 || v.chars().any(char::is_control) {
                return Err(AgentError::invalid("Invalid voice identifier"));
            }
        }
        Ok(PreparedSpeech { request: SpeechRequest { model: model.to_owned(), input, voice,
            response_format: Some(format.to_owned()), speed: Some(speed) },
            output: options.output.clone(), text_chars })
    }

    pub fn prepare_batch(&self, batch: &BatchOptions) -> Result<Vec<PreparedSpeech>> {
        if batch.items.is_empty() || batch.items.len() > MAX_BATCH_ITEMS {
            return Err(AgentError::invalid("batch must contain 1..100 items"));
        }
        let mut names = HashSet::new();
        let mut prepared = Vec::new();
        let mut total = 0;
        for item in &batch.items {
            let p = self.prepare(item)?;
            if !names.insert(p.output.to_lowercase()) { return Err(AgentError::invalid("Duplicate output name in batch")); }
            total += p.text_chars;
            prepared.push(p);
        }
        if total > 100_000 { return Err(AgentError::invalid("batch exceeds 100000 prepared characters")); }
        Ok(prepared)
    }

    pub fn check_ready(&self, prepared: &[PreparedSpeech]) -> Result<()> {
        if self.key.is_empty() {
            return Err(AgentError::new("missing_api_key", "Set OPENROUTER_API_KEY or explicitly pass --config"));
        }
        for p in prepared {
            match fs::symlink_metadata(self.root.join(&p.output)) {
                Ok(_) => return Err(AgentError::new("output_exists", "Output already exists; choose a new filename")),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {},
                Err(e) => return Err(io_error(e)),
            }
        }
        Ok(())
    }

    pub fn synthesize(&self, options: &SpeechOptions, cancelled: &AtomicBool) -> Result<Value> {
        let p = self.prepare(options)?;
        self.check_ready(std::slice::from_ref(&p))?;
        if cancelled.load(Ordering::SeqCst) { return Err(AgentError::new("cancelled", "Job cancelled")); }
        let path = self.root.join(&p.output);
        // create_new is an atomic no-overwrite reservation, including dangling symlinks.
        let mut file = OpenOptions::new().write(true).create_new(true).open(&path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::AlreadyExists {
                AgentError::new("output_exists", "Output already exists; no API call was made")
            } else { io_error(e) }
        })?;
        let result = (|| {
            let bytes = self.backend.synthesize(&self.key, &p.request).map_err(|message| {
                let safe = message.replace(&self.key, "[REDACTED]");
                AgentError::new("upstream_error", safe.chars().take(1000).collect::<String>())
            })?;
            if cancelled.load(Ordering::SeqCst) {
                return Err(AgentError::new("cancelled", "Result discarded; the provider may still charge for the request"));
            }
            let format = p.request.response_format.as_deref().unwrap_or("mp3");
            validate_audio(&bytes, format)?;
            file.write_all(&bytes).and_then(|_| file.sync_all()).map_err(io_error)?;
            Ok(json!({"output": p.output, "path": path, "bytes": bytes.len(),
                "format": format, "model": p.request.model, "text_chars": p.text_chars}))
        })();
        drop(file);
        if result.is_err() { let _ = fs::remove_file(path); }
        result
    }

    /// Validate the entire batch before any paid request; stop on the first failure.
    pub fn run_batch(&self, batch: &BatchOptions) -> Result<Value> {
        let prepared = self.prepare_batch(batch)?;
        self.check_ready(&prepared)?;
        let mut outputs = Vec::new();
        for (index, options) in batch.items.iter().enumerate() {
            match self.synthesize(options, &AtomicBool::new(false)) {
                Ok(value) => outputs.push(value),
                Err(error) => return Ok(json!({"ok": false, "status": "failed", "outputs": outputs,
                    "failed_index": index, "error": error, "automatic_retry": false})),
            }
        }
        Ok(json!({"ok": true, "status": "completed", "outputs": outputs}))
    }
}

pub fn read_utf8(path: &Path) -> Result<String> {
    let mut bytes = Vec::new();
    File::open(path).map_err(io_error)?.take(MAX_INPUT_BYTES + 1).read_to_end(&mut bytes).map_err(io_error)?;
    if bytes.len() as u64 > MAX_INPUT_BYTES { return Err(AgentError::invalid("Input file exceeds 1 MiB")); }
    let text = String::from_utf8(bytes).map_err(|_| AgentError::invalid("Input must be UTF-8"))?;
    Ok(text.trim_start_matches('\u{feff}').to_owned())
}

pub fn validate_output_name(name: &str, format: &str) -> Result<()> {
    let bad = name.is_empty() || name.len() > 180 || name.starts_with('.') || name.ends_with('.')
        || name.ends_with(' ') || name.chars().any(|c| c.is_control() || "/\\:<>\"|?*".contains(c));
    let stem = name.split('.').next().unwrap_or("").to_ascii_uppercase();
    let reserved = ["CON", "PRN", "AUX", "NUL", "CLOCK$"].contains(&stem.as_str())
        || (stem.len() == 4 && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.as_bytes()[3].is_ascii_digit());
    if bad || reserved || !name.ends_with(&format!(".{format}")) {
        return Err(AgentError::invalid("output must be a plain, portable filename with a matching extension (no directory, overwrite or traversal)"));
    }
    Ok(())
}

fn validate_audio(bytes: &[u8], format: &str) -> Result<()> {
    let valid = match format {
        "mp3" => bytes.starts_with(b"ID3") || (bytes.len() >= 2 && bytes[0] == 0xff && bytes[1] & 0xe0 == 0xe0),
        "wav" => bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WAVE",
        "pcm" => !bytes.is_empty() && bytes.len() % 2 == 0
            && serde_json::from_slice::<Value>(bytes).is_err() && !bytes.starts_with(b"<"),
        _ => false,
    };
    if !valid || bytes.len() > 64 * 1024 * 1024 {
        return Err(AgentError::new("invalid_audio", "Provider returned invalid, mismatched or oversized audio"));
    }
    Ok(())
}
