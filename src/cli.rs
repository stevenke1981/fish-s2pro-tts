//! Dependency-free, strict CLI parser. Normal command output is always JSON.
use crate::agent::{self, AgentError, AgentService, BatchOptions, Result, SpeechOptions};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, atomic::AtomicBool};

const HELP: &str = r#"Fish Audio TTS Studio - Agent CLI

Usage: fish-s2pro-tts-cli [--output-dir DIR] [--config FILE] COMMAND [OPTIONS]

Commands:
  status                    Local configuration, no network/key disclosure
  catalog                   App characters, emotion tags and story presets
  prepare                   Validate/preview a speech request without synthesis
  synthesize                Generate one audio file (synchronous)
  batch                     Generate sequential items from a JSON batch
  mcp [--stdio]             Start an MCP server on stdin/stdout

Speech options (prepare/synthesize):
  --text TEXT | --text-file FILE    UTF-8 input with optional Fish tags
  --output NAME                    Plain filename, e.g. narration.mp3
  --format mp3|wav|pcm              Extension must match; default mp3
  --model MODEL --voice VOICE_ID --speed NUMBER
  --character NAME --preset ID      Exact values from catalog
  --input FILE | --json JSON        Alternative speech request JSON
  --dry-run                        Preview without making a provider request

Batch options:
  --input FILE | --json JSON        Object with an items array of speech requests
  --dry-run                        Validate the ENTIRE batch without charges

Global options:
  --output-dir DIR          Output boundary (or FISH_TTS_OUTPUT_DIR)
  --config FILE             Explicit GUI config import; never auto-discovered
  --help                    This help
  --version                 Package version

Credentials: OPENROUTER_API_KEY (preferred), or --config's saved key.
No API key command-line option; no automatic retry, overwrite or paid fallback.
Exit codes: 0 success, 2 invalid input, 3 missing key, 4 provider/audio,
            5 filesystem, 6 batch stopped (see completed outputs in JSON).
MCP jobs are session-local. Keep stdin open while polling generation jobs.
"#;

pub fn run(args: Vec<String>) -> i32 {
    match execute(args) {
        Ok(code) => code,
        Err(error) => {
            let code = error.exit_code();
            eprintln!("{error}");
            // mcp startup failures belong on stderr, never on the protocol stream.
            code
        }
    }
}

fn execute(args: Vec<String>) -> Result<i32> {
    let mut command = None;
    let mut options = BTreeMap::new();
    let mut dry_run = false;
    let mut stdio = false;
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--help" | "-h" => { print!("{HELP}"); return Ok(0); }
            "--version" | "-V" => { println!("{}", env!("CARGO_PKG_VERSION")); return Ok(0); }
            "--dry-run" if !dry_run => dry_run = true,
            "--stdio" if !stdio => stdio = true,
            "--output-dir" | "--config" | "--text" | "--text-file" | "--output" | "--format"
            | "--model" | "--voice" | "--speed" | "--character" | "--preset" | "--input" | "--json" => {
                let value = iter.next().ok_or_else(|| AgentError::invalid(format!("Missing value for {arg}")))?;
                if options.insert(arg.clone(), value).is_some() { return Err(AgentError::invalid(format!("Duplicate option {arg}"))); }
            }
            _ if !arg.starts_with('-') && command.is_none() => command = Some(arg),
            _ => return Err(AgentError::invalid(format!("Unexpected argument: {arg}"))),
        }
    }
    let command = command.as_deref().unwrap_or("help");
    if command == "help" { print!("{HELP}"); return Ok(0); }
    if !["status", "catalog", "prepare", "synthesize", "batch", "mcp"].contains(&command) {
        return Err(AgentError::invalid("Unknown command; use --help"));
    }
    let output_dir = options.remove("--output-dir").map(PathBuf::from);
    let config = options.remove("--config").map(PathBuf::from);
    if stdio && command != "mcp" { return Err(AgentError::invalid("--stdio is only valid with mcp")); }
    if dry_run && !["prepare", "synthesize", "batch"].contains(&command) {
        return Err(AgentError::invalid("--dry-run is only valid for speech/batch commands"));
    }
    if ["status", "catalog", "mcp"].contains(&command) && !options.is_empty() {
        return Err(AgentError::invalid("Unexpected options for this command"));
    }
    let service = Arc::new(AgentService::from_environment(output_dir, config.as_deref())?);
    if command == "mcp" {
        crate::mcp::serve(std::io::stdin().lock(), std::io::stdout().lock(), service)
            .map_err(|e| AgentError::new("io_error", e.to_string()))?;
        return Ok(0);
    }
    let operation: Result<Value> = (|| match command {
        "status" => Ok(service.status()),
        "catalog" => Ok(service.catalog()),
        "batch" => {
            let input = input_json(&mut options)?;
            if !options.is_empty() { return Err(AgentError::invalid("batch only accepts --input/--json and --dry-run")); }
            let batch: BatchOptions = serde_json::from_value(input).map_err(|_| AgentError::invalid("Invalid batch JSON; expected {items: [...]}"))?;
            if dry_run { Ok(json!({"dry_run":true,"items":service.prepare_batch(&batch)?})) }
            else { service.run_batch(&batch) }
        }
        _ => {
            let speech = speech_options(&mut options)?;
            if dry_run || command == "prepare" { Ok(json!({"dry_run":true,"prepared":service.prepare(&speech)?})) }
            else { service.synthesize(&speech, &AtomicBool::new(false)) }
        }
    })();
    match operation {
        Ok(value) => {
            let ok = value.get("ok").and_then(Value::as_bool).unwrap_or(true);
            println!("{}", json!({"ok":ok,"result":value}));
            Ok(if ok { 0 } else { 6 })
        }
        Err(error) => {
            println!("{}", json!({"ok":false,"error":error}));
            Ok(error.exit_code())
        }
    }
}
fn input_json(options: &mut BTreeMap<String, String>) -> Result<Value> {
    let file = options.remove("--input");
    let inline = options.remove("--json");
    let text = match (file, inline) {
        (Some(path), None) => agent::read_utf8(Path::new(&path))?,
        (None, Some(text)) => text,
        _ => return Err(AgentError::invalid("Provide exactly one of --input FILE or --json JSON")),
    };
    if text.len() as u64 > agent::MAX_INPUT_BYTES { return Err(AgentError::invalid("JSON exceeds 1 MiB")); }
    serde_json::from_str(&text).map_err(|_| AgentError::invalid("Invalid request JSON"))
}
fn speech_options(options: &mut BTreeMap<String, String>) -> Result<SpeechOptions> {
    let mut value = if options.contains_key("--input") || options.contains_key("--json") {
        input_json(options)?
    } else { json!({}) };
    let object = value.as_object_mut().ok_or_else(|| AgentError::invalid("Speech request must be an object"))?;
    let text = options.remove("--text");
    let file = options.remove("--text-file");
    let text = match (text, file) {
        (Some(_), Some(_)) => return Err(AgentError::invalid("Use --text OR --text-file")),
        (Some(text), None) => Some(text),
        (None, Some(path)) => Some(agent::read_utf8(Path::new(&path))?),
        _ => None,
    };
    if let Some(text) = text { object.insert("text".into(), json!(text)); }
    for name in ["output", "format", "model", "voice", "character", "preset"] {
        if let Some(value) = options.remove(&format!("--{name}")) { object.insert(name.into(), json!(value)); }
    }
    if let Some(speed) = options.remove("--speed") {
        let speed = speed.parse::<f32>().map_err(|_| AgentError::invalid("Invalid numeric speed"))?;
        if !speed.is_finite() { return Err(AgentError::invalid("speed must be finite")); }
        object.insert("speed".into(), json!(speed));
    }
    if !options.is_empty() { return Err(AgentError::invalid("Unexpected speech options")); }
    serde_json::from_value(value).map_err(|_| AgentError::invalid("Speech requires text and output; unknown fields are rejected"))
}
