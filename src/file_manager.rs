use crate::audio::{estimate_audio_duration_with_size, export_audio_bytes, AudioPlayer};
use crate::models::GenerationHistoryItem;
use chrono::{DateTime, Local};
use eframe::egui;
use egui::{Color32, RichText, Stroke, Vec2};
use std::collections::HashSet;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// 支援的音訊副檔名
pub const SUPPORTED_EXTENSIONS: &[&str] = &["mp3", "wav", "ogg", "flac", "m4a", "aac"];

/// 檔案列表排序依據
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileSortBy {
    DateDesc,
    DateAsc,
    SizeDesc,
    SizeAsc,
    DurationDesc,
    DurationAsc,
    NameAsc,
    NameDesc,
}

impl FileSortBy {
    pub fn label(&self) -> &'static str {
        match self {
            FileSortBy::DateDesc => "建立時間 (最新優先)",
            FileSortBy::DateAsc => "建立時間 (最舊優先)",
            FileSortBy::SizeDesc => "檔案大小 (由大到小)",
            FileSortBy::SizeAsc => "檔案大小 (由小到大)",
            FileSortBy::DurationDesc => "音訊時長 (由長到短)",
            FileSortBy::DurationAsc => "音訊時長 (由短到長)",
            FileSortBy::NameAsc => "檔案名稱 (A-Z)",
            FileSortBy::NameDesc => "檔案名稱 (Z-A)",
        }
    }
}

/// 格式篩選選項
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormatFilter {
    All,
    Mp3,
    Wav,
    Other,
}

impl FormatFilter {
    pub fn label(&self) -> &'static str {
        match self {
            FormatFilter::All => "全部格式",
            FormatFilter::Mp3 => "MP3",
            FormatFilter::Wav => "WAV",
            FormatFilter::Other => "其他格式",
        }
    }
}

/// 單一音訊檔案元數據項目
#[derive(Clone, Debug, PartialEq)]
pub struct AudioFileEntry {
    pub filename: String,
    pub path: PathBuf,
    pub byte_size: u64,
    pub modified_time: SystemTime,
    pub modified_str: String,
    pub duration_secs: Option<f32>,
    pub format: String,
    pub character_name: String,
    pub text_snippet: String,
    pub model: String,
    pub is_selected: bool,
}

impl AudioFileEntry {
    pub fn formatted_size(&self) -> String {
        let kb = self.byte_size as f64 / 1024.0;
        if kb < 1024.0 {
            format!("{:.1} KB", kb)
        } else {
            format!("{:.2} MB", kb / 1024.0)
        }
    }

    pub fn formatted_duration(&self) -> String {
        if let Some(dur) = self.duration_secs {
            let total_secs = dur.round() as u64;
            format!("{:02}:{:02}", total_secs / 60, total_secs % 60)
        } else {
            "--:--".to_string()
        }
    }
}

/// 語音檔案管理狀態
pub struct FileManagerState {
    pub files: Vec<AudioFileEntry>,
    pub search_query: String,
    pub format_filter: FormatFilter,
    pub sort_by: FileSortBy,
    pub selected_file_path: Option<PathBuf>,
    pub rename_target: Option<(PathBuf, String)>, // (舊路徑, 新名稱輸入)
    pub status_feedback: Option<String>,
    pub error_feedback: Option<String>,
    pub show_batch_confirm_dialog: bool,
}

impl Default for FileManagerState {
    fn default() -> Self {
        Self::new()
    }
}

impl FileManagerState {
    pub fn new() -> Self {
        Self {
            files: Vec::new(),
            search_query: String::new(),
            format_filter: FormatFilter::All,
            sort_by: FileSortBy::DateDesc,
            selected_file_path: None,
            rename_target: None,
            status_feedback: None,
            error_feedback: None,
            show_batch_confirm_dialog: false,
        }
    }

    /// 掃描 outputs 目錄並整合歷史紀錄資訊
    pub fn refresh(&mut self, outputs_dir: &Path, history: &[GenerationHistoryItem]) {
        self.files = scan_output_files(outputs_dir, history);
        self.apply_sort();
    }

    /// 套用排序規則
    pub fn apply_sort(&mut self) {
        match self.sort_by {
            FileSortBy::DateDesc => self.files.sort_by(|a, b| b.modified_time.cmp(&a.modified_time)),
            FileSortBy::DateAsc => self.files.sort_by(|a, b| a.modified_time.cmp(&b.modified_time)),
            FileSortBy::SizeDesc => self.files.sort_by(|a, b| b.byte_size.cmp(&a.byte_size)),
            FileSortBy::SizeAsc => self.files.sort_by(|a, b| a.byte_size.cmp(&b.byte_size)),
            FileSortBy::DurationDesc => self.files.sort_by(|a, b| {
                let dur_a = a.duration_secs.unwrap_or(0.0);
                let dur_b = b.duration_secs.unwrap_or(0.0);
                dur_b.partial_cmp(&dur_a).unwrap_or(std::cmp::Ordering::Equal)
            }),
            FileSortBy::DurationAsc => self.files.sort_by(|a, b| {
                let dur_a = a.duration_secs.unwrap_or(0.0);
                let dur_b = b.duration_secs.unwrap_or(0.0);
                dur_a.partial_cmp(&dur_b).unwrap_or(std::cmp::Ordering::Equal)
            }),
            FileSortBy::NameAsc => self.files.sort_by(|a, b| a.filename.to_lowercase().cmp(&b.filename.to_lowercase())),
            FileSortBy::NameDesc => self.files.sort_by(|a, b| b.filename.to_lowercase().cmp(&a.filename.to_lowercase())),
        }
    }

    /// 取得通過篩選與關鍵字條件的檔案清單
    pub fn filtered_files(&self) -> Vec<&AudioFileEntry> {
        let query = self.search_query.trim().to_lowercase();
        self.files
            .iter()
            .filter(|item| {
                let match_format = match self.format_filter {
                    FormatFilter::All => true,
                    FormatFilter::Mp3 => item.format.eq_ignore_ascii_case("mp3"),
                    FormatFilter::Wav => item.format.eq_ignore_ascii_case("wav"),
                    FormatFilter::Other => {
                        !item.format.eq_ignore_ascii_case("mp3") && !item.format.eq_ignore_ascii_case("wav")
                    }
                };

                if !match_format {
                    return false;
                }

                if query.is_empty() {
                    return true;
                }

                item.filename.to_lowercase().contains(&query)
                    || item.text_snippet.to_lowercase().contains(&query)
                    || item.character_name.to_lowercase().contains(&query)
                    || item.model.to_lowercase().contains(&query)
            })
            .collect()
    }

    /// 全選
    pub fn select_all(&mut self, select: bool) {
        for file in &mut self.files {
            file.is_selected = select;
        }
    }

    /// 反向選取
    pub fn invert_selection(&mut self) {
        for file in &mut self.files {
            file.is_selected = !file.is_selected;
        }
    }

    /// 取得選中項目數量
    pub fn selected_count(&self) -> usize {
        self.files.iter().filter(|f| f.is_selected).count()
    }

    /// 批次刪除選中項目 (僅從列表移除實際刪除成功的檔案)
    pub fn batch_delete_selected(&mut self) -> Result<usize, String> {
        let to_delete: Vec<PathBuf> = self
            .files
            .iter()
            .filter(|f| f.is_selected)
            .map(|f| f.path.clone())
            .collect();

        let mut deleted_count = 0;
        let mut errors = Vec::new();
        let mut deleted_paths = HashSet::new();

        for path in to_delete {
            match fs::remove_file(&path) {
                Ok(_) => {
                    deleted_count += 1;
                    deleted_paths.insert(path);
                }
                Err(e) => errors.push(format!("{}: {}", path.display(), e)),
            }
        }

        self.files.retain(|f| !deleted_paths.contains(&f.path));
        if let Some(selected) = &self.selected_file_path
            && deleted_paths.contains(selected)
        {
            self.selected_file_path = None;
        }

        if !errors.is_empty() {
            Err(format!("部分檔案刪除失敗: {}", errors.join("; ")))
        } else {
            Ok(deleted_count)
        }
    }

    /// 清理超過指定天數的舊檔案 (僅從列表移除實際刪除成功的檔案)
    pub fn cleanup_older_than_days(&mut self, days: i64) -> Result<usize, String> {
        let cutoff = SystemTime::now()
            .checked_sub(std::time::Duration::from_secs((days as u64) * 86400))
            .unwrap_or(SystemTime::now());

        let to_delete: Vec<PathBuf> = self
            .files
            .iter()
            .filter(|f| f.modified_time < cutoff)
            .map(|f| f.path.clone())
            .collect();

        let mut deleted_count = 0;
        let mut errors = Vec::new();
        let mut deleted_paths = HashSet::new();

        for path in to_delete {
            match fs::remove_file(&path) {
                Ok(_) => {
                    deleted_count += 1;
                    deleted_paths.insert(path);
                }
                Err(e) => errors.push(format!("{}: {}", path.display(), e)),
            }
        }

        self.files.retain(|f| !deleted_paths.contains(&f.path));
        if let Some(selected) = &self.selected_file_path
            && deleted_paths.contains(selected)
        {
            self.selected_file_path = None;
        }

        if !errors.is_empty() {
            Err(format!("部分舊檔清理失敗: {}", errors.join("; ")))
        } else {
            Ok(deleted_count)
        }
    }

    /// 重新命名檔案
    pub fn rename_file(&mut self, old_path: &Path, new_name_input: &str) -> Result<PathBuf, String> {
        let trimmed_new = new_name_input.trim();
        if trimmed_new.is_empty() {
            return Err("檔案名稱不可為空".to_string());
        }

        let parent = old_path
            .parent()
            .ok_or_else(|| "無法取得檔案所在目錄".to_string())?;

        let old_ext = old_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("mp3");

        // 檢查使用者輸入是否已具備任何支援的音訊副檔名 (不分大小寫)
        let has_supported_ext = SUPPORTED_EXTENSIONS.iter().any(|ext| {
            let suffix = format!(".{}", ext);
            trimmed_new.to_lowercase().ends_with(&suffix)
        });

        let final_filename = if has_supported_ext {
            trimmed_new.to_string()
        } else {
            format!("{}.{}", trimmed_new, old_ext)
        };

        // 檢查是否有非法字元
        let illegal_chars = ['/', '\\', ':', '*', '?', '"', '<', '>', '|'];
        if final_filename.chars().any(|c| illegal_chars.contains(&c)) {
            return Err("檔案名稱含有 Windows 系統不支援的非法字元".to_string());
        }

        let new_path = parent.join(&final_filename);
        if new_path.exists() && new_path != old_path {
            return Err(format!("已存在同名檔案: {}", final_filename));
        }

        fs::rename(old_path, &new_path).map_err(|e| format!("重新命名失敗: {}", e))?;

        let new_ext = new_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or(old_ext)
            .to_uppercase();

        // 更新記憶體中相應項目的路徑、檔名與格式
        if let Some(entry) = self.files.iter_mut().find(|f| f.path == old_path) {
            entry.filename = final_filename;
            entry.path = new_path.clone();
            entry.format = new_ext;
        }

        if self.selected_file_path.as_ref() == Some(&old_path.to_path_buf()) {
            self.selected_file_path = Some(new_path.clone());
        }

        Ok(new_path)
    }

    /// 總計統計 (檔案數, 總位元組數, 總時長秒數)
    pub fn statistics(&self) -> (usize, u64, f32) {
        let total_count = self.files.len();
        let total_bytes: u64 = self.files.iter().map(|f| f.byte_size).sum();
        let total_duration: f32 = self.files.iter().filter_map(|f| f.duration_secs).sum();
        (total_count, total_bytes, total_duration)
    }
}

/// 掃描目錄下所有音訊檔案並配對歷史中記載的資訊
pub fn scan_output_files(outputs_dir: &Path, history: &[GenerationHistoryItem]) -> Vec<AudioFileEntry> {
    let mut entries = Vec::new();

    if !outputs_dir.exists() {
        let _ = fs::create_dir_all(outputs_dir);
        return entries;
    }

    let dir_entries = match fs::read_dir(outputs_dir) {
        Ok(read_dir) => read_dir,
        Err(_) => return entries,
    };

    for entry in dir_entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_lowercase())
            .unwrap_or_default();

        if !SUPPORTED_EXTENSIONS.contains(&ext.as_str()) {
            continue;
        }

        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();

        let metadata = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };

        let byte_size = metadata.len();
        let modified_time = metadata.modified().unwrap_or(SystemTime::now());
        let dt: DateTime<Local> = modified_time.into();
        let modified_str = dt.format("%Y-%m-%d %H:%M:%S").to_string();

        // 嘗試在歷史紀錄中查找匹配 (先比對完整路徑，再比對檔名結尾或時間戳 ID)
        let matched_history = history.iter().find(|h| {
            let h_path = Path::new(&h.file_path);
            h.file_path == path.to_string_lossy()
                || h_path.file_name() == path.file_name()
                || (!h.id.is_empty() && filename.contains(&h.id))
        });

        let (character_name, text_snippet, model, mut duration_secs) = if let Some(h) = matched_history {
            (
                h.character_name.clone(),
                h.text.clone(),
                h.model.clone(),
                h.duration_secs,
            )
        } else {
            (
                "未登錄角色".to_string(),
                "(無對應台詞紀錄)".to_string(),
                "-".to_string(),
                None,
            )
        };

        // 若無時長記錄，讀取檔案頭部 (最多 64KB) 以估算時長，避免載入完整大檔案
        if duration_secs.is_none()
            && byte_size > 0
            && let Ok(mut file) = fs::File::open(&path)
        {
            let mut header_buf = Vec::new();
            let _ = file.by_ref().take(65536).read_to_end(&mut header_buf);
            duration_secs = estimate_audio_duration_with_size(&header_buf, byte_size)
                .map(|d| d.as_secs_f32());
        }

        entries.push(AudioFileEntry {
            filename,
            path,
            byte_size,
            modified_time,
            modified_str,
            duration_secs,
            format: ext.to_uppercase(),
            character_name,
            text_snippet,
            model,
            is_selected: false,
        });
    }

    entries
}

/// 檔案管理頁面動作通知
#[derive(Clone, Debug, PartialEq)]
pub enum FileManagerAction {
    None,
    SendToTimeline(PathBuf),
}

/// 繪製語音檔案管理頁面 UI
pub fn render_file_manager_page(
    ui: &mut egui::Ui,
    state: &mut FileManagerState,
    history: &mut Vec<GenerationHistoryItem>,
    audio_player: &mut AudioPlayer,
    save_history_fn: &dyn Fn(&[GenerationHistoryItem]),
) -> FileManagerAction {
    let mut send_timeline_file: Option<PathBuf> = None;
    ui.add_space(6.0);

    // 提示與錯誤反饋訊息
    let mut dismiss_err = false;
    if let Some(err) = &state.error_feedback {
        egui::Frame::NONE
            .fill(Color32::from_rgba_premultiplied(239, 68, 68, 35))
            .stroke(egui::Stroke::new(1.0, Color32::from_rgb(239, 68, 68)))
            .corner_radius(6)
            .inner_margin(8.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("⚠️ 錯誤:").color(Color32::from_rgb(239, 68, 68)).strong());
                    ui.label(RichText::new(err).color(Color32::from_rgb(239, 68, 68)));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("✕").clicked() {
                            dismiss_err = true;
                        }
                    });
                });
            });
        ui.add_space(4.0);
    }
    if dismiss_err {
        state.error_feedback = None;
    }

    let mut dismiss_status = false;
    if let Some(status) = &state.status_feedback {
        egui::Frame::NONE
            .fill(Color32::from_rgba_premultiplied(34, 197, 94, 30))
            .stroke(egui::Stroke::new(1.0, Color32::from_rgb(34, 197, 94)))
            .corner_radius(6)
            .inner_margin(8.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("✓").color(Color32::from_rgb(34, 197, 94)).strong());
                    ui.label(RichText::new(status).color(Color32::from_rgb(34, 197, 94)));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("✕").clicked() {
                            dismiss_status = true;
                        }
                    });
                });
            });
        ui.add_space(4.0);
    }
    if dismiss_status {
        state.status_feedback = None;
    }

    // ==================== 頂部控制與搜尋欄 ====================
    egui::Frame::group(ui.style())
        .corner_radius(8)
        .inner_margin(12.0)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading(RichText::new("📁 outputs 語音檔案總管").strong());

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // 開啟資料夾
                    if ui.button("📂 開啟 outputs 目錄").clicked() {
                        let outputs_path = std::env::current_dir()
                            .unwrap_or_else(|_| PathBuf::from("."))
                            .join("outputs");
                        let _ = fs::create_dir_all(&outputs_path);
                        let _ = std::process::Command::new("explorer.exe")
                            .arg(&outputs_path)
                            .spawn();
                    }

                    // 重新整理
                    if ui.button("🔄 重新整理").clicked() {
                        state.refresh(Path::new("outputs"), history);
                        state.status_feedback = Some("已重新掃描檔案目錄".to_string());
                    }
                });
            });

            ui.add_space(6.0);

            // 搜尋與篩選列
            ui.horizontal(|ui| {
                ui.label("🔍 搜尋:");
                ui.add(
                    egui::TextEdit::singleline(&mut state.search_query)
                        .hint_text("輸入檔名、角色名或台詞關鍵字...")
                        .desired_width(240.0),
                );

                if !state.search_query.is_empty() && ui.small_button("✕").clicked() {
                    state.search_query.clear();
                }

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(8.0);

                // 格式篩選
                ui.label("格式:");
                let filters = [
                    (FormatFilter::All, "全部"),
                    (FormatFilter::Mp3, "MP3"),
                    (FormatFilter::Wav, "WAV"),
                ];
                for (fmt, label) in filters {
                    if ui.selectable_value(&mut state.format_filter, fmt, label).clicked() {
                        // filtered on-the-fly
                    }
                }

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(8.0);

                // 排序選單
                egui::ComboBox::from_label("排序依據")
                    .selected_text(state.sort_by.label())
                    .show_ui(ui, |ui| {
                        let sorts = [
                            FileSortBy::DateDesc,
                            FileSortBy::DateAsc,
                            FileSortBy::SizeDesc,
                            FileSortBy::SizeAsc,
                            FileSortBy::DurationDesc,
                            FileSortBy::DurationAsc,
                            FileSortBy::NameAsc,
                            FileSortBy::NameDesc,
                        ];
                        for s in sorts {
                            if ui.selectable_value(&mut state.sort_by, s, s.label()).clicked() {
                                state.apply_sort();
                            }
                        }
                    });
            });

            ui.add_space(4.0);

            // 統計與批次操作欄
            let (total_count, total_bytes, total_dur) = state.statistics();
            let total_dur_mins = (total_dur / 60.0).floor();
            let total_dur_secs = (total_dur % 60.0).round();
            let size_mb = total_bytes as f64 / (1024.0 * 1024.0);

            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!(
                        "📊 總檔案: {} 個 | 總容量: {:.2} MB | 總長度: {:.0}分{:.0}秒",
                        total_count, size_mb, total_dur_mins, total_dur_secs
                    ))
                    .size(11.5)
                    .color(Color32::from_rgb(156, 163, 175)),
                );

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let sel_count = state.selected_count();

                    // 批次刪除按鈕
                    if sel_count > 0 {
                        let del_text = format!("🗑️ 刪除選中 ({})", sel_count);
                        let del_btn = egui::Button::new(RichText::new(del_text).color(Color32::from_rgb(239, 68, 68)));
                        if ui.add(del_btn).clicked() {
                            state.show_batch_confirm_dialog = true;
                        }
                    }

                    // 清理 7 天前舊檔按鈕
                    if ui.button("🧹 清理 7 天前舊檔").clicked() {
                        let prev_selected = state.selected_file_path.clone();
                        match state.cleanup_older_than_days(7) {
                            Ok(n) => {
                                if prev_selected.is_some() && state.selected_file_path.is_none() {
                                    audio_player.stop();
                                }
                                state.status_feedback = Some(format!("已清理 {} 個舊檔案", n));
                                // 同步歷史
                                history.retain(|h| Path::new(&h.file_path).exists());
                                save_history_fn(history);
                            }
                            Err(e) => state.error_feedback = Some(e),
                        }
                    }

                    if ui.button("反向選取").clicked() {
                        state.invert_selection();
                    }

                    if ui.button("全選").clicked() {
                        state.select_all(true);
                    }

                    if ui.button("取消選取").clicked() {
                        state.select_all(false);
                    }
                });
            });
        });

    // 批次刪除確認對話方塊
    if state.show_batch_confirm_dialog {
        let sel_count = state.selected_count();
        egui::Window::new("確認批次刪除")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ui.ctx(), |ui| {
                ui.label(format!("確定要將選中的 {} 個音訊檔案永久刪除嗎？", sel_count));
                ui.label(RichText::new("此操作不可復原。").color(Color32::from_rgb(239, 68, 68)));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("確認刪除").clicked() {
                        let prev_selected = state.selected_file_path.clone();
                        match state.batch_delete_selected() {
                            Ok(cnt) => {
                                if prev_selected.is_some() && state.selected_file_path.is_none() {
                                    audio_player.stop();
                                }
                                state.status_feedback = Some(format!("成功刪除 {} 個檔案", cnt));
                                history.retain(|h| Path::new(&h.file_path).exists());
                                save_history_fn(history);
                            }
                            Err(e) => state.error_feedback = Some(e),
                        }
                        state.show_batch_confirm_dialog = false;
                    }
                    if ui.button("取消").clicked() {
                        state.show_batch_confirm_dialog = false;
                    }
                });
            });
    }

    // 重新命名對話方塊
    if let Some((old_path, ref mut new_name_input)) = state.rename_target.clone() {
        let mut close_dialog = false;
        let mut submit_rename = false;
        let mut current_input = new_name_input.clone();

        egui::Window::new("重新命名音訊檔案")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ui.ctx(), |ui| {
                ui.label(format!("原始檔案: {}", old_path.file_name().unwrap_or_default().to_string_lossy()));
                ui.add_space(4.0);
                ui.label("新檔案名稱:");
                ui.add(egui::TextEdit::singleline(&mut current_input).desired_width(280.0));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("確認變更").clicked() {
                        submit_rename = true;
                    }
                    if ui.button("取消").clicked() {
                        close_dialog = true;
                    }
                });
            });

        if let Some((_, ref mut input_ref)) = state.rename_target {
            *input_ref = current_input.clone();
        }

        if submit_rename {
            match state.rename_file(&old_path, &current_input) {
                Ok(new_path) => {
                    state.status_feedback = Some(format!("檔案已更名為: {}", new_path.file_name().unwrap_or_default().to_string_lossy()));
                    // 更新歷史紀錄中的路徑
                    for h in history.iter_mut() {
                        if Path::new(&h.file_path) == old_path {
                            h.file_path = new_path.to_string_lossy().to_string();
                        }
                    }
                    save_history_fn(history);
                    state.rename_target = None;
                }
                Err(e) => {
                    state.error_feedback = Some(e);
                }
            }
        } else if close_dialog {
            state.rename_target = None;
        }
    }

    ui.add_space(8.0);

    // ==================== 專用音訊播放器控制面板 ====================
    let selected_entry = state
        .selected_file_path
        .as_ref()
        .and_then(|p| state.files.iter().find(|f| &f.path == p));

    egui::Frame::group(ui.style())
        .fill(Color32::from_rgba_premultiplied(30, 41, 59, 50))
        .stroke(Stroke::new(1.0, Color32::from_rgb(99, 102, 241)))
        .corner_radius(8)
        .inner_margin(12.0)
        .show(ui, |ui| {
            let is_playing = audio_player.is_playing();
            let is_paused = audio_player.is_paused();
            let has_source = audio_player.current_bytes().is_some() || selected_entry.is_some();

            ui.horizontal(|ui| {
                let status_badge = if is_playing {
                    RichText::new("🔊 播放中").color(Color32::from_rgb(34, 197, 94)).strong()
                } else if is_paused {
                    RichText::new("⏸ 暫停中").color(Color32::from_rgb(245, 158, 11)).strong()
                } else {
                    RichText::new("⏹ 已停止").color(Color32::from_rgb(156, 163, 175)).strong()
                };
                ui.label(status_badge);

                if let Some(entry) = selected_entry {
                    ui.label(
                        RichText::new(format!("「{}」", entry.filename))
                            .strong()
                            .color(Color32::from_rgb(224, 231, 255)),
                    );
                    if !entry.character_name.is_empty() && entry.character_name != "未登錄角色" {
                        ui.label(
                            RichText::new(format!("({})", entry.character_name))
                                .color(Color32::from_rgb(129, 140, 248))
                                .size(11.5),
                        );
                    }
                } else {
                    ui.label(
                        RichText::new("尚未選取檔案 (點擊下方列表 ▶ 播放即可開始試聽)")
                            .color(Color32::from_rgb(156, 163, 175))
                            .size(11.5),
                    );
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // 音量控制
                    ui.label("🔊");
                    let mut vol = audio_player.get_volume();
                    if ui.add(egui::Slider::new(&mut vol, 0.0..=1.0).show_value(false)).changed() {
                        audio_player.set_volume(vol);
                    }

                    ui.add_space(10.0);

                    // 傳送至時間軸按鈕
                    if let Some(entry) = selected_entry
                        && ui.button("🎞️ 時間軸").on_hover_text("將目前選取的音訊檔案放入多軌時間軸").clicked()
                    {
                        send_timeline_file = Some(entry.path.clone());
                    }

                    // 快速另存
                    if let Some(entry) = selected_entry
                        && ui.button("💾 匯出目前").on_hover_text("另存目前選取的音訊檔案").clicked()
                        && let Ok(bytes) = fs::read(&entry.path)
                    {
                        let ext = entry.format.to_lowercase();
                        if let Some(target_path) = rfd::FileDialog::new()
                            .set_file_name(&entry.filename)
                            .add_filter("Audio File", &[ext.as_str(), "mp3", "wav"])
                            .save_file()
                        {
                            match export_audio_bytes(&bytes, &target_path) {
                                Ok(_) => {
                                    state.status_feedback = Some(format!(
                                        "已成功匯出至: {}",
                                        target_path.display()
                                    ));
                                }
                                Err(e) => {
                                    state.error_feedback = Some(format!("匯出失敗: {}", e));
                                }
                            }
                        }
                    }

                    // 重播按鈕
                    if ui
                        .add_enabled(has_source, egui::Button::new("🔁 重播"))
                        .on_hover_text("從頭重播當前音訊")
                        .clicked()
                    {
                        if audio_player.current_bytes().is_some() {
                            let _ = audio_player.replay();
                        } else if let Some(entry) = selected_entry
                            && let Ok(bytes) = fs::read(&entry.path)
                        {
                            let _ = audio_player.play_bytes(bytes);
                        }
                    }

                    // 停止按鈕
                    if ui
                        .add_enabled(is_playing || is_paused, egui::Button::new("⏹ 停止"))
                        .on_hover_text("停止播放")
                        .clicked()
                    {
                        audio_player.stop();
                    }

                    // 播放 / 暫停按鈕
                    let play_pause_icon = if is_playing { "⏸ 暫停" } else { "▶ 播放" };
                    if ui
                        .add_enabled(has_source, egui::Button::new(play_pause_icon).min_size(Vec2::new(70.0, 24.0)))
                        .on_hover_text("播放或暫停當前音訊")
                        .clicked()
                    {
                        if is_playing {
                            audio_player.pause();
                        } else if is_paused {
                            audio_player.resume();
                        } else if audio_player.current_bytes().is_some() {
                            let _ = audio_player.replay();
                        } else if let Some(entry) = selected_entry
                            && let Ok(bytes) = fs::read(&entry.path)
                        {
                            let _ = audio_player.play_bytes(bytes);
                        }
                    }
                });
            });

            ui.add_space(6.0);

            // 進度條與時間顯示
            ui.horizontal(|ui| {
                let elapsed = audio_player.elapsed();
                let total = audio_player.total_duration().unwrap_or(Duration::ZERO);
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

                let bar_width = (ui.available_width() - 20.0).max(100.0);
                if total_secs > 0.0 {
                    let slider = egui::Slider::new(&mut current_secs, 0.0..=total_secs)
                        .show_value(false);
                    let response = ui.add_sized([bar_width, 16.0], slider);
                    if response.drag_stopped() {
                        let _ = audio_player.seek(Duration::from_secs_f32(current_secs));
                    }
                } else {
                    let progress_bar = egui::ProgressBar::new(0.0).animate(is_playing);
                    ui.add_sized([bar_width, 16.0], progress_bar);
                }
            });
        });

    ui.add_space(6.0);

    // ==================== 檔案資料表格 ====================
    let filtered_indices: Vec<usize> = {
        let query = state.search_query.trim().to_lowercase();
        state
            .files
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                let match_format = match state.format_filter {
                    FormatFilter::All => true,
                    FormatFilter::Mp3 => item.format.eq_ignore_ascii_case("mp3"),
                    FormatFilter::Wav => item.format.eq_ignore_ascii_case("wav"),
                    FormatFilter::Other => {
                        !item.format.eq_ignore_ascii_case("mp3") && !item.format.eq_ignore_ascii_case("wav")
                    }
                };

                if !match_format {
                    return false;
                }

                if query.is_empty() {
                    return true;
                }

                item.filename.to_lowercase().contains(&query)
                    || item.text_snippet.to_lowercase().contains(&query)
                    || item.character_name.to_lowercase().contains(&query)
                    || item.model.to_lowercase().contains(&query)
            })
            .map(|(idx, _)| idx)
            .collect()
    };

    if filtered_indices.is_empty() {
        egui::Frame::group(ui.style())
            .corner_radius(8)
            .inner_margin(24.0)
            .show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("📂 沒有找到符合條件的音訊檔案").size(16.0).color(Color32::from_rgb(156, 163, 175)));
                    ui.add_space(4.0);
                    ui.label(RichText::new("可前往「單人語音生成」或「多角色對白生成」建立新作品，或點擊上方「重新整理」按鈕。").size(12.0).color(Color32::from_rgb(107, 114, 128)));
                });
            });
    } else {
        egui::ScrollArea::both()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let mut play_file_path = None;
                let mut rename_file_info = None;
                let mut export_file_info = None;
                let mut delete_file_path = None;

                egui::Grid::new("audio_files_grid")
                    .striped(true)
                    .min_col_width(50.0)
                    .spacing(Vec2::new(10.0, 8.0))
                    .show(ui, |ui| {
                        // 表頭
                        ui.strong("選取");
                        ui.strong("格式");
                        ui.strong("檔案名稱");
                        ui.strong("配音角色");
                        ui.strong("台詞摘要");
                        ui.strong("時長");
                        ui.strong("大小");
                        ui.strong("建立時間");
                        ui.strong("操作管理");
                        ui.end_row();

                        for &idx in &filtered_indices {
                            let item = &mut state.files[idx];

                            // 1. Checkbox
                            ui.checkbox(&mut item.is_selected, "");

                            // 2. 格式標籤 (Pill)
                            let pill_color = match item.format.as_str() {
                                "MP3" => Color32::from_rgb(59, 130, 246),  // Blue
                                "WAV" => Color32::from_rgb(16, 185, 129), // Green
                                _ => Color32::from_rgb(139, 92, 246),     // Purple
                            };
                            ui.label(
                                RichText::new(&item.format)
                                    .size(11.0)
                                    .strong()
                                    .color(pill_color),
                            );

                            // 3. 檔名
                            ui.label(RichText::new(&item.filename).strong());

                            // 4. 角色
                            ui.label(
                                RichText::new(&item.character_name)
                                    .color(Color32::from_rgb(99, 102, 241)),
                            );

                            // 5. 台詞摘要 (最多截取 28 字)
                            let text_preview = if item.text_snippet.chars().count() > 28 {
                                format!("{}...", item.text_snippet.chars().take(28).collect::<String>())
                            } else {
                                item.text_snippet.clone()
                            };
                            ui.label(text_preview).on_hover_text(&item.text_snippet);

                            // 6. 時長
                            ui.label(item.formatted_duration());

                            // 7. 大小
                            ui.label(item.formatted_size());

                            // 8. 建立時間
                            ui.label(RichText::new(&item.modified_str).size(11.0));

                            // 9. 操作群組
                            ui.horizontal(|ui| {
                                // 播放按鈕
                                let is_this_selected = state.selected_file_path.as_ref() == Some(&item.path);
                                let is_this_playing = is_this_selected && audio_player.is_playing();
                                let is_this_paused = is_this_selected && audio_player.is_paused();

                                let btn_text = if is_this_playing {
                                    "⏸ 暫停"
                                } else if is_this_paused {
                                    "▶ 繼續"
                                } else {
                                    "▶ 播放"
                                };

                                let play_btn = egui::Button::new(
                                    if is_this_selected {
                                        RichText::new(btn_text).color(Color32::from_rgb(129, 140, 248)).strong()
                                    } else {
                                        RichText::new(btn_text)
                                    }
                                );

                                if ui.add(play_btn).on_hover_text("播放、暫停或繼續此音訊").clicked() {
                                    if is_this_playing {
                                        audio_player.pause();
                                    } else if is_this_paused {
                                        audio_player.resume();
                                    } else {
                                        play_file_path = Some(item.path.clone());
                                    }
                                }

                                // 重新命名
                                if ui.button("✏️ 更名").on_hover_text("修改此音檔名稱").clicked() {
                                    let stem = item.path.file_stem().unwrap_or_default().to_string_lossy().to_string();
                                    rename_file_info = Some((item.path.clone(), stem));
                                }

                                // 匯出另存
                                if ui.button("💾 匯出").on_hover_text("另存音檔至自訂路徑 (支援 WAV/MP3 格式轉換)").clicked() {
                                    export_file_info = Some((item.path.clone(), item.filename.clone(), item.format.to_lowercase()));
                                }

                                // 傳送至時間軸
                                if ui.button("🎞️ 時間軸").on_hover_text("將此音檔放置於多軌時間軸編輯").clicked() {
                                    send_timeline_file = Some(item.path.clone());
                                }

                                // 刪除按鈕
                                let del_btn = egui::Button::new(RichText::new("🗑️").color(Color32::from_rgb(239, 68, 68)));
                                if ui.add(del_btn).on_hover_text("刪除此檔案").clicked() {
                                    delete_file_path = Some(item.path.clone());
                                }
                            });

                            ui.end_row();
                        }
                    });

                // 處理單一播放
                if let Some(path) = play_file_path {
                    match fs::read(&path) {
                        Ok(bytes) => {
                            let _ = audio_player.play_bytes(bytes);
                            state.selected_file_path = Some(path.clone());
                            state.status_feedback = Some(format!("正在播放: {}", path.file_name().unwrap_or_default().to_string_lossy()));
                        }
                        Err(e) => {
                            state.error_feedback = Some(format!("讀取音訊檔案失敗: {}", e));
                        }
                    }
                }

                // 處理重新命名觸發
                if let Some((path, name)) = rename_file_info {
                    state.rename_target = Some((path, name));
                }

                // 處理匯出 (採用 export_audio_bytes，支援格式正確轉碼)
                if let Some((source_path, filename, ext)) = export_file_info
                    && let Some(target_path) = rfd::FileDialog::new()
                        .set_file_name(&filename)
                        .add_filter("Audio File", &[ext.as_str(), "mp3", "wav"])
                        .save_file()
                {
                    match fs::read(&source_path) {
                        Ok(bytes) => {
                            match export_audio_bytes(&bytes, &target_path) {
                                Ok(_) => {
                                    state.status_feedback = Some(format!("已成功匯出至: {}", target_path.display()));
                                }
                                Err(e) => {
                                    state.error_feedback = Some(format!("匯出檔案失敗: {}", e));
                                }
                            }
                        }
                        Err(e) => {
                            state.error_feedback = Some(format!("讀取來源檔案失敗: {}", e));
                        }
                    }
                }

                // 處理單一刪除 (若正在播放該檔案則停止播放)
                if let Some(path) = delete_file_path {
                    match fs::remove_file(&path) {
                        Ok(_) => {
                            if state.selected_file_path.as_ref() == Some(&path) {
                                audio_player.stop();
                                state.selected_file_path = None;
                            }
                            state.files.retain(|f| f.path != path);
                            history.retain(|h| Path::new(&h.file_path) != path);
                            save_history_fn(history);
                            state.status_feedback = Some(format!("已刪除檔案: {}", path.file_name().unwrap_or_default().to_string_lossy()));
                        }
                        Err(e) => {
                            state.error_feedback = Some(format!("刪除檔案失敗: {}", e));
                        }
                    }
                }
            });
    }

    send_timeline_file
        .map(FileManagerAction::SendToTimeline)
        .unwrap_or(FileManagerAction::None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_entry_formatters() {
        let entry = AudioFileEntry {
            filename: "speech_test.mp3".to_string(),
            path: PathBuf::from("outputs/speech_test.mp3"),
            byte_size: 2048,
            modified_time: SystemTime::now(),
            modified_str: "2026-09-05 02:00:00".to_string(),
            duration_secs: Some(65.4),
            format: "MP3".to_string(),
            character_name: "溫柔御姐".to_string(),
            text_snippet: "這是一段測試台詞".to_string(),
            model: "fish-audio/s2.1-pro-free:free".to_string(),
            is_selected: false,
        };

        assert_eq!(entry.formatted_size(), "2.0 KB");
        assert_eq!(entry.formatted_duration(), "01:05");
    }

    #[test]
    fn test_file_manager_sorting_and_selection() {
        let mut state = FileManagerState::new();

        let e1 = AudioFileEntry {
            filename: "b_speech.mp3".to_string(),
            path: PathBuf::from("outputs/b_speech.mp3"),
            byte_size: 5000,
            modified_time: SystemTime::now(),
            modified_str: "2026-09-05 01:00:00".to_string(),
            duration_secs: Some(10.0),
            format: "MP3".to_string(),
            character_name: "青年".to_string(),
            text_snippet: "你好".to_string(),
            model: "model".to_string(),
            is_selected: false,
        };

        let e2 = AudioFileEntry {
            filename: "a_speech.wav".to_string(),
            path: PathBuf::from("outputs/a_speech.wav"),
            byte_size: 15000,
            modified_time: SystemTime::now() + std::time::Duration::from_secs(10),
            modified_str: "2026-09-05 01:00:10".to_string(),
            duration_secs: Some(30.0),
            format: "WAV".to_string(),
            character_name: "少年".to_string(),
            text_snippet: "戰鬥開始".to_string(),
            model: "model".to_string(),
            is_selected: false,
        };

        state.files = vec![e1, e2];

        // 依檔名 A-Z
        state.sort_by = FileSortBy::NameAsc;
        state.apply_sort();
        assert_eq!(state.files[0].filename, "a_speech.wav");

        // 依大小 由大到小
        state.sort_by = FileSortBy::SizeDesc;
        state.apply_sort();
        assert_eq!(state.files[0].byte_size, 15000);

        // 選取與反選
        state.select_all(true);
        assert_eq!(state.selected_count(), 2);
        state.invert_selection();
        assert_eq!(state.selected_count(), 0);
    }
}
