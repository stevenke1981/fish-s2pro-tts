//! Tools-only MCP stdio adapter. Supported revisions are explicitly negotiated.
//! Long speech work uses application jobs, not the experimental MCP Tasks capability.
use crate::agent::{AgentError, AgentService, BatchOptions, MAX_INPUT_BYTES, Result, SpeechOptions};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::io::{BufRead, Read, Write};
use std::sync::{Arc, Mutex, MutexGuard, atomic::{AtomicBool, AtomicU64, Ordering}};
use std::thread::JoinHandle;

pub const PROTOCOL_VERSIONS: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26"];
const TOOL_NAMES: &[&str] = &["fish_status", "fish_catalog", "fish_prepare", "fish_synthesize",
    "fish_batch_synthesize", "fish_job_status", "fish_job_cancel"];
fn lock<T>(value: &Mutex<T>) -> MutexGuard<'_, T> { value.lock().unwrap_or_else(|e| e.into_inner()) }

#[derive(Clone, Serialize)]
struct JobView {
    job_id: String,
    status: String,
    total: usize,
    completed: usize,
    outputs: Vec<Value>,
    cancellation_requested: bool,
    error: Option<AgentError>,
    failed_index: Option<usize>,
}
impl JobView {
    fn active(&self) -> bool { ["queued", "running", "cancelling"].contains(&self.status.as_str()) }
}
struct Job { view: Mutex<JobView>, cancel: AtomicBool }

pub struct Jobs {
    service: Arc<AgentService>,
    entries: Mutex<BTreeMap<String, Arc<Job>>>,
    workers: Mutex<Vec<JoinHandle<()>>>,
    sequence: AtomicU64,
}
impl Jobs {
    pub fn new(service: Arc<AgentService>) -> Self {
        Self { service, entries: Mutex::new(BTreeMap::new()), workers: Mutex::new(Vec::new()),
            sequence: AtomicU64::new(1) }
    }
    pub fn start(&self, batch: BatchOptions) -> Result<Value> {
        let prepared = self.service.prepare_batch(&batch)?;
        self.service.check_ready(&prepared)?;
        let mut entries = lock(&self.entries);
        if entries.values().any(|j| lock(&j.view).active()) {
            return Err(AgentError::new("busy", "One generation job is already active; poll its status before starting another"));
        }
        // Bound session memory. Job IDs are session-local; completed jobs are not resumable.
        if entries.len() >= 32 {
            if let Some(id) = entries.keys().next().cloned() { entries.remove(&id); }
        }
        lock(&self.workers).retain(|worker| !worker.is_finished());
        let id = format!("job-{:016}", self.sequence.fetch_add(1, Ordering::SeqCst));
        let job = Arc::new(Job { cancel: AtomicBool::new(false), view: Mutex::new(JobView {
            job_id: id.clone(), status: "queued".into(), total: batch.items.len(), completed: 0,
            outputs: Vec::new(), cancellation_requested: false, error: None, failed_index: None,
        }) });
        let initial = json!(&*lock(&job.view));
        entries.insert(id.clone(), Arc::clone(&job));
        let service = Arc::clone(&self.service);
        let worker = std::thread::Builder::new().name("fish-tts-job".into()).spawn(move || {
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                for (index, options) in batch.items.iter().enumerate() {
                    if job.cancel.load(Ordering::SeqCst) {
                        lock(&job.view).status = "cancelled".into();
                        return;
                    }
                    { let mut view = lock(&job.view);
                      if !view.cancellation_requested { view.status = "running".into(); } }
                    match service.synthesize(options, &job.cancel) {
                        Ok(output) => {
                            let mut view = lock(&job.view);
                            view.outputs.push(output);
                            view.completed += 1;
                        }
                        Err(error) => {
                            let mut view = lock(&job.view);
                            view.status = if error.code == "cancelled" { "cancelled" } else { "failed" }.into();
                            view.error = Some(error);
                            view.failed_index = Some(index);
                            return;
                        }
                    }
                }
                lock(&job.view).status = "completed".into();
            }));
            if outcome.is_err() {
                let mut view = lock(&job.view);
                view.status = "failed".into();
                view.error = Some(AgentError::new("internal_error", "Generation worker failed; no automatic retry"));
            }
        }).map_err(|e| { entries.remove(&id); AgentError::new("internal_error", e.to_string()) })?;
        lock(&self.workers).push(worker);
        Ok(initial)
    }
    fn find(&self, id: &str) -> Result<Arc<Job>> {
        lock(&self.entries).get(id).cloned().ok_or_else(||
            AgentError::new("job_not_found", "Unknown or expired job ID in this MCP session"))
    }
    pub fn status(&self, id: &str) -> Result<Value> {
        let job = self.find(id)?;
        let value = json!(&*lock(&job.view));
        Ok(value)
    }
    pub fn cancel(&self, id: &str) -> Result<Value> {
        let job = self.find(id)?;
        let mut view = lock(&job.view);
        if view.active() {
            job.cancel.store(true, Ordering::SeqCst);
            view.cancellation_requested = true;
            view.status = "cancelling".into();
        }
        Ok(json!({"job": &*view, "note": "An in-flight HTTP request may finish and be charged; subsequent items will not start."}))
    }
    pub fn shutdown(&self) {
        for job in lock(&self.entries).values() { job.cancel.store(true, Ordering::SeqCst); }
        // Allow current blocking HTTP call (existing 90-second timeout) to unwind and clean its reservation.
        for worker in lock(&self.workers).drain(..) { let _ = worker.join(); }
    }
}

pub fn speech_schema() -> Value {
    json!({"type": "object", "additionalProperties": false, "required": ["text", "output"],
        "properties": {
            "text": {"type":"string", "minLength":1, "maxLength":4000, "description":"Text with optional Fish emotion/speaker tags"},
            "output": {"type":"string", "minLength":1, "description":"Plain filename inside the configured output directory, e.g. scene-01.mp3. Never overwritten."},
            "model": {"type":"string", "description":"Explicit provider model; no automatic fallback to paid models"},
            "voice": {"type":"string", "description":"Optional authorized Fish reference voice ID"},
            "format": {"type":"string", "enum":["mp3","wav","pcm"]},
            "speed": {"type":"number", "minimum":0.25, "maximum":4.0},
            "character": {"type":"string", "description":"Exact built-in character name from fish_catalog"},
            "preset": {"type":"string", "description":"Story preset ID from fish_catalog, e.g. bedtime or wuxia"}
        }})
}
pub fn tools() -> Value {
    let empty = json!({"type":"object", "properties":{}, "additionalProperties":false});
    let job = json!({"type":"object", "required":["job_id"], "properties":{"job_id":{"type":"string"}}, "additionalProperties":false});
    let batch = json!({"type":"object", "required":["items"], "additionalProperties":false,
        "properties":{"items":{"type":"array", "minItems":1, "maxItems":100, "items":speech_schema()}}});
    let definitions = [
        ("fish_status", "Check local configuration without making a network request; never exposes API keys.", empty.clone(), true, false),
        ("fish_catalog", "List the app's built-in characters, emotion tags and story presets; not a live provider voice list.", empty, true, false),
        ("fish_prepare", "Validate and preview one speech request without calling the provider or creating audio.", speech_schema(), true, false),
        ("fish_synthesize", "Start one speech generation job. Sends text to OpenRouter and may incur charges. Poll fish_job_status; do not retry automatically.", speech_schema(), false, true),
        ("fish_batch_synthesize", "Start a sequential multi-speaker/narration batch. Validate all items first; stop on first failure. Poll fish_job_status.", batch, false, true),
        ("fish_job_status", "Read job progress and absolute paths to completed audio files in this session.", job.clone(), true, false),
        ("fish_job_cancel", "Cancel remaining work; an in-flight provider request may still be charged.", job, false, false),
    ];
    let list: Vec<Value> = definitions.into_iter().map(|(name, description, schema, read, network)|
        json!({"name":name,"description":description,"inputSchema":schema,
            "annotations":{"readOnlyHint":read,"destructiveHint":false,
                "idempotentHint":read || name == "fish_job_cancel","openWorldHint":network}})).collect();
    json!({"tools":list})
}

fn parse<T: serde::de::DeserializeOwned>(args: Value) -> Result<T> {
    serde_json::from_value(args).map_err(|_| AgentError::invalid("Arguments do not match the tool input schema"))
}
fn job_id(args: &Value) -> Result<&str> {
    if args.as_object().is_none_or(|m| m.len() != 1) {
        return Err(AgentError::invalid("Expected only job_id"));
    }
    args.get("job_id").and_then(Value::as_str).filter(|s| !s.is_empty())
        .ok_or_else(|| AgentError::invalid("job_id must be a non-empty string"))
}
fn call(service: &AgentService, jobs: &Jobs, name: &str, args: Value) -> Result<Value> {
    match name {
        "fish_status" | "fish_catalog" => {
            if args.as_object().is_none_or(|m| !m.is_empty()) { return Err(AgentError::invalid("This tool takes no arguments")); }
            Ok(if name == "fish_status" { service.status() } else { service.catalog() })
        }
        "fish_prepare" => Ok(json!(service.prepare(&parse::<SpeechOptions>(args)?)?)),
        "fish_synthesize" => jobs.start(BatchOptions { items: vec![parse(args)?] }),
        "fish_batch_synthesize" => jobs.start(parse(args)?),
        "fish_job_status" => jobs.status(job_id(&args)?),
        "fish_job_cancel" => jobs.cancel(job_id(&args)?),
        _ => Err(AgentError::invalid("Unknown tool")),
    }
}
fn rpc_error(id: Value, code: i32, message: &str) -> Value {
    json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
}

pub struct Session {
    service: Arc<AgentService>,
    jobs: Jobs,
    protocol: Option<String>,
    ready: bool,
}
impl Session {
    pub fn new(service: Arc<AgentService>) -> Self {
        Self { jobs: Jobs::new(Arc::clone(&service)), service, protocol: None, ready: false }
    }
    pub fn handle(&mut self, message: Value) -> Option<Value> {
        let valid_id = message.get("id").is_none_or(|id| id.is_string() || id.is_i64() || id.is_u64());
        if !message.is_object() || message.get("jsonrpc") != Some(&json!("2.0"))
            || !message.get("method").is_some_and(Value::is_string) || !valid_id {
            return Some(rpc_error(Value::Null, -32600, "Invalid JSON-RPC request"));
        }
        let method = message["method"].as_str().unwrap_or("");
        let Some(id) = message.get("id").cloned() else {
            if method == "notifications/initialized" && self.protocol.is_some() { self.ready = true; }
            // Notifications, including unknown ones, never receive responses.
            return None;
        };
        let params = message.get("params").cloned().unwrap_or_else(|| json!({}));
        if !params.is_object() { return Some(rpc_error(id, -32602, "params must be an object")); }
        let result = match method {
            "initialize" => {
                if self.protocol.is_some() { return Some(rpc_error(id, -32600, "Already initialized")); }
                let requested = params.get("protocolVersion").and_then(Value::as_str);
                if requested.is_none() || !params.get("capabilities").is_some_and(Value::is_object)
                    || !params["clientInfo"]["name"].is_string() || !params["clientInfo"]["version"].is_string() {
                    return Some(rpc_error(id, -32602, "initialize requires protocolVersion, capabilities and clientInfo"));
                }
                let version = requested.filter(|v| PROTOCOL_VERSIONS.contains(v)).unwrap_or(PROTOCOL_VERSIONS[0]);
                self.protocol = Some(version.into());
                json!({"protocolVersion":version, "capabilities":{"tools":{"listChanged":false}},
                    "serverInfo":{"name":"fish-s2pro-tts", "version":env!("CARGO_PKG_VERSION")},
                    "instructions":"Use fish_catalog/prepare first. Synthesis sends text to OpenRouter and may cost money. Generation returns a job ID; poll fish_job_status. Never automatically retry charged work. Output filenames are restricted to the configured directory."})
            }
            "ping" => json!({}),
            _ if !self.ready => return Some(rpc_error(id, -32002, "Complete initialize and notifications/initialized first")),
            "tools/list" => {
                if params.get("cursor").is_some_and(|v| !v.is_null()) {
                    return Some(rpc_error(id, -32602, "This server has no pagination cursor"));
                }
                tools()
            }
            "tools/call" => {
                let Some(name) = params.get("name").and_then(Value::as_str) else {
                    return Some(rpc_error(id, -32602, "Tool name is required"));
                };
                if !TOOL_NAMES.contains(&name) { return Some(rpc_error(id, -32602, "Unknown tool")); }
                let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
                let (data, is_error) = match call(&self.service, &self.jobs, name, args) {
                    Ok(value) => (value, false),
                    Err(error) => (json!({"error":error}), true),
                };
                let mut response = json!({"content":[{"type":"text","text":data.to_string()}], "isError":is_error});
                if self.protocol.as_deref() != Some("2025-03-26") { response["structuredContent"] = data; }
                response
            }
            _ => return Some(rpc_error(id, -32601, "Method not found")),
        };
        Some(json!({"jsonrpc":"2.0","id":id,"result":result}))
    }
}
impl Drop for Session { fn drop(&mut self) { self.jobs.shutdown(); } }

/// Newline-delimited UTF-8 JSON-RPC. stdout contains protocol messages only.
pub fn serve<R: BufRead, W: Write>(mut reader: R, mut writer: W, service: Arc<AgentService>) -> std::io::Result<()> {
    let mut session = Session::new(service);
    loop {
        let mut bytes = Vec::new();
        let count = reader.by_ref().take(MAX_INPUT_BYTES + 1).read_until(b'\n', &mut bytes)?;
        if count == 0 { break; }
        if bytes.len() as u64 > MAX_INPUT_BYTES {
            writeln!(writer, "{}", rpc_error(Value::Null, -32600, "Message exceeds 1 MiB"))?;
            writer.flush()?;
            break;
        }
        let response = match serde_json::from_slice::<Value>(&bytes) {
            Ok(message) => session.handle(message),
            Err(_) => Some(rpc_error(Value::Null, -32700, "Invalid JSON")),
        };
        if let Some(response) = response { writeln!(writer, "{response}")?; writer.flush()?; }
    }
    Ok(())
}
