use fish_s2pro_tts::agent::{AgentService, BatchOptions, SpeechBackend, SpeechOptions, validate_output_name};
use fish_s2pro_tts::api::SpeechRequest;
use fish_s2pro_tts::config::AppConfig;
use fish_s2pro_tts::mcp::{self, Jobs, Session};
use serde_json::{Value, json};
use std::io::Cursor;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, atomic::{AtomicBool, AtomicUsize, Ordering}};
use std::time::{Duration, Instant};

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct TempDir(PathBuf);
impl TempDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("fish-agent-test-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::SeqCst)));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for TempDir { fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); } }
struct Fake {
    calls: AtomicUsize,
    fail: bool,
    delay: bool,
    started: AtomicBool,
}
impl Fake {
    fn new(fail: bool, delay: bool) -> Self {
        Self { calls: AtomicUsize::new(0), fail, delay, started: AtomicBool::new(false) }
    }
}
impl SpeechBackend for Fake {
    fn synthesize(&self, key: &str, _: &SpeechRequest) -> Result<Vec<u8>, String> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.started.store(true, Ordering::SeqCst);
        if self.delay { std::thread::sleep(Duration::from_millis(200)); }
        if self.fail { Err(format!("provider echoed {key}")) } else { Ok(b"ID3mock-audio".to_vec()) }
    }
}
fn service(dir: &TempDir, fake: Arc<Fake>, key: &str) -> Arc<AgentService> {
    Arc::new(AgentService::with_backend(dir.0.clone(), AppConfig::default(), key.to_owned(), fake).unwrap())
}
fn speech(name: &str) -> SpeechOptions {
    serde_json::from_value(json!({"text":"[happy] 你好，這是繁體中文配音。", "output":name})).unwrap()
}
fn init(session: &mut Session, version: &str) -> Value {
    let result = session.handle(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
        "protocolVersion":version,"capabilities":{},"clientInfo":{"name":"test","version":"1"}}})).unwrap();
    assert!(session.handle(json!({"jsonrpc":"2.0","method":"notifications/initialized"})).is_none());
    result
}

#[test]
fn prepare_is_offline_and_reuses_story_preset() {
    let dir = TempDir::new(); let fake = Arc::new(Fake::new(false, false));
    let service = service(&dir, fake.clone(), "");
    let mut options = speech("故事.mp3"); options.preset = Some("bedtime".into()); options.speed = Some(1.1);
    let prepared = service.prepare(&options).unwrap();
    assert!(prepared.request.input.contains("[happy]"));
    assert!(prepared.request.input.contains("[soft voice]"));
    assert_eq!(prepared.request.speed, Some(1.1));
    assert_eq!(fake.calls.load(Ordering::SeqCst), 0);
    assert_eq!(std::fs::read_dir(&dir.0).unwrap().count(), 0);
}
#[test]
fn character_catalog_comes_from_gui_models() {
    let dir = TempDir::new(); let service = service(&dir, Arc::new(Fake::new(false, false)), "secret-for-test");
    let catalog = service.catalog();
    assert_eq!(catalog["characters"].as_array().unwrap().len(), fish_s2pro_tts::models::get_default_characters().len());
    assert!(!service.status().to_string().contains("secret-for-test"));
    assert!(!catalog.to_string().contains("secret-for-test"));
    let mut options = speech("voice.mp3"); options.character = Some("溫柔知性御姐".into());
    assert!(service.prepare(&options).unwrap().request.input.contains("溫柔知性御姐音色"));
}
#[test]
fn rejects_unsafe_names_and_format_mismatch() {
    for name in ["../out.mp3", "sub/out.mp3", "sub\\out.mp3", "C:out.mp3", "CON.mp3", "LPT1.mp3", ".hidden.mp3", "audio.wav", "out.mp3 ", "a\n.mp3"] {
        assert!(validate_output_name(name, "mp3").is_err(), "accepted {name}");
    }
    assert!(validate_output_name("中文-01.mp3", "mp3").is_ok());
}
#[test]
fn checks_unicode_length_speed_and_unknown_fields() {
    let dir = TempDir::new(); let service = service(&dir, Arc::new(Fake::new(false, false)), "");
    let mut options = speech("x.mp3"); options.text = "字".repeat(4000);
    assert!(service.prepare(&options).is_ok());
    options.text.push('字'); assert!(service.prepare(&options).is_err());
    options.text = "hello".into(); options.speed = Some(f32::NAN);
    assert!(service.prepare(&options).is_err());
    assert!(serde_json::from_value::<SpeechOptions>(json!({"text":"x","output":"x.mp3","api_key":"no"})).is_err());
}
#[test]
fn synthesis_is_no_overwrite_and_reports_real_bytes() {
    let dir = TempDir::new(); let fake = Arc::new(Fake::new(false, false));
    let service = service(&dir, fake.clone(), "test-key");
    let output = service.synthesize(&speech("x.mp3"), &AtomicBool::new(false)).unwrap();
    assert_eq!(output["bytes"], 13);
    assert_eq!(std::fs::read(dir.0.join("x.mp3")).unwrap(), b"ID3mock-audio");
    assert_eq!(service.synthesize(&speech("x.mp3"), &AtomicBool::new(false)).unwrap_err().code, "output_exists");
    assert_eq!(fake.calls.load(Ordering::SeqCst), 1);
}
#[test]
fn error_redaction_and_reservation_cleanup() {
    let dir = TempDir::new(); let service = service(&dir, Arc::new(Fake::new(true, false)), "super-secret");
    let error = service.synthesize(&speech("fail.mp3"), &AtomicBool::new(false)).unwrap_err();
    assert!(!error.message.contains("super-secret")); assert!(error.message.contains("[REDACTED]"));
    assert!(!dir.0.join("fail.mp3").exists());
}
#[test]
fn no_key_or_invalid_batch_never_calls_provider() {
    let dir = TempDir::new(); let fake = Arc::new(Fake::new(false, false));
    let service = service(&dir, fake.clone(), "");
    assert_eq!(service.synthesize(&speech("x.mp3"), &AtomicBool::new(false)).unwrap_err().code, "missing_api_key");
    let batch = BatchOptions { items: vec![speech("one.mp3"), speech("ONE.mp3")] };
    assert!(service.run_batch(&batch).is_err());
    let batch = BatchOptions { items: vec![speech("one.mp3"), speech("../bad.mp3")] };
    assert!(service.run_batch(&batch).is_err());
    assert_eq!(fake.calls.load(Ordering::SeqCst), 0);
}
#[cfg(unix)]
#[test]
fn dangling_symlink_cannot_escape_output_boundary() {
    let dir = TempDir::new(); let outside = TempDir::new(); let fake = Arc::new(Fake::new(false, false));
    std::os::unix::fs::symlink(outside.0.join("outside.mp3"), dir.0.join("link.mp3")).unwrap();
    let service = service(&dir, fake.clone(), "test-key");
    assert_eq!(service.synthesize(&speech("link.mp3"), &AtomicBool::new(false)).unwrap_err().code, "output_exists");
    assert_eq!(fake.calls.load(Ordering::SeqCst), 0); assert!(!outside.0.join("outside.mp3").exists());
}
#[test]
fn jobs_complete_and_return_artifacts() {
    let dir = TempDir::new(); let jobs = Jobs::new(service(&dir, Arc::new(Fake::new(false, false)), "test"));
    let started = jobs.start(BatchOptions { items: vec![speech("1.mp3"), speech("2.mp3")] }).unwrap();
    let id = started["job_id"].as_str().unwrap(); let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let view = jobs.status(id).unwrap();
        if view["status"] == "completed" { assert_eq!(view["completed"], 2); break; }
        assert!(Instant::now() < deadline, "{view}"); std::thread::sleep(Duration::from_millis(5));
    }
    jobs.shutdown();
}
#[test]
fn cancellation_discards_inflight_result_and_prevents_next_request() {
    let dir = TempDir::new(); let fake = Arc::new(Fake::new(false, true));
    let jobs = Jobs::new(service(&dir, fake.clone(), "test"));
    let started = jobs.start(BatchOptions { items: vec![speech("1.mp3"), speech("2.mp3")] }).unwrap();
    let id = started["job_id"].as_str().unwrap(); let deadline = Instant::now() + Duration::from_secs(3);
    while !fake.started.load(Ordering::SeqCst) {
        assert!(Instant::now() < deadline); std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(jobs.start(BatchOptions { items: vec![speech("3.mp3")] }).unwrap_err().code, "busy");
    jobs.cancel(id).unwrap(); jobs.shutdown();
    assert_eq!(jobs.status(id).unwrap()["status"], "cancelled");
    assert_eq!(fake.calls.load(Ordering::SeqCst), 1);
    assert_eq!(std::fs::read_dir(&dir.0).unwrap().count(), 0);
}
#[test]
fn protocol_negotiates_versions_and_silences_notifications() {
    let dir = TempDir::new(); let service = service(&dir, Arc::new(Fake::new(false, false)), "");
    for version in mcp::PROTOCOL_VERSIONS.iter().copied().chain(["future-version"]) {
        let mut session = Session::new(service.clone()); let response = init(&mut session, version);
        assert_eq!(response["result"]["protocolVersion"], if version == "future-version" { mcp::PROTOCOL_VERSIONS[0] } else { version });
        let call = session.handle(json!({"jsonrpc":"2.0","id":"test","method":"tools/call","params":{"name":"fish_status","arguments":{}}})).unwrap();
        assert_eq!(call["result"]["isError"], false);
        assert_eq!(call["result"].get("structuredContent").is_some(), version != "2025-03-26");
        assert!(session.handle(json!({"jsonrpc":"2.0","method":"notifications/unknown"})).is_none());
    }
}
#[test]
fn protocol_rejects_invalid_lifecycle_tools_and_arguments() {
    let dir = TempDir::new(); let mut session = Session::new(service(&dir, Arc::new(Fake::new(false, false)), ""));
    assert_eq!(session.handle(json!({"jsonrpc":"2.0","id":2,"method":"tools/list"})).unwrap()["error"]["code"], -32002);
    init(&mut session, "2025-11-25");
    assert_eq!(session.handle(json!({"jsonrpc":"2.0","id":2,"method":"unknown"})).unwrap()["error"]["code"], -32601);
    let bad = session.handle(json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"fish_prepare","arguments":{"text":"x","output":"../x.mp3"}}})).unwrap();
    assert_eq!(bad["result"]["isError"], true);
    let tools = session.handle(json!({"jsonrpc":"2.0","id":3,"method":"tools/list"})).unwrap();
    assert_eq!(tools["result"]["tools"].as_array().unwrap().len(), 7);
    assert_eq!(tools["result"]["tools"][3]["annotations"]["openWorldHint"], true);
    assert_eq!(session.handle(json!([])).unwrap()["error"]["code"], -32600);
}
#[test]
fn transport_recovers_from_bad_json_and_bounds_input() {
    let dir = TempDir::new(); let service = service(&dir, Arc::new(Fake::new(false, false)), "");
    let input = b"bad json\n{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n";
    let mut output = Vec::new(); mcp::serve(Cursor::new(input), &mut output, service.clone()).unwrap();
    let text = String::from_utf8(output).unwrap(); let lines: Vec<_> = text.lines().collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(serde_json::from_str::<Value>(lines[0]).unwrap()["error"]["code"], -32700);
    assert_eq!(serde_json::from_str::<Value>(lines[1]).unwrap()["result"], json!({}));
    let input = vec![b'x'; 1_048_577]; let mut output = Vec::new();
    mcp::serve(Cursor::new(input), &mut output, service).unwrap();
    assert_eq!(serde_json::from_slice::<Value>(&output).unwrap()["error"]["code"], -32600);
}
#[test]
fn cli_binary_dry_run_and_missing_key_are_machine_readable() {
    let dir = TempDir::new();
    for (command, code) in [("prepare", 0), ("synthesize", 3)] {
        let output = Command::new(env!("CARGO_BIN_EXE_fish-s2pro-tts-cli"))
            .env_remove("OPENROUTER_API_KEY").arg("--output-dir").arg(&dir.0)
            .args([command, "--text", "繁體中文測試", "--output", "test.mp3"])
            .output().unwrap();
        assert_eq!(output.status.code(), Some(code));
        let result: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(result["ok"], code == 0);
    }
    assert_eq!(std::fs::read_dir(&dir.0).unwrap().count(), 0);
}
