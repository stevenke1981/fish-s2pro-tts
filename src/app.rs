use crate::api::{KeyAuthData, OpenRouterClient, SpeechRequest, DEFAULT_MODEL, PRO_MODEL};
use crate::audio::{estimate_audio_duration, export_audio_bytes, AudioPlayer};
use crate::config::AppConfig;
use crate::file_manager::{render_file_manager_page, FileManagerState};
use crate::models::{
    format_speech_input, get_default_characters, get_sample_scripts, get_tone_tags,
    CharacterPreset, GenerationHistoryItem, SampleScript, ToneCategory, ToneTag,
};
use crate::multi_speech::{render_multi_speech_page, MultiSpeechState};
use chrono::Local;
use eframe::egui;
use egui::{Color32, RichText, Stroke, Vec2};
use rodio::Source;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

const HISTORY_FILE_PATH: &str = "outputs/history.json";

/// 標籤插入模式
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TagInsertMode {
    Append,  // 追加至末尾
    Prepend, // 插入至開頭
}

/// 主要功能分頁
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppTab {
    SingleTts,    // 單人語音生成
    MultiSpeech,  // 多角色對白生成
    FileManager,  // 語音檔案管理
}

/// 非同步背景任務回傳訊息
pub enum WorkerMessage {
    KeyVerified(Result<KeyAuthData, String>),
    SpeechProgress {
        current: usize,
        total: usize,
        message: String,
    },
    SpeechGenerated {
        result: Result<Vec<u8>, String>,
        req: SpeechRequest,
        character_name: String,
        file_path: String,
        duration_secs: Option<f32>,
    },
}

pub struct FishTtsApp {
    pub active_tab: AppTab,
    pub file_manager: FileManagerState,
    pub multi_speech: MultiSpeechState,

    config: AppConfig,
    api_key_visible: bool,
    audio_player: AudioPlayer,

    // 狀態與訊息
    status_message: String,
    is_generating: bool,
    is_verifying_key: bool,
    key_auth_info: Option<KeyAuthData>,
    last_error: Option<String>,
    success_toast: Option<(String, Instant)>,

    // 輸入區
    input_text: String,
    selected_character_idx: usize,
    auto_apply_character_tag: bool,
    selected_model: String,
    custom_model_input: String,
    custom_voice_id_input: String,
    selected_format: String,
    speed: f32,

    // 標籤與分類
    characters: Vec<CharacterPreset>,
    tone_tags: Vec<ToneTag>,
    selected_tone_category: usize,
    tag_insert_mode: TagInsertMode,
    custom_tone_input: String,
    sample_scripts: Vec<SampleScript>,
    selected_sample_idx: usize,

    // 歷史紀錄
    history: Vec<GenerationHistoryItem>,
    history_expanded: bool,

    // 通訊通道
    sender: Sender<WorkerMessage>,
    receiver: Receiver<WorkerMessage>,
}

impl FishTtsApp {
    pub fn new(_cc: &eframe::CreationContext) -> Self {
        let config = AppConfig::load();
        let (sender, receiver) = channel();

        // 確保 outputs 目錄存在
        let _ = fs::create_dir_all("outputs");

        let characters = get_default_characters();
        let tone_tags = get_tone_tags();
        let sample_scripts = get_sample_scripts();

        let initial_text = sample_scripts
            .first()
            .map(|s| s.content.to_string())
            .unwrap_or_else(|| "[calm] 歡迎使用 Fish Audio 語音合成。".to_string());

        let mut audio_player = AudioPlayer::new();
        audio_player.set_volume(config.volume);

        // 載入持久化歷史紀錄
        let mut history = Vec::new();
        if let Ok(content) = fs::read_to_string(HISTORY_FILE_PATH)
            && let Ok(items) = serde_json::from_str::<Vec<GenerationHistoryItem>>(&content)
        {
            history = items;
        }

        let tag_insert_mode = if config.tag_insert_mode == "prepend" {
            TagInsertMode::Prepend
        } else {
            TagInsertMode::Append
        };

        let mut file_manager = FileManagerState::new();
        file_manager.refresh(Path::new("outputs"), &history);
        let multi_speech = MultiSpeechState::new();

        Self {
            active_tab: AppTab::SingleTts,
            file_manager,
            multi_speech,
            api_key_visible: false,
            audio_player,

            status_message: "就緒。請輸入 API Key 並點擊產生語音。".to_string(),
            is_generating: false,
            is_verifying_key: false,
            key_auth_info: None,
            last_error: None,
            success_toast: None,

            input_text: initial_text,
            selected_character_idx: config.selected_character_index.min(characters.len().saturating_sub(1)),
            auto_apply_character_tag: config.auto_apply_character_tag,
            selected_model: config.model.clone(),
            custom_model_input: config.custom_model.clone(),
            custom_voice_id_input: config.custom_voice_id.clone(),
            selected_format: config.response_format.clone(),
            speed: config.speed,

            characters,
            tone_tags,
            selected_tone_category: 0,
            tag_insert_mode,
            custom_tone_input: String::new(),
            sample_scripts,
            selected_sample_idx: 0,

            history,
            history_expanded: true,

            config,
            sender,
            receiver,
        }
    }

    /// 儲存歷史紀錄至檔案
    fn save_history(&self) {
        let _ = fs::create_dir_all("outputs");
        if let Ok(json) = serde_json::to_string_pretty(&self.history) {
            let _ = fs::write(HISTORY_FILE_PATH, json);
        }
    }

    /// 觸發非同步金鑰驗證
    fn verify_api_key(&mut self) {
        let key = self.config.api_key.trim().to_string();
        if key.is_empty() {
            self.last_error = Some("請先輸入 OpenRouter API Key".to_string());
            return;
        }

        self.is_verifying_key = true;
        self.status_message = "正在向 OpenRouter 驗證 API 金鑰...".to_string();
        self.last_error = None;

        let tx = self.sender.clone();
        let client = OpenRouterClient::new();

        thread::spawn(move || {
            let res = client.verify_key(&key);
            let _ = tx.send(WorkerMessage::KeyVerified(res));
        });
    }

    /// 觸發非同步語音生成
    fn start_generation(&mut self) {
        let key = self.config.api_key.trim().to_string();
        if key.is_empty() {
            self.last_error = Some("請先填寫 OpenRouter API Key".to_string());
            return;
        }

        let raw_input = self.input_text.trim().to_string();
        if raw_input.is_empty() {
            self.last_error = Some("台詞文字不可為空".to_string());
            return;
        }

        let character = self
            .characters
            .get(self.selected_character_idx)
            .cloned()
            .unwrap_or_else(|| self.characters[0].clone());

        // 自動套用角色聲線提示詞 (若有啟用且未手動包含)
        let final_input = format_speech_input(&raw_input, &character, self.auto_apply_character_tag);

        // 決定使用自訂 Voice ID 或角色預設 Voice ID (若為 None 則不傳遞 voice 欄位，避免無效 ID 報錯)
        let voice_id = if !self.custom_voice_id_input.trim().is_empty() {
            Some(self.custom_voice_id_input.trim().to_string())
        } else {
            character.voice_id.clone()
        };

        // 決定模型
        let model = if self.selected_model == "custom" {
            if self.custom_model_input.trim().is_empty() {
                DEFAULT_MODEL.to_string()
            } else {
                self.custom_model_input.trim().to_string()
            }
        } else {
            self.selected_model.clone()
        };

        // 同步並儲存偏好設定
        self.config.model = self.selected_model.clone();
        self.config.custom_model = self.custom_model_input.clone();
        self.config.selected_character_index = self.selected_character_idx;
        self.config.auto_apply_character_tag = self.auto_apply_character_tag;
        self.config.custom_voice_id = self.custom_voice_id_input.clone();
        self.config.speed = self.speed;
        self.config.response_format = self.selected_format.clone();
        let _ = self.config.save();

        let req = SpeechRequest {
            model,
            input: final_input,
            voice: voice_id,
            response_format: Some(self.selected_format.clone()),
            speed: Some(self.speed),
        };

        self.is_generating = true;
        self.status_message = "正在請求 Fish Audio S2.1 合成語音，請稍候...".to_string();
        self.last_error = None;

        let tx = self.sender.clone();
        let client = OpenRouterClient::new();
        let character_name = character.name;
        let format_ext = self.selected_format.clone();

        thread::spawn(move || {
            let res = client.synthesize(&key, &req);
            match res {
                Ok(bytes) => {
                    // 自動儲存到 outputs 目錄
                    let timestamp_str = Local::now().format("%Y%m%d_%H%M%S").to_string();
                    let filename = format!("speech_{}.{}", timestamp_str, format_ext);
                    let output_path = PathBuf::from("outputs").join(&filename);
                    let file_path_str = output_path.to_string_lossy().to_string();

                    let _ = fs::write(&output_path, &bytes);

                    // 估算時長 (優先讀取解碼器，備用位元率估算)
                    let duration_secs = rodio::Decoder::new(std::io::Cursor::new(bytes.clone()))
                        .ok()
                        .and_then(|d| d.total_duration())
                        .or_else(|| estimate_audio_duration(&bytes))
                        .map(|d: Duration| d.as_secs_f32());

                    let _ = tx.send(WorkerMessage::SpeechGenerated {
                        result: Ok(bytes),
                        req,
                        character_name,
                        file_path: file_path_str,
                        duration_secs,
                    });
                }
                Err(e) => {
                    let _ = tx.send(WorkerMessage::SpeechGenerated {
                        result: Err(e),
                        req,
                        character_name,
                        file_path: String::new(),
                        duration_secs: None,
                    });
                }
            }
        });
    }

    /// 在文字輸入框插入口氣或對白標籤
    fn insert_tone_tag(&mut self, tag: &str) {
        if self.input_text.trim().is_empty() {
            self.input_text = format!("{} ", tag);
        } else {
            match self.tag_insert_mode {
                TagInsertMode::Append => {
                    let ends_with_ws = self.input_text.ends_with(' ') || self.input_text.ends_with('\n');
                    if !ends_with_ws {
                        self.input_text.push(' ');
                    }
                    self.input_text.push_str(tag);
                    self.input_text.push(' ');
                }
                TagInsertMode::Prepend => {
                    self.input_text = format!("{} {}", tag, self.input_text);
                }
            }
        }
    }

    /// 另存目前音訊
    fn export_current_audio(&mut self) {
        if let Some(bytes) = self.audio_player.current_bytes().cloned() {
            let ext = &self.selected_format;
            let default_name = format!("fish_audio_{}.{}", Local::now().format("%Y%m%d_%H%M%S"), ext);
            if let Some(path) = rfd::FileDialog::new()
                .set_file_name(&default_name)
                .add_filter("Audio File", &[ext.as_str(), "mp3", "wav"])
                .save_file()
            {
                if let Err(e) = export_audio_bytes(&bytes, &path) {
                    self.last_error = Some(format!("另存音檔失敗: {}", e));
                } else {
                    let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                    self.success_toast = Some((format!("音檔已另存至: {}", name), Instant::now()));
                }
            }
        }
    }

    /// 另存歷史項目
    fn export_history_audio(&mut self, item: &GenerationHistoryItem) {
        if let Ok(bytes) = fs::read(&item.file_path) {
            let ext = &item.format;
            let default_name = format!("export_{}.{}", item.id, ext);
            if let Some(path) = rfd::FileDialog::new()
                .set_file_name(&default_name)
                .add_filter("Audio File", &[ext.as_str(), "mp3", "wav"])
                .save_file()
            {
                if let Err(e) = export_audio_bytes(&bytes, &path) {
                    self.last_error = Some(format!("另存歷史音檔失敗: {}", e));
                } else {
                    let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                    self.success_toast = Some((format!("音檔已另存至: {}", name), Instant::now()));
                }
            }
        }
    }

    /// 開啟輸出資料夾
    fn open_output_dir(&self) {
        let dir = std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join("outputs");
        let _ = fs::create_dir_all(&dir);
        let _ = std::process::Command::new("explorer.exe")
            .arg(&dir)
            .spawn();
    }

    /// 處理非同步訊息
    fn handle_async_messages(&mut self) {
        while let Ok(msg) = self.receiver.try_recv() {
            match msg {
                WorkerMessage::KeyVerified(res) => {
                    self.is_verifying_key = false;
                    match res {
                        Ok(data) => {
                            let label = data.label.clone().unwrap_or_else(|| "正常金鑰".to_string());
                            self.status_message = format!("API 金鑰驗證成功: {}", label);
                            self.key_auth_info = Some(data);
                            self.success_toast = Some(("金鑰驗證通過！".to_string(), Instant::now()));
                            let _ = self.config.save();
                        }
                        Err(e) => {
                            self.status_message = "API 金鑰驗證失敗".to_string();
                            self.last_error = Some(e);
                        }
                    }
                }
                WorkerMessage::SpeechProgress {
                    current,
                    total,
                    message,
                } => {
                    self.multi_speech.current_step = current;
                    self.multi_speech.total_steps = total;
                    self.multi_speech.status_message = message.clone();
                    self.status_message = message;
                }
                WorkerMessage::SpeechGenerated {
                    result,
                    req,
                    character_name,
                    file_path,
                    duration_secs,
                } => {
                    self.is_generating = false;
                    self.multi_speech.is_generating = false;
                    self.multi_speech.preview_line_id = None;
                    match result {
                        Ok(bytes) => {
                            let size_kb = bytes.len() as f32 / 1024.0;
                            let dur_str = duration_secs
                                .map(|d| format!("約 {:.1} 秒", d))
                                .unwrap_or_else(|| "未知時長".to_string());

                            self.status_message =
                                format!("生成成功！大小: {:.1} KB ({})", size_kb, dur_str);
                            self.success_toast =
                                Some(("語音合成完成！".to_string(), Instant::now()));
                            self.multi_speech.status_message =
                                format!("語音生成成功！大小: {:.1} KB ({})", size_kb, dur_str);
                            self.multi_speech.success_toast =
                                Some("語音合成完成！".to_string());
                            self.multi_speech.last_generated_bytes = Some(bytes.clone());
                            if !file_path.is_empty() {
                                self.multi_speech.last_generated_path = Some(file_path.clone());
                            }

                            // 新增至歷史紀錄並儲存 (若有產出實體音訊檔)
                            if !file_path.is_empty() {
                                let item_id = Local::now().format("%Y%m%d_%H%M%S").to_string();
                                let format_ext = if file_path.ends_with(".wav") { "wav".to_string() } else { "mp3".to_string() };
                                let history_item = GenerationHistoryItem {
                                    id: item_id,
                                    timestamp: Local::now().format("%H:%M:%S").to_string(),
                                    text: req.input.clone(),
                                    character_name,
                                    model: req.model.clone(),
                                    format: format_ext,
                                    speed: req.speed.unwrap_or(self.speed),
                                    duration_secs,
                                    file_path,
                                    byte_size: bytes.len(),
                                };
                                self.history.insert(0, history_item);
                                if self.history.len() > 50 {
                                    self.history.truncate(50);
                                }
                                self.save_history();
                                self.file_manager.refresh(Path::new("outputs"), &self.history);
                            }

                            // 若啟動自動播放
                            if self.config.auto_play {
                                let _ = self.audio_player.play_bytes(bytes);
                            }
                        }
                        Err(e) => {
                            self.status_message = "語音生成失敗".to_string();
                            self.last_error = Some(e.clone());
                            self.multi_speech.error_message = Some(e);
                        }
                    }
                }
            }
        }
    }
}

impl eframe::App for FishTtsApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_async_messages();

        // 檢查如果有進行中的播放，保持每幀重繪更新進度條
        if self.audio_player.is_playing() {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }

        // ======================= 頂部標題列 =======================
        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.heading(RichText::new("🐟 Fish Audio S2.1 Pro TTS Studio").strong());

                ui.add_space(8.0);

                // 免費模型標記 Pill
                let pill_color = if self.selected_model.contains(":free") {
                    Color32::from_rgb(16, 185, 129) // Emerald green
                } else {
                    Color32::from_rgb(245, 158, 11) // Amber
                };

                ui.label(
                    RichText::new(format!("● {}", self.selected_model))
                        .size(12.0)
                        .color(pill_color)
                        .background_color(Color32::from_rgba_premultiplied(
                            pill_color.r(),
                            pill_color.g(),
                            pill_color.b(),
                            32,
                        )),
                );

                ui.add_space(16.0);

                // 頁面切換分頁
                let is_single = self.active_tab == AppTab::SingleTts;
                if ui.selectable_label(is_single, "🎙️ 單人語音生成").clicked() {
                    self.active_tab = AppTab::SingleTts;
                }

                let is_multi = self.active_tab == AppTab::MultiSpeech;
                if ui.selectable_label(is_multi, "👥 多角色對白生成").clicked() {
                    self.active_tab = AppTab::MultiSpeech;
                }

                let files_label = format!("📁 語音檔案管理 ({})", self.file_manager.files.len());
                let is_files = self.active_tab == AppTab::FileManager;
                if ui.selectable_label(is_files, files_label).clicked() {
                    self.active_tab = AppTab::FileManager;
                    self.file_manager.refresh(Path::new("outputs"), &self.history);
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // 主題切換
                    let dark = self.config.dark_mode;
                    let theme_btn = if dark { "☀️ 淺色" } else { "🌙 深色" };
                    if ui.button(theme_btn).clicked() {
                        self.config.dark_mode = !dark;
                        let _ = self.config.save();
                        if self.config.dark_mode {
                            ctx.set_visuals(egui::Visuals::dark());
                        } else {
                            ctx.set_visuals(egui::Visuals::light());
                        }
                    }

                    // OpenRouter Playground 連結
                    if ui.button("🌐 開啟 Playground").clicked() {
                        ctx.open_url(egui::OpenUrl::same_tab(
                            "https://openrouter.ai/fish-audio/s2.1-pro-free:free#playground",
                        ));
                    }
                });
            });
            ui.add_space(4.0);
        });

        // ======================= 底部狀態與歷史紀錄 =======================
        if self.active_tab != AppTab::FileManager {
            egui::TopBottomPanel::bottom("bottom_bar")
                .resizable(true)
                .min_height(36.0)
                .default_height(if self.history_expanded { 150.0 } else { 36.0 })
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        let arrow = if self.history_expanded { "▼" } else { "▶" };
                        if ui
                        .button(format!("{} 生成歷史紀錄 ({})", arrow, self.history.len()))
                        .clicked()
                    {
                        self.history_expanded = !self.history_expanded;
                    }

                    ui.label(RichText::new(&self.status_message).color(Color32::from_rgb(156, 163, 175)));

                    if let Some((toast, instant)) = &self.success_toast
                        && instant.elapsed().as_secs() < 4
                    {
                        ui.label(
                            RichText::new(format!("✓ {}", toast))
                                .color(Color32::from_rgb(34, 197, 94))
                                .strong(),
                        );
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("📁 開啟輸出資料夾").clicked() {
                            self.open_output_dir();
                        }
                        if !self.history.is_empty() && ui.button("🗑️ 清空歷史").clicked() {
                            self.history.clear();
                            self.save_history();
                        }
                    });
                });

                if self.history_expanded {
                    ui.separator();
                    if self.history.is_empty() {
                        ui.label(RichText::new("尚無歷史紀錄。點擊上方「立即產生語音」開始試聽！").italics());
                    } else {
                        egui::ScrollArea::vertical().max_height(140.0).show(ui, |ui| {
                            egui::Grid::new("history_grid")
                                .striped(true)
                                .min_col_width(60.0)
                                .spacing(Vec2::new(12.0, 6.0))
                                .show(ui, |ui| {
                                    ui.strong("時間");
                                    ui.strong("角色");
                                    ui.strong("台詞摘要");
                                    ui.strong("大小/時長");
                                    ui.strong("操作");
                                    ui.end_row();

                                    let mut play_path = None;
                                    let mut export_item = None;

                                    for item in &self.history {
                                        ui.label(&item.timestamp);
                                        ui.label(&item.character_name);
                                        let text_preview = if item.text.chars().count() > 30 {
                                            format!("{}...", item.text.chars().take(30).collect::<String>())
                                        } else {
                                            item.text.clone()
                                        };
                                        ui.label(text_preview);

                                        let dur_label = item
                                            .duration_secs
                                            .map(|d| format!("{:.1}s", d))
                                            .unwrap_or_else(|| "-".to_string());
                                        let size_kb = item.byte_size as f32 / 1024.0;
                                        ui.label(format!("{:.1}KB / {}", size_kb, dur_label));

                                        ui.horizontal(|ui| {
                                            if ui.button("▶ 播放").clicked() {
                                                play_path = Some(item.file_path.clone());
                                            }
                                            if ui.button("💾 匯出").clicked() {
                                                export_item = Some(item.clone());
                                            }
                                        });
                                        ui.end_row();
                                    }

                                    if let Some(path) = play_path {
                                        if let Ok(bytes) = fs::read(&path) {
                                            let _ = self.audio_player.play_bytes(bytes);
                                        } else {
                                            self.last_error = Some(format!("找不到音訊檔案: {}", path));
                                        }
                                    }
                                    if let Some(item) = export_item {
                                        self.export_history_audio(&item);
                                    }
                                });
                        });
                    }
                }
            });
        }

        // ======================= 左側設定欄 (僅單人語音頁面顯示) =======================
        if self.active_tab == AppTab::SingleTts {
            egui::SidePanel::left("left_settings_panel")
                .min_width(280.0)
                .default_width(310.0)
                .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.add_space(8.0);

                    // 1. OpenRouter API Key 設定卡片
                    egui::Frame::group(ui.style())
                        .corner_radius(8)
                        .inner_margin(12.0)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.strong("🔑 OpenRouter API Key");
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    let eye = if self.api_key_visible { "🙈" } else { "👁️" };
                                    if ui.button(eye).clicked() {
                                        self.api_key_visible = !self.api_key_visible;
                                    }
                                });
                            });

                            ui.add_space(4.0);
                            let text_edit = egui::TextEdit::singleline(&mut self.config.api_key)
                                .password(!self.api_key_visible)
                                .hint_text("sk-or-v1-...");
                            if ui.add(text_edit).changed() && self.config.remember_api_key {
                                let _ = self.config.save();
                            }

                            ui.add_space(6.0);
                            ui.horizontal(|ui| {
                                if ui
                                    .checkbox(&mut self.config.remember_api_key, "本機記憶 Key")
                                    .changed()
                                {
                                    let _ = self.config.save();
                                }

                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    let verify_btn_text = if self.is_verifying_key {
                                        "驗證中..."
                                    } else {
                                        "測試連線"
                                    };
                                    if ui
                                        .add_enabled(!self.is_verifying_key, egui::Button::new(verify_btn_text))
                                        .clicked()
                                    {
                                        self.verify_api_key();
                                    }
                                });
                            });

                            if let Some(info) = &self.key_auth_info {
                                ui.add_space(4.0);
                                let free_tier_str = if info.is_free_tier.unwrap_or(false) { " [免費層]" } else { "" };
                                let usage_str = match (info.usage, info.limit) {
                                    (Some(u), Some(l)) => format!(" (已用: ${:.2} / 額度: ${:.2})", u, l),
                                    (Some(u), None) => format!(" (已用: ${:.2})", u),
                                    _ => String::new(),
                                };
                                ui.label(
                                    RichText::new(format!(
                                        "✓ 連線正常: {}{}{}",
                                        info.label.as_deref().unwrap_or("預設"),
                                        free_tier_str,
                                        usage_str
                                    ))
                                    .color(Color32::from_rgb(34, 197, 94))
                                    .size(11.0),
                                );
                            }

                            if !self.audio_player.is_device_available() {
                                ui.add_space(4.0);
                                ui.label(
                                    RichText::new("⚠️ 未檢測到音訊輸出設備 (仍可正常產生並存檔)")
                                        .color(Color32::from_rgb(234, 179, 8))
                                        .size(11.0),
                                );
                            }
                        });

                    ui.add_space(10.0);

                    // 2. 模型選擇卡片
                    egui::Frame::group(ui.style())
                        .corner_radius(8)
                        .inner_margin(12.0)
                        .show(ui, |ui| {
                            ui.strong("🤖 TTS 模型選擇");
                            ui.add_space(4.0);

                            let mut model_changed = false;
                            if ui
                                .selectable_value(
                                    &mut self.selected_model,
                                    DEFAULT_MODEL.to_string(),
                                    "Fish Audio S2.1 Pro Free (免費)",
                                )
                                .clicked()
                            {
                                model_changed = true;
                            }
                            if ui
                                .selectable_value(
                                    &mut self.selected_model,
                                    PRO_MODEL.to_string(),
                                    "Fish Audio S2.1 Pro (付費/生產級)",
                                )
                                .clicked()
                            {
                                model_changed = true;
                            }
                            if ui
                                .selectable_value(
                                    &mut self.selected_model,
                                    "fish-audio/s2-pro".to_string(),
                                    "Fish Audio S2 Pro",
                                )
                                .clicked()
                            {
                                model_changed = true;
                            }
                            if ui
                                .selectable_value(
                                    &mut self.selected_model,
                                    "fish-audio/s1".to_string(),
                                    "Fish Audio S1 (舊版)",
                                )
                                .clicked()
                            {
                                model_changed = true;
                            }
                            if ui
                                .selectable_value(
                                    &mut self.selected_model,
                                    "custom".to_string(),
                                    "自訂模型識別碼",
                                )
                                .clicked()
                            {
                                model_changed = true;
                            }

                            if model_changed {
                                self.config.model = self.selected_model.clone();
                                let _ = self.config.save();
                            }

                            if self.selected_model == "custom" {
                                ui.add_space(4.0);
                                if ui.text_edit_singleline(&mut self.custom_model_input).changed() {
                                    self.config.custom_model = self.custom_model_input.clone();
                                    let _ = self.config.save();
                                }
                            }
                        });

                    ui.add_space(10.0);

                    // 3. 角色與音色設定卡片
                    egui::Frame::group(ui.style())
                        .corner_radius(8)
                        .inner_margin(12.0)
                        .show(ui, |ui| {
                            ui.strong("🎭 角色聲線預設");
                            ui.add_space(4.0);

                            let current_name = self
                                .characters
                                .get(self.selected_character_idx)
                                .map(|c| c.name.as_str())
                                .unwrap_or("未選擇");

                            egui::ComboBox::from_label("選擇角色")
                                .selected_text(current_name)
                                .show_ui(ui, |ui| {
                                    for (idx, char_item) in self.characters.iter().enumerate() {
                                        if ui
                                            .selectable_value(
                                                &mut self.selected_character_idx,
                                                idx,
                                                &char_item.name,
                                            )
                                            .clicked()
                                        {
                                            self.speed = char_item.recommended_speed;
                                            self.config.selected_character_index = idx;
                                            self.config.speed = self.speed;
                                            let _ = self.config.save();
                                        }
                                    }
                                });

                            if let Some(char_item) = self.characters.get(self.selected_character_idx).cloned() {
                                ui.add_space(4.0);
                                ui.label(
                                    RichText::new(&char_item.description)
                                        .color(Color32::from_rgb(156, 163, 175))
                                        .size(11.5),
                                );
                                ui.add_space(2.0);
                                ui.label(
                                    RichText::new(format!("聲線標籤: {}", char_item.prompt_tag))
                                        .color(Color32::from_rgb(99, 102, 241))
                                        .size(11.5),
                                );

                                ui.add_space(4.0);
                                ui.horizontal(|ui| {
                                    if ui
                                        .checkbox(&mut self.auto_apply_character_tag, "自動在台詞套用聲線標籤")
                                        .changed()
                                    {
                                        self.config.auto_apply_character_tag = self.auto_apply_character_tag;
                                        let _ = self.config.save();
                                    }
                                });

                                ui.add_space(2.0);
                                if ui.button("➕ 插入角色標籤至台詞").clicked() {
                                    let tag = char_item.prompt_tag.clone();
                                    self.insert_tone_tag(&tag);
                                }
                            }

                            ui.add_space(8.0);
                            ui.separator();
                            ui.add_space(4.0);

                            ui.strong("自訂 Fish Audio Voice ID");
                            ui.label(
                                RichText::new("可選，填入 fish.audio 已建立聲音模型的 reference ID")
                                    .size(11.0)
                                    .color(Color32::from_rgb(156, 163, 175)),
                            );
                            let voice_input = egui::TextEdit::singleline(&mut self.custom_voice_id_input)
                                .hint_text("例如: 7f8a9b0c...");
                            if ui.add(voice_input).changed() {
                                self.config.custom_voice_id = self.custom_voice_id_input.clone();
                                let _ = self.config.save();
                            }
                        });

                    ui.add_space(10.0);

                    // 4. 音訊產生參數
                    egui::Frame::group(ui.style())
                        .corner_radius(8)
                        .inner_margin(12.0)
                        .show(ui, |ui| {
                            ui.strong("⚙️ 音訊產生參數");
                            ui.add_space(4.0);

                            ui.horizontal(|ui| {
                                ui.label("語速:");
                                if ui
                                    .add(egui::Slider::new(&mut self.speed, 0.5..=2.0).step_by(0.05).suffix("x"))
                                    .changed()
                                {
                                    self.config.speed = self.speed;
                                    let _ = self.config.save();
                                }
                            });

                            ui.add_space(4.0);
                            ui.horizontal(|ui| {
                                ui.label("輸出格式:");
                                if ui.selectable_value(&mut self.selected_format, "mp3".to_string(), "MP3").clicked()
                                    || ui.selectable_value(&mut self.selected_format, "wav".to_string(), "WAV").clicked()
                                {
                                    self.config.response_format = self.selected_format.clone();
                                    let _ = self.config.save();
                                }
                            });

                            ui.add_space(4.0);
                            if ui
                                .checkbox(&mut self.config.auto_play, "產生完成後自動播放")
                                .changed()
                            {
                                let _ = self.config.save();
                            }
                        });

                    ui.add_space(12.0);
                });
            });
        }

        // ======================= 中央主要視圖 =======================
        egui::CentralPanel::default().show(ctx, |ui| {
            match self.active_tab {
                AppTab::SingleTts => {
                    self.render_single_tts_view(ui);
                }
                AppTab::MultiSpeech => {
                    render_multi_speech_page(
                        ui,
                        &mut self.multi_speech,
                        &self.characters,
                        &self.config.api_key,
                        &mut self.audio_player,
                        self.sender.clone(),
                    );
                }
                AppTab::FileManager => {
                    let save_fn = |items: &[GenerationHistoryItem]| {
                        let _ = fs::create_dir_all("outputs");
                        if let Ok(json) = serde_json::to_string_pretty(items) {
                            let _ = fs::write(HISTORY_FILE_PATH, json);
                        }
                    };
                    render_file_manager_page(
                        ui,
                        &mut self.file_manager,
                        &mut self.history,
                        &mut self.audio_player,
                        &save_fn,
                    );
                }
            }
        });
    }
}

impl FishTtsApp {
    fn render_single_tts_view(&mut self, ui: &mut egui::Ui) {
        // 錯誤訊息提示條
            let mut dismiss_error = false;
            if let Some(err) = &self.last_error {
                egui::Frame::NONE
                    .fill(Color32::from_rgba_premultiplied(239, 68, 68, 30))
                    .stroke(Stroke::new(1.0, Color32::from_rgb(239, 68, 68)))
                    .corner_radius(6)
                    .inner_margin(8.0)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("⚠️ 錯誤:").color(Color32::from_rgb(239, 68, 68)).strong());
                            ui.label(RichText::new(err).color(Color32::from_rgb(239, 68, 68)));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.small_button("✕").clicked() {
                                    dismiss_error = true;
                                }
                            });
                        });
                    });
                ui.add_space(6.0);
            }
            if dismiss_error {
                self.last_error = None;
            }

            // 1. 口氣與情緒標籤區 (Tone Tags)
            egui::Frame::group(ui.style())
                .corner_radius(8)
                .inner_margin(12.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.strong("✨ 語氣與對白標籤 (點擊直接插入台詞)");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                RichText::new("Fish Audio S2.1 支援行內情緒轉折與多角色對白")
                                    .size(11.0)
                                    .color(Color32::from_rgb(156, 163, 175)),
                            );
                        });
                    });

                    ui.add_space(4.0);

                    // 插入模式切換
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("插入模式:").size(12.0));
                        if ui
                            .radio_value(&mut self.tag_insert_mode, TagInsertMode::Append, "追加至末尾")
                            .clicked()
                        {
                            self.config.tag_insert_mode = "append".to_string();
                            let _ = self.config.save();
                        }
                        if ui
                            .radio_value(&mut self.tag_insert_mode, TagInsertMode::Prepend, "插入至開頭")
                            .clicked()
                        {
                            self.config.tag_insert_mode = "prepend".to_string();
                            let _ = self.config.save();
                        }
                    });

                    ui.add_space(6.0);

                    // 分類切換按鈕
                    ui.horizontal_wrapped(|ui| {
                        let categories = ["全部", "基礎情緒", "副語言動作", "風格語氣", "多角色對白", "自訂口氣"];
                        for (idx, cat_name) in categories.iter().enumerate() {
                            if ui
                                .selectable_label(self.selected_tone_category == idx, *cat_name)
                                .clicked()
                            {
                                self.selected_tone_category = idx;
                            }
                        }
                    });

                    ui.add_space(6.0);

                    let mut clicked_tag = None;

                    // 標籤列表
                    ui.horizontal_wrapped(|ui| {
                        let filter_category = match self.selected_tone_category {
                            1 => Some(ToneCategory::Emotion),
                            2 => Some(ToneCategory::Action),
                            3 => Some(ToneCategory::Style),
                            4 => Some(ToneCategory::Speaker),
                            _ => None,
                        };

                        if self.selected_tone_category != 5 {
                            for tag_item in &self.tone_tags {
                                if filter_category.is_none()
                                    || filter_category.as_ref() == Some(&tag_item.category)
                                {
                                    let btn = egui::Button::new(format!("{} {}", tag_item.tag, tag_item.label))
                                        .corner_radius(4);
                                    if ui.add(btn).on_hover_text(tag_item.description).clicked() {
                                        clicked_tag = Some(tag_item.tag.to_string());
                                    }
                                }
                            }
                        }

                        // 自訂口氣標籤輸入
                        if self.selected_tone_category == 0 || self.selected_tone_category == 5 {
                            for custom in &self.config.custom_tones {
                                let btn = egui::Button::new(custom)
                                    .corner_radius(4);
                                if ui.add(btn).clicked() {
                                    clicked_tag = Some(custom.clone());
                                }
                            }
                        }
                    });

                    if let Some(tag) = clicked_tag {
                        self.insert_tone_tag(&tag);
                    }

                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.label("自訂語氣標籤:");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.custom_tone_input)
                                .hint_text("例如: [帶著微笑] 或 [像在講秘密一樣]"),
                        );
                        if ui.button("＋ 插入至台詞").clicked() {
                            let tag = self.custom_tone_input.trim();
                            if !tag.is_empty() {
                                let formatted = if tag.starts_with('[') && tag.ends_with(']') {
                                    tag.to_string()
                                } else {
                                    format!("[{}]", tag)
                                };
                                self.insert_tone_tag(&formatted);
                                if !self.config.custom_tones.contains(&formatted) {
                                    self.config.custom_tones.push(formatted);
                                    let _ = self.config.save();
                                }
                                self.custom_tone_input.clear();
                            }
                        }
                    });
                });

            ui.add_space(10.0);

            // 2. 台詞編輯區
            egui::Frame::group(ui.style())
                .corner_radius(8)
                .inner_margin(12.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.strong("📝 角色文字台詞");

                        // 快速載入示範腳本
                        egui::ComboBox::from_id_salt("sample_script_combo")
                            .selected_text(
                                self.sample_scripts
                                    .get(self.selected_sample_idx)
                                    .map(|s| s.title)
                                    .unwrap_or("快速範例"),
                            )
                            .show_ui(ui, |ui| {
                                for (idx, sample) in self.sample_scripts.iter().enumerate() {
                                    if ui
                                        .selectable_value(&mut self.selected_sample_idx, idx, sample.title)
                                        .clicked()
                                    {
                                        self.input_text = sample.content.to_string();
                                        // 自動配對推薦角色
                                        if let Some(c_idx) = self
                                            .characters
                                            .iter()
                                            .position(|c| c.name.contains(sample.suggested_character))
                                        {
                                            self.selected_character_idx = c_idx;
                                            self.speed = self.characters[c_idx].recommended_speed;
                                            self.config.selected_character_index = c_idx;
                                            self.config.speed = self.speed;
                                            let _ = self.config.save();
                                        }
                                    }
                                }
                            });

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("🗑️ 清空文字").clicked() {
                                self.input_text.clear();
                            }
                            let count = self.input_text.chars().count();
                            ui.label(
                                RichText::new(format!("字數: {}", count))
                                    .size(11.5)
                                    .color(Color32::from_rgb(156, 163, 175)),
                            );
                        });
                    });

                    ui.add_space(6.0);

                    let multiline = egui::TextEdit::multiline(&mut self.input_text)
                        .desired_rows(6)
                        .desired_width(f32::INFINITY)
                        .hint_text("在此輸入需要朗讀的台詞，可自由混合 [happy]、[whispering]、<|speaker:0|> 等標籤以獲得生動口氣！");
                    ui.add(multiline);
                });

            ui.add_space(12.0);

            // 3. 語音生成操作與內建播放器
            egui::Frame::group(ui.style())
                .corner_radius(8)
                .inner_margin(14.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        // 產生按鈕
                        let gen_btn_text = if self.is_generating {
                            "🎙️ 正在產生語音中..."
                        } else {
                            "🎙️ 立即產生語音 (S2.1 Pro)"
                        };

                        let gen_btn = egui::Button::new(RichText::new(gen_btn_text).size(16.0).strong())
                            .fill(Color32::from_rgb(79, 70, 229)) // Indigo 600
                            .min_size(Vec2::new(240.0, 42.0))
                            .corner_radius(6);

                        if ui.add_enabled(!self.is_generating, gen_btn).clicked() {
                            self.start_generation();
                        }

                        if self.is_generating {
                            ui.spinner();
                        }

                        ui.add_space(16.0);
                        ui.separator();
                        ui.add_space(16.0);

                        // 播放器控制按鈕群
                        let is_playing = self.audio_player.is_playing();
                        let is_paused = self.audio_player.is_paused();

                        let play_pause_icon = if is_playing { "⏸ 暫停" } else { "▶ 播放" };
                        if ui
                            .add_enabled(
                                self.audio_player.current_bytes().is_some(),
                                egui::Button::new(play_pause_icon).min_size(Vec2::new(75.0, 36.0)),
                            )
                            .clicked()
                        {
                            if is_playing {
                                self.audio_player.pause();
                            } else if is_paused {
                                self.audio_player.resume();
                            } else {
                                let _ = self.audio_player.replay();
                            }
                        }

                        if ui
                            .add_enabled(
                                is_playing || is_paused,
                                egui::Button::new("⏹ 停止").min_size(Vec2::new(70.0, 36.0)),
                            )
                            .clicked()
                        {
                            self.audio_player.stop();
                        }

                        if ui
                            .add_enabled(
                                self.audio_player.current_bytes().is_some(),
                                egui::Button::new("🔁 重播").min_size(Vec2::new(70.0, 36.0)),
                            )
                            .clicked()
                        {
                            let _ = self.audio_player.replay();
                        }

                        // 另存音檔
                        if ui
                            .add_enabled(
                                self.audio_player.current_bytes().is_some(),
                                egui::Button::new("💾 另存音檔").min_size(Vec2::new(90.0, 36.0)),
                            )
                            .clicked()
                        {
                            self.export_current_audio();
                        }
                    });

                    ui.add_space(10.0);

                    // 播放進度條、快進跳轉與音量
                    ui.horizontal(|ui| {
                        let elapsed = self.audio_player.elapsed();
                        let total = self.audio_player.total_duration().unwrap_or(Duration::ZERO);
                        let total_secs = total.as_secs_f32();
                        let mut current_secs = elapsed.as_secs_f32();

                        let time_text = format!(
                            "{:02}:{:02} / {:02}:{:02}",
                            elapsed.as_secs() / 60,
                            elapsed.as_secs() % 60,
                            total.as_secs() / 60,
                            total.as_secs() % 60
                        );

                        ui.label(RichText::new(time_text).monospace().size(12.0));

                        let bar_width = (ui.available_width() - 170.0).max(100.0);
                        if total_secs > 0.0 {
                            let slider = egui::Slider::new(&mut current_secs, 0.0..=total_secs)
                                .show_value(false);
                            let response = ui.add_sized([bar_width, 18.0], slider);
                            if response.drag_stopped() {
                                let _ = self.audio_player.seek(Duration::from_secs_f32(current_secs));
                            }
                        } else {
                            let progress_bar = egui::ProgressBar::new(0.0)
                                .animate(self.audio_player.is_playing());
                            ui.add_sized([bar_width, 18.0], progress_bar);
                        }

                        // 音量控制
                        ui.label("🔊");
                        let mut vol = self.audio_player.get_volume();
                        if ui.add(egui::Slider::new(&mut vol, 0.0..=1.0).show_value(false)).changed() {
                            self.audio_player.set_volume(vol);
                            self.config.volume = vol;
                            let _ = self.config.save();
                        }
                    });
                });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_creation() {
        let config = AppConfig::default();
        assert_eq!(config.model, DEFAULT_MODEL);
        assert_eq!(config.tag_insert_mode, "append");
    }

    #[test]
    fn test_tag_insert_mode_logic() {
        let mut app_prepend = FishTtsApp {
            active_tab: AppTab::SingleTts,
            file_manager: FileManagerState::new(),
            multi_speech: MultiSpeechState::new(),
            config: AppConfig::default(),
            api_key_visible: false,
            audio_player: AudioPlayer::new(),
            status_message: String::new(),
            is_generating: false,
            is_verifying_key: false,
            key_auth_info: None,
            last_error: None,
            success_toast: None,
            input_text: "這是內文。".to_string(),
            selected_character_idx: 0,
            auto_apply_character_tag: true,
            selected_model: DEFAULT_MODEL.to_string(),
            custom_model_input: String::new(),
            custom_voice_id_input: String::new(),
            selected_format: "mp3".to_string(),
            speed: 1.0,
            characters: get_default_characters(),
            tone_tags: get_tone_tags(),
            selected_tone_category: 0,
            tag_insert_mode: TagInsertMode::Prepend,
            custom_tone_input: String::new(),
            sample_scripts: get_sample_scripts(),
            selected_sample_idx: 0,
            history: Vec::new(),
            history_expanded: false,
            sender: channel().0,
            receiver: channel().1,
        };

        app_prepend.insert_tone_tag("[happy]");
        assert_eq!(app_prepend.input_text, "[happy] 這是內文。");

        let mut app_append = app_prepend;
        app_append.tag_insert_mode = TagInsertMode::Append;
        app_append.insert_tone_tag("[excited]");
        assert!(app_append.input_text.ends_with("[excited] "));
    }
}
