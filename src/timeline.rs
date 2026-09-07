use crate::api::{OpenRouterClient, SpeechRequest, DEFAULT_MODEL};
use crate::audio::{decode_to_pcm, encode_pcm_to_wav, AudioPlayer};
use crate::models::CharacterPreset;
use chrono::Local;
use eframe::egui;
use egui::{Color32, Pos2, Rect, RichText, Stroke, Vec2};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

/// 軌道類型
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum TrackType {
    Dialogue,  // 角色對白
    VoiceOver, // 旁白解說
    Bgm,       // 背景音樂
    Sfx,       // 環境音效
}

impl TrackType {
    pub fn label(&self) -> &'static str {
        match self {
            TrackType::Dialogue => "角色對白",
            TrackType::VoiceOver => "旁白解說",
            TrackType::Bgm => "背景音樂 BGM",
            TrackType::Sfx => "環境音效 SFX",
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            TrackType::Dialogue => "🎙️",
            TrackType::VoiceOver => "📖",
            TrackType::Bgm => "🎵",
            TrackType::Sfx => "🔊",
        }
    }

    pub fn default_color(&self) -> [u8; 3] {
        match self {
            TrackType::Dialogue => [59, 130, 246],  // Blue
            TrackType::VoiceOver => [168, 85, 247], // Purple
            TrackType::Bgm => [16, 185, 129],       // Emerald Green
            TrackType::Sfx => [249, 115, 22],       // Orange
        }
    }
}

/// 多軌時間軸上的軌道定義
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TimelineTrack {
    pub id: usize,
    pub name: String,
    pub track_type: TrackType,
    pub volume: f32, // 0.0 ~ 2.0 (預設 1.0)
    pub is_muted: bool,
    pub is_solo: bool,
    pub color: [u8; 3],
    pub height: f32, // 軌道像素高度
}

/// 放置在時間軸上的音訊片段
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TimelineClip {
    pub id: u64,
    pub track_id: usize,
    pub name: String,
    pub speaker: String,
    #[serde(default)]
    pub voice_id: Option<String>,
    #[serde(default)]
    pub prompt_tag: Option<String>,
    pub text: String,
    pub start_sec: f32,        // 時間軸起始秒數
    pub duration_sec: f32,     // 時間軸佔用秒數 (受 trim 與 speed 影響)
    pub raw_duration_sec: f32, // 原始未剪裁長度
    pub trim_start_sec: f32,   // 開頭修剪秒數
    pub trim_end_sec: f32,     // 結尾修剪秒數
    pub gain: f32,             // 音量增益 (0.0 ~ 2.0, 預設 1.0)
    pub speed: f32,            // 播放速度 (0.5 ~ 2.0, 預設 1.0)
    pub color: [u8; 3],
    #[serde(skip)]
    pub audio_bytes: Option<Vec<u8>>,
    pub file_path: Option<String>,
    #[serde(skip)]
    pub pcm_samples: Option<Vec<i16>>,
    pub sample_rate: u32,
    pub channels: u16,
    pub waveform_peaks: Vec<f32>, // 預先採樣的正規化振幅峰值 (0.0 ~ 1.0)
}

impl TimelineClip {
    /// 建立全新音訊片段
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: u64,
        track_id: usize,
        name: String,
        speaker: String,
        text: String,
        start_sec: f32,
        audio_bytes: Vec<u8>,
        file_path: Option<String>,
        color: [u8; 3],
    ) -> Result<Self, String> {
        let (pcm_samples, sample_rate, channels) = decode_to_pcm(&audio_bytes)?;
        let total_frames = pcm_samples.len() / (channels as usize).max(1);
        let raw_duration_sec = (total_frames as f32 / sample_rate as f32).max(0.05);

        let waveform_peaks = Self::calculate_waveform_peaks(&pcm_samples, channels, 120);

        Ok(Self {
            id,
            track_id,
            name,
            speaker,
            voice_id: None,
            prompt_tag: None,
            text,
            start_sec,
            duration_sec: raw_duration_sec,
            raw_duration_sec,
            trim_start_sec: 0.0,
            trim_end_sec: 0.0,
            gain: 1.0,
            speed: 1.0,
            color,
            audio_bytes: Some(audio_bytes),
            file_path,
            pcm_samples: Some(pcm_samples),
            sample_rate,
            channels,
            waveform_peaks,
        })
    }

    /// 從 PCM 與時長建立虛擬片段 (用於測試或占位)
    #[allow(clippy::too_many_arguments)]
    pub fn new_mock(
        id: u64,
        track_id: usize,
        name: String,
        speaker: String,
        text: String,
        start_sec: f32,
        duration_sec: f32,
        color: [u8; 3],
    ) -> Self {
        let sample_rate = 44100;
        let channels = 1;
        let waveform_peaks = vec![0.0; 120];

        Self {
            id,
            track_id,
            name,
            speaker,
            voice_id: None,
            prompt_tag: None,
            text,
            start_sec,
            duration_sec,
            raw_duration_sec: duration_sec,
            trim_start_sec: 0.0,
            trim_end_sec: 0.0,
            gain: 1.0,
            speed: 1.0,
            color,
            audio_bytes: None,
            file_path: None,
            pcm_samples: None,
            sample_rate,
            channels,
            waveform_peaks,
        }
    }

    /// 重新計算當前有效時長並同步修剪後的波形峰值
    pub fn recalculate_duration(&mut self) {
        let active_raw = (self.raw_duration_sec - self.trim_start_sec - self.trim_end_sec).max(0.05);
        let speed = self.speed.clamp(0.5, 2.0);
        self.duration_sec = active_raw / speed;

        if let Some(pcm) = &self.pcm_samples {
            self.waveform_peaks = Self::calculate_waveform_peaks_sliced(
                pcm,
                self.channels,
                self.trim_start_sec,
                self.trim_end_sec,
                self.sample_rate,
                120,
            );
        }
    }

    /// 根據 PCM 計算規一化波形峰值點
    pub fn calculate_waveform_peaks(pcm: &[i16], channels: u16, num_peaks: usize) -> Vec<f32> {
        Self::calculate_waveform_peaks_sliced(pcm, channels, 0.0, 0.0, 44100, num_peaks)
    }

    /// 根據修剪範圍計算特定區間的波形峰值點
    pub fn calculate_waveform_peaks_sliced(
        pcm: &[i16],
        channels: u16,
        trim_start_sec: f32,
        trim_end_sec: f32,
        sample_rate: u32,
        num_peaks: usize,
    ) -> Vec<f32> {
        if pcm.is_empty() || num_peaks == 0 {
            return vec![0.1; num_peaks];
        }

        let ch = (channels as usize).max(1);
        let sr = sample_rate.max(8000) as f32;
        let total_frames = pcm.len() / ch;
        let trim_start_f = ((trim_start_sec * sr) as usize).min(total_frames);
        let trim_end_f = ((trim_end_sec * sr) as usize).min(total_frames.saturating_sub(trim_start_f));
        let active_frames = total_frames.saturating_sub(trim_start_f + trim_end_f);

        if active_frames == 0 {
            return vec![0.05; num_peaks];
        }

        let frames_per_bin = (active_frames / num_peaks).max(1);
        let mut peaks = Vec::with_capacity(num_peaks);

        for i in 0..num_peaks {
            let start_f = trim_start_f + i * frames_per_bin;
            let end_f = (trim_start_f + (i + 1) * frames_per_bin).min(trim_start_f + active_frames);
            let mut max_amp: f32 = 0.0;

            for f in start_f..end_f {
                let sample = pcm[f * ch];
                let norm = (sample.abs() as f32) / 32768.0;
                if norm > max_amp {
                    max_amp = norm;
                }
            }

            peaks.push(max_amp.clamp(0.05, 1.0));
        }

        peaks
    }

    /// 在指定時間軸秒數處裁切片段為兩部分 (Split Clip)
    pub fn split_at(&self, split_timeline_sec: f32, next_id: u64) -> Result<(TimelineClip, TimelineClip), String> {
        let clip_end = self.start_sec + self.duration_sec;
        if split_timeline_sec <= self.start_sec + 0.05 || split_timeline_sec >= clip_end - 0.05 {
            return Err("裁切點太靠近片段邊緣或超出片段範圍".to_string());
        }

        let offset_active = split_timeline_sec - self.start_sec;
        let offset_raw = offset_active * self.speed;
        let split_raw_point = self.trim_start_sec + offset_raw;

        // 前段
        let mut first = self.clone();
        first.trim_end_sec = (self.raw_duration_sec - split_raw_point).max(0.0);
        first.name = format!("{} (前段)", self.name);
        first.recalculate_duration();

        // 後段
        let mut second = self.clone();
        second.id = next_id;
        second.start_sec = split_timeline_sec;
        second.trim_start_sec = split_raw_point;
        second.name = format!("{} (後段)", self.name);
        second.recalculate_duration();

        Ok((first, second))
    }
}

/// 拖曳互動狀態機
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DragMode {
    None,
    MoveClip {
        clip_id: u64,
        initial_start_sec: f32,
        initial_track_id: usize,
        pointer_start_x: f32,
        pointer_start_y: f32,
    },
    TrimLeft {
        clip_id: u64,
        initial_trim_start: f32,
        initial_start_sec: f32,
        pointer_start_x: f32,
    },
    TrimRight {
        clip_id: u64,
        initial_trim_end: f32,
        pointer_start_x: f32,
    },
    ScrubPlayhead,
}

/// 異步生成 TTS 片段的訊息
pub enum TimelineWorkerMessage {
    TtsClipGenerated {
        track_id: usize,
        name: String,
        speaker: String,
        text: String,
        start_sec: f32,
        result: Result<Vec<u8>, String>,
    },
    ClipAudioSynthesized {
        clip_id: u64,
        result: Result<Vec<u8>, String>,
    },
}

/// 時間軸編輯器完整狀態
pub struct TimelineState {
    pub tracks: Vec<TimelineTrack>,
    pub clips: Vec<TimelineClip>,
    pub next_clip_id: u64,
    pub next_track_id: usize,

    // 播放頭與播放器
    pub playhead_sec: f32,
    pub is_playing: bool,
    pub playback_start_instant: Option<Instant>,
    pub playback_start_sec: f32,
    pub active_playing_clip_id: Option<u64>,

    // 選取與縮放滾動
    pub selected_clip_id: Option<u64>,
    pub zoom_px_per_sec: f32, // 每秒像素數 (20 ~ 250)
    pub scroll_offset_x: f32,

    // 拖曳互動
    pub drag_mode: DragMode,

    // 狀態通知
    pub status_message: String,
    pub error_message: Option<String>,
    pub success_toast: Option<(String, Instant)>,

    // TTS 生成狀態
    pub synthesizing_clip_id: Option<u64>,
    pub external_generation_busy: bool,

    // 新增 TTS 模態視窗狀態
    pub show_tts_modal: bool,
    pub tts_modal_text: String,
    pub tts_modal_speaker_name: String,
    pub tts_modal_track_id: usize,
    pub tts_modal_character_idx: usize,
    pub tts_modal_start_sec: f32,
    pub tts_modal_is_generating: bool,

    // 通訊通道
    pub sender: Sender<TimelineWorkerMessage>,
    pub receiver: Receiver<TimelineWorkerMessage>,
}

impl Default for TimelineState {
    fn default() -> Self {
        Self::new()
    }
}

impl TimelineState {
    pub fn new() -> Self {
        let (sender, receiver) = channel();

        // 預設建立四條專業軌道
        let tracks = vec![
            TimelineTrack {
                id: 0,
                name: "🎙️ 主角對白軌 (Speaker A)".to_string(),
                track_type: TrackType::Dialogue,
                volume: 1.0,
                is_muted: false,
                is_solo: false,
                color: [59, 130, 246],
                height: 72.0,
            },
            TimelineTrack {
                id: 1,
                name: "🎙️ 配角對白軌 (Speaker B)".to_string(),
                track_type: TrackType::Dialogue,
                volume: 1.0,
                is_muted: false,
                is_solo: false,
                color: [249, 115, 22],
                height: 72.0,
            },
            TimelineTrack {
                id: 2,
                name: "📖 旁白朗讀軌 (Narrator)".to_string(),
                track_type: TrackType::VoiceOver,
                volume: 0.95,
                is_muted: false,
                is_solo: false,
                color: [168, 85, 247],
                height: 72.0,
            },
            TimelineTrack {
                id: 3,
                name: "🎵 背景音樂軌 (BGM / Ambience)".to_string(),
                track_type: TrackType::Bgm,
                volume: 0.6,
                is_muted: false,
                is_solo: false,
                color: [16, 185, 129],
                height: 64.0,
            },
        ];

        Self {
            tracks,
            clips: Vec::new(),
            next_clip_id: 1,
            next_track_id: 4,
            playhead_sec: 0.0,
            is_playing: false,
            playback_start_instant: None,
            playback_start_sec: 0.0,
            active_playing_clip_id: None,
            selected_clip_id: None,
            zoom_px_per_sec: 80.0,
            scroll_offset_x: 0.0,
            drag_mode: DragMode::None,
            status_message: "就緒。拖曳音訊片段、剪切、調整音量或點擊「新增 TTS 片段」開始編輯。".to_string(),
            error_message: None,
            success_toast: None,
            synthesizing_clip_id: None,
            external_generation_busy: false,
            show_tts_modal: false,
            tts_modal_text: "[calm] 很久很久以前，在遙遠的森林深處……".to_string(),
            tts_modal_speaker_name: "旁白".to_string(),
            tts_modal_track_id: 2,
            tts_modal_character_idx: 5, // 大氣旁白
            tts_modal_start_sec: 0.0,
            tts_modal_is_generating: false,
            sender,
            receiver,
        }
    }

    pub fn is_busy(&self) -> bool {
        self.external_generation_busy || self.tts_modal_is_generating || self.synthesizing_clip_id.is_some()
    }

    pub fn ensure_dialogue_track(&mut self) -> usize {
        if let Some(track) = self.tracks.iter().find(|t| t.track_type == TrackType::Dialogue) {
            return track.id;
        }
        let id = self.next_track_id;
        self.add_track("語音".to_string(), TrackType::Dialogue);
        id
    }

    /// 載入故事範本至時間軸多軌工程
    pub fn load_story_template_project(&mut self, template: &crate::storytelling::StoryScriptTemplate) {
        if self.is_busy() { self.error_message = Some("請等待語音生成完成後再修改片段結構。".to_string()); return; }
        self.clips.clear();
        self.next_clip_id = 1;

        let mut speaker_tracks: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        speaker_tracks.insert("旁白".to_string(), 2);
        speaker_tracks.insert("說書人".to_string(), 2);
        speaker_tracks.insert("史詩旁白".to_string(), 2);

        let mut current_sec = 0.0f32;
        for line in &template.lines {
            let track_id = if line.is_narrator {
                2 // 旁白軌
            } else {
                *speaker_tracks.entry(line.speaker.clone()).or_insert_with(|| {
                    if self.tracks.len() > 4 {
                        let tid = self.next_track_id;
                        self.next_track_id += 1;
                        self.tracks.push(TimelineTrack {
                            id: tid,
                            name: format!("🎙️ {}", line.speaker),
                            track_type: TrackType::Dialogue,
                            volume: 1.0,
                            is_muted: false,
                            is_solo: false,
                            color: [59, 130, 246],
                            height: 70.0,
                        });
                        tid
                    } else if line.speaker.contains("主角") || line.speaker.contains("劍客") || line.speaker.contains("探險者") || line.speaker.contains("狐狸") || line.speaker.contains("騎士") {
                        0 // 主角軌
                    } else {
                        1 // 配角軌
                    }
                })
            };

            let dur = (line.text.chars().count() as f32 * 0.22).max(1.8);
            let tag_prefix = line.tags.iter().map(|t| {
                let clean = t.trim().trim_start_matches('[').trim_end_matches(']');
                format!("[{}]", clean)
            }).collect::<Vec<_>>().join("");
            let formatted_text = if tag_prefix.is_empty() { line.text.clone() } else { format!("{} {}", tag_prefix, line.text) };
            let color = line.intensity.color();

            let clip = TimelineClip::new_mock(
                self.next_clip_id,
                track_id,
                format!("{}_{}", line.speaker, self.next_clip_id),
                line.speaker.clone(),
                formatted_text,
                current_sec,
                dur,
                color,
            );
            self.next_clip_id += 1;
            self.clips.push(clip);

            current_sec += dur + (line.pause_after_ms as f32 / 1000.0);
        }

        self.playhead_sec = 0.0;
        self.status_message = format!("已成功載入劇本《{}》共 {} 個語音片段！點擊片段即可試聽或為草稿片段生成真實 TTS。", template.title, self.clips.len());
        self.success_toast = Some((format!("已載入故事劇本: {}", template.title), Instant::now()));
    }

    /// 載入內建示範工程 (包含對白、旁白與環境音效)
    pub fn load_demo_project(&mut self) {
        if self.is_busy() { self.error_message = Some("請等待語音生成完成後再修改片段結構。".to_string()); return; }
        self.clips.clear();
        self.next_clip_id = 1;

        // Clip 1: 旁白開場 (Track 2)
        let clip1 = TimelineClip::new_mock(
            self.next_clip_id,
            2,
            "旁白開場敘事".to_string(),
            "大氣旁白".to_string(),
            "[calm] 夜幕低垂，古道上沒有半點人聲。".to_string(),
            0.0,
            3.5,
            [168, 85, 247],
        );
        self.next_clip_id += 1;
        self.clips.push(clip1);

        // Clip 2: 主角對白 (Track 0)
        let clip2 = TimelineClip::new_mock(
            self.next_clip_id,
            0,
            "主角警惕".to_string(),
            "溫柔御姐".to_string(),
            "[whispering] 等等……前面好像有動靜。".to_string(),
            3.8,
            2.6,
            [59, 130, 246],
        );
        self.next_clip_id += 1;
        self.clips.push(clip2);

        // Clip 3: 配角回應 (Track 1)
        let clip3 = TimelineClip::new_mock(
            self.next_clip_id,
            1,
            "配角拔劍".to_string(),
            "沉穩青年".to_string(),
            "[serious] 拔劍吧，是暗影刺客！".to_string(),
            6.6,
            2.4,
            [249, 115, 22],
        );
        self.next_clip_id += 1;
        self.clips.push(clip3);

        // Clip 4: BGM 音樂軌 (Track 3)
        let clip4 = TimelineClip::new_mock(
            self.next_clip_id,
            3,
            "幽暗懸疑背景音樂".to_string(),
            "BGM".to_string(),
            "長篇氛圍低頻弦樂".to_string(),
            0.0,
            10.0,
            [16, 185, 129],
        );
        self.next_clip_id += 1;
        self.clips.push(clip4);

        self.playhead_sec = 0.0;
        self.status_message = "已成功載入示範時間軸工程！點擊播放或裁切試聽。".to_string();
    }

    /// 新增軌道
    pub fn add_track(&mut self, name: String, track_type: TrackType) {
        let id = self.next_track_id;
        self.next_track_id += 1;
        let color = track_type.default_color();

        self.tracks.push(TimelineTrack {
            id,
            name,
            track_type,
            volume: 1.0,
            is_muted: false,
            is_solo: false,
            color,
            height: 70.0,
        });
    }

    /// 刪除軌道與其上所有片段
    pub fn delete_track(&mut self, track_id: usize) {
        if self.is_busy() { self.error_message = Some("請等待語音生成完成後再修改片段結構。".to_string()); return; }
        if self.tracks.len() > 1 {
            self.tracks.retain(|t| t.id != track_id);
            self.clips.retain(|c| c.track_id != track_id);
            if self.selected_clip_id.is_some() && !self.clips.iter().any(|c| Some(c.id) == self.selected_clip_id) {
                self.selected_clip_id = None;
            }
        }
    }

    /// 刪除選取片段
    pub fn delete_selected_clip(&mut self) {
        if self.is_busy() { self.error_message = Some("請等待語音生成完成後再修改片段結構。".to_string()); return; }
        if let Some(sel_id) = self.selected_clip_id {
            self.clips.retain(|c| c.id != sel_id);
            self.selected_clip_id = None;
            self.status_message = "已刪除選取的音訊片段".to_string();
        }
    }

    /// 複製選取片段 (貼在後面)
    pub fn duplicate_selected_clip(&mut self) {
        if let Some(sel_id) = self.selected_clip_id
            && let Some(clip) = self.clips.iter().find(|c| c.id == sel_id).cloned()
        {
            let mut new_clip = clip;
            new_clip.id = self.next_clip_id;
            self.next_clip_id += 1;
            new_clip.start_sec += new_clip.duration_sec + 0.1;
            new_clip.name = format!("{} (副本)", new_clip.name);
            self.clips.push(new_clip.clone());
            self.selected_clip_id = Some(new_clip.id);
            self.status_message = format!("已複製片段至 {:.1}s", new_clip.start_sec);
        }
    }

    /// 在指針處剪切選取片段 (Split)
    pub fn split_selected_clip_at_playhead(&mut self) {
        if self.is_busy() { self.error_message = Some("請等待語音生成完成後再修改片段結構。".to_string()); return; }
        let playhead = self.playhead_sec;
        // 優先順序：若選取的片段恰好橫跨指針，則剪切選取片段；否則剪切指針涵蓋的任意片段
        let target_clip_id = if let Some(sel_id) = self.selected_clip_id
            && let Some(c) = self.clips.iter().find(|c| c.id == sel_id)
            && playhead > c.start_sec + 0.05
            && playhead < c.start_sec + c.duration_sec - 0.05
        {
            Some(sel_id)
        } else {
            self.clips
                .iter()
                .find(|c| playhead > c.start_sec + 0.05 && playhead < c.start_sec + c.duration_sec - 0.05)
                .map(|c| c.id)
        };

        if let Some(cid) = target_clip_id {
            if let Some(idx) = self.clips.iter().position(|c| c.id == cid) {
                let clip = &self.clips[idx];
                match clip.split_at(playhead, self.next_clip_id) {
                    Ok((first, second)) => {
                        self.next_clip_id += 1;
                        let second_id = second.id;
                        self.clips[idx] = first;
                        self.clips.insert(idx + 1, second);
                        self.selected_clip_id = Some(second_id);
                        self.status_message = format!("已在 {:.2}s 處將片段一分為二！", playhead);
                    }
                    Err(e) => {
                        self.error_message = Some(e);
                    }
                }
            }
        } else {
            self.error_message = Some("播放指針處沒有可剪切的音訊片段 (請確認指針位於片段有效時長內)".to_string());
        }
    }

    /// 為指定片段立即生成 Fish Audio TTS 語音
    pub fn synthesize_clip_speech(
        &mut self,
        clip_id: u64,
        api_key: &str,
        characters: &[CharacterPreset],
    ) {
        if self.is_busy() { return; }
        let key = api_key.trim().to_string();
        if key.is_empty() {
            self.error_message = Some("請先在單人語音頁面設定 OpenRouter API Key".to_string());
            return;
        }

        let clip = match self.clips.iter().find(|c| c.id == clip_id) {
            Some(c) => c,
            None => {
                self.error_message = Some("找不到指定片段".to_string());
                return;
            }
        };

        let raw_text = clip.text.trim().to_string();
        if raw_text.is_empty() {
            self.error_message = Some("片段台詞不可為空".to_string());
            return;
        }

        let char_preset = characters
            .iter()
            .find(|c| c.name == clip.speaker || clip.speaker.contains(&c.name))
            .cloned()
            .unwrap_or_else(|| characters[0].clone());

        let prompt_tag = clip.prompt_tag.as_deref().unwrap_or(&char_preset.prompt_tag);
        let speech_input = if !prompt_tag.is_empty() && !raw_text.contains(prompt_tag) {
            format!("{} {}", prompt_tag, raw_text)
        } else {
            raw_text.clone()
        };

        let req = SpeechRequest {
            model: DEFAULT_MODEL.to_string(),
            input: speech_input,
            voice: clip.voice_id.clone().or_else(|| char_preset.voice_id.clone()),
            response_format: Some("mp3".to_string()),
            speed: Some(1.0), // Timeline speed is applied by the mixer exactly once.
        };

        self.synthesizing_clip_id = Some(clip_id);
        self.status_message = format!("正在為片段「{}」合成語音...", clip.name);
        let tx = self.sender.clone();

        thread::spawn(move || {
            let client = OpenRouterClient::new();
            let res = client.synthesize(&key, &req);
            let _ = tx.send(TimelineWorkerMessage::ClipAudioSynthesized {
                clip_id,
                result: res,
            });
        });
    }

    /// 時間軸總時長 (所有片段中最大的 end_sec)
    pub fn total_timeline_duration(&self) -> f32 {
        self.clips
            .iter()
            .map(|c| c.start_sec + c.duration_sec)
            .fold(0.0, f32::max)
            .max(5.0)
    }

    /// 將時間軸上所有軌道與片段混音為 44.1kHz 立體聲 16-bit PCM
    pub fn mix_timeline_to_pcm(&self) -> Result<(Vec<i16>, u32, u16), String> {
        if self.clips.is_empty() {
            return Err("時間軸上沒有任何音訊片段，無法匯出".to_string());
        }

        let has_solo = self.tracks.iter().any(|t| t.is_solo);
        let unfinished = self.clips.iter().any(|c| c.pcm_samples.is_none()
            && self.tracks.iter().any(|t| t.id == c.track_id && !t.is_muted && (!has_solo || t.is_solo)));
        if unfinished {
            return Err("尚有草稿片段未生成音訊。請先生成，或將草稿所在軌道靜音。".to_string());
        }
        let target_sample_rate: u32 = 44100;
        let target_channels: u16 = 2;
        let total_dur = self.total_timeline_duration();
        let total_frames = ((total_dur + 0.5) * target_sample_rate as f32).ceil() as usize;

        // 雙聲道立體聲 master 緩衝區 (浮點數以防混合時溢位)
        let mut master_buffer = vec![0.0f32; total_frames * 2];

        // 檢查是否有軌道獨奏 (Solo)
        let has_solo = self.tracks.iter().any(|t| t.is_solo);

        for track in &self.tracks {
            // 若該軌被靜音，或有其他軌獨奏且本軌非獨奏，則跳過
            if track.is_muted || (has_solo && !track.is_solo) {
                continue;
            }

            let track_vol = track.volume;
            let track_clips: Vec<&TimelineClip> = self.clips.iter().filter(|c| c.track_id == track.id).collect();

            for clip in track_clips {
                let pcm = match &clip.pcm_samples {
                    Some(samples) => samples,
                    None => continue,
                };

                let clip_vol = track_vol * clip.gain;
                let src_ch = (clip.channels as usize).max(1);
                let src_sr = clip.sample_rate.max(8000) as f32;
                let speed = clip.speed.clamp(0.5, 2.0);

                let src_total_frames = pcm.len() / src_ch;
                let trim_start_f = ((clip.trim_start_sec * src_sr) as usize).min(src_total_frames);
                let trim_end_f = ((clip.trim_end_sec * src_sr) as usize).min(src_total_frames.saturating_sub(trim_start_f));
                let active_src_frames = src_total_frames.saturating_sub(trim_start_f + trim_end_f);

                if active_src_frames == 0 {
                    continue;
                }

                let dest_start_f = ((clip.start_sec * target_sample_rate as f32) as usize).min(total_frames);
                let dest_duration_f = ((clip.duration_sec * target_sample_rate as f32) as usize).min(total_frames.saturating_sub(dest_start_f));

                for df in 0..dest_duration_f {
                    let master_f = dest_start_f + df;
                    if master_f >= total_frames {
                        break;
                    }

                    // 依速度與取樣率重取樣映射至來源影格
                    let src_rel_f = (df as f32 * speed * (src_sr / target_sample_rate as f32)) as usize;
                    if src_rel_f >= active_src_frames {
                        break;
                    }

                    let src_idx = trim_start_f + src_rel_f;
                    let (s_left, s_right) = if src_ch >= 2 {
                        let base = src_idx * src_ch;
                        (pcm[base] as f32, pcm[base + 1] as f32)
                    } else {
                        let val = pcm[src_idx] as f32;
                        (val, val)
                    };

                    master_buffer[master_f * 2] += s_left * clip_vol;
                    master_buffer[master_f * 2 + 1] += s_right * clip_vol;
                }
            }
        }

        // 軟限制器 (Soft Limiter) 與立體聲輸出轉換
        let mut final_pcm = Vec::with_capacity(master_buffer.len());
        for sample in master_buffer {
            // tanh 曲線防爆音
            let compressed = (sample / 32768.0).tanh() * 32000.0;
            final_pcm.push(compressed.clamp(-32767.0, 32767.0) as i16);
        }

        Ok((final_pcm, target_sample_rate, target_channels))
    }

    /// 匯出全時間軸混音檔案 (WAV)
    pub fn export_timeline_mix(&self, target_path: &Path) -> Result<(), String> {
        let (pcm, sr, ch) = self.mix_timeline_to_pcm()?;
        let wav_bytes = encode_pcm_to_wav(&pcm, sr, ch);
        crate::audio::export_audio_bytes(&wav_bytes, target_path)?;
        Ok(())
    }

    /// 接收非同步 TTS 產生完成的片段
    pub fn poll_worker_messages(&mut self) {
        while let Ok(msg) = self.receiver.try_recv() {
            match msg {
                TimelineWorkerMessage::TtsClipGenerated {
                    track_id,
                    name,
                    speaker,
                    text,
                    start_sec,
                    result,
                } => {
                    self.tts_modal_is_generating = false;
                    match result {
                        Ok(bytes) => {
                            let track_color = self
                                .tracks
                                .iter()
                                .find(|t| t.id == track_id)
                                .map(|t| t.color)
                                .unwrap_or([59, 130, 246]);

                            match TimelineClip::new(
                                self.next_clip_id,
                                track_id,
                                name,
                                speaker,
                                text,
                                start_sec,
                                bytes,
                                None,
                                track_color,
                            ) {
                                Ok(clip) => {
                                    self.next_clip_id += 1;
                                    let new_id = clip.id;
                                    self.clips.push(clip);
                                    self.selected_clip_id = Some(new_id);
                                    self.show_tts_modal = false;
                                    self.status_message = format!("TTS 語音片段已成功生成並加入時間軸 (軌道 {})！", track_id);
                                    self.success_toast = Some((
                                        "TTS 語音片段已成功放入時間軸！".to_string(),
                                        Instant::now(),
                                    ));
                                }
                                Err(e) => {
                                    self.error_message = Some(format!("解碼生成音訊失敗: {}", e));
                                }
                            }
                        }
                        Err(e) => {
                            self.error_message = Some(format!("TTS 語音生成失敗: {}", e));
                        }
                    }
                }
                TimelineWorkerMessage::ClipAudioSynthesized { clip_id, result } => {
                    self.synthesizing_clip_id = None;
                    match result {
                        Ok(bytes) => {
                            if let Some(clip) = self.clips.iter_mut().find(|c| c.id == clip_id) {
                                match decode_to_pcm(&bytes) {
                                    Ok((pcm_samples, sample_rate, channels)) => {
                                        let total_frames = pcm_samples.len() / (channels as usize).max(1);
                                        let raw_duration_sec = (total_frames as f32 / sample_rate as f32).max(0.05);
                                        clip.audio_bytes = Some(bytes);
                                        clip.pcm_samples = Some(pcm_samples);
                                        clip.sample_rate = sample_rate;
                                        clip.channels = channels;
                                        clip.raw_duration_sec = raw_duration_sec;
                                        clip.trim_start_sec = 0.0;
                                        clip.trim_end_sec = 0.0;
                                        clip.recalculate_duration();
                                        self.status_message = format!("片段「{}」已成功合成 Fish Audio 語音！", clip.name);
                                        self.success_toast = Some((format!("片段「{}」語音合成完成！", clip.name), Instant::now()));
                                    }
                                    Err(e) => {
                                        self.error_message = Some(format!("解碼合成音訊失敗: {}", e));
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            self.error_message = Some(format!("片段語音合成失敗: {}", e));
                        }
                    }
                }
            }
        }
    }

    /// 更新播放進度與同步
    pub fn update_playback(&mut self, audio_player: &mut AudioPlayer) {
        if self.is_playing && self.drag_mode != DragMode::ScrubPlayhead
            && let Some(start_inst) = self.playback_start_instant {
                let elapsed = start_inst.elapsed().as_secs_f32();
                self.playhead_sec = self.playback_start_sec + elapsed;

                let max_dur = self.total_timeline_duration();
                if self.playhead_sec >= max_dur {
                    self.is_playing = false;
                    self.playback_start_instant = None;
                    self.playhead_sec = 0.0;
                    audio_player.stop();
                }
            }
    }

    /// 開始時間軸同步播放
    pub fn start_playback(&mut self, audio_player: &mut AudioPlayer) {
        if self.clips.is_empty() {
            self.error_message = Some("時間軸上沒有任何音訊片段可播放".to_string());
            return;
        }

        // 混音並開始播放
        match self.mix_timeline_to_pcm() {
            Ok((pcm, sr, ch)) => {
                let wav_bytes = encode_pcm_to_wav(&pcm, sr, ch);
                match audio_player.play_bytes(wav_bytes) {
                    Ok(()) => {
                        if self.playhead_sec > 0.05 {
                            if let Err(e) = audio_player.seek(Duration::from_secs_f32(self.playhead_sec)) {
                                audio_player.stop();
                                self.error_message = Some(e);
                                return;
                            }
                        }
                        self.is_playing = true;
                        self.playback_start_instant = Some(Instant::now());
                        self.playback_start_sec = self.playhead_sec;
                    }
                    Err(e) => {
                        self.is_playing = false;
                        self.playback_start_instant = None;
                        self.error_message = Some(format!("無法播放：{}。仍可匯出 WAV 混音。", e));
                    }
                }
            }
            Err(e) => {
                self.error_message = Some(format!("混音試聽失敗: {}", e));
            }
        }
    }

    /// 暫停時間軸播放
    pub fn pause_playback(&mut self, audio_player: &mut AudioPlayer) {
        self.is_playing = false;
        self.playback_start_instant = None;
        audio_player.pause();
    }

    /// 停止播放並回到開頭
    pub fn stop_playback(&mut self, audio_player: &mut AudioPlayer) {
        self.is_playing = false;
        self.playback_start_instant = None;
        self.playhead_sec = 0.0;
        audio_player.stop();
    }
}

/// 渲染 CapCut / Filmora 風格多軌可視化時間軸編輯器頁面
pub fn render_timeline_page(
    ui: &mut egui::Ui,
    state: &mut TimelineState,
    characters: &[CharacterPreset],
    api_key: &str,
    audio_player: &mut AudioPlayer,
) {
    // 檢查非同步訊息與播放頭更新
    state.poll_worker_messages();
    state.update_playback(audio_player);

    let ctx = ui.ctx().clone();
    if state.is_playing || state.is_busy() {
        ctx.request_repaint_after(Duration::from_millis(50));
    }

    // 錯誤提示條
    if let Some(err) = &state.error_message.clone() {
        egui::Frame::NONE
            .fill(Color32::from_rgba_premultiplied(239, 68, 68, 30))
            .stroke(Stroke::new(1.0, Color32::from_rgb(239, 68, 68)))
            .corner_radius(6)
            .inner_margin(8.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("⚠️ 時間軸提示:").color(Color32::from_rgb(239, 68, 68)).strong());
                    ui.label(RichText::new(err).color(Color32::from_rgb(239, 68, 68)));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("✕").clicked() {
                            state.error_message = None;
                        }
                    });
                });
            });
        ui.add_space(4.0);
    }

    // 成功 Toast 提示
    if let Some((toast, inst)) = &state.success_toast
        && inst.elapsed().as_secs() < 3 {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("✓ {}", toast)).color(Color32::from_rgb(34, 197, 94)).strong());
            });
            ui.add_space(4.0);
        }

    // 鍵盤快捷鍵 (在非文字編輯狀態下生效)
    if ui.is_enabled() && !state.is_busy() && !ui.ctx().wants_keyboard_input() {
        if ui.input(|i| i.key_pressed(egui::Key::Space)) {
            if state.is_playing {
                state.pause_playback(audio_player);
            } else {
                state.start_playback(audio_player);
            }
        }
        if ui.input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace)) {
            state.delete_selected_clip();
        }
        if ui.input(|i| (i.modifiers.command || i.modifiers.ctrl) && i.key_pressed(egui::Key::B))
            || ui.input(|i| !i.modifiers.any() && i.key_pressed(egui::Key::S))
        {
            state.split_selected_clip_at_playhead();
        }
    }

    // 1. 頂部多軌工具列 (Transport & Operations Toolbar)
    egui::Frame::group(ui.style())
        .corner_radius(8)
        .inner_margin(10.0)
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                // 時間碼顯示 (Timecode badge with frames at 30fps)
                let mins = (state.playhead_sec / 60.0).floor() as u32;
                let secs = (state.playhead_sec % 60.0).floor() as u32;
                let ms = ((state.playhead_sec % 1.0) * 100.0).floor() as u32;
                let frames = ((state.playhead_sec % 1.0) * 30.0).floor() as u32;
                let timecode_str = format!("{:02}:{:02}.{:02} (F:{:02})", mins, secs, ms, frames);

                ui.label(
                    RichText::new(timecode_str)
                        .monospace()
                        .size(17.0)
                        .color(Color32::from_rgb(99, 102, 241))
                        .strong(),
                );

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(8.0);

                // 播放控制按鈕群
                if ui.button("⏮ 回開頭").clicked() {
                    state.playhead_sec = 0.0;
                    if state.is_playing {
                        state.start_playback(audio_player);
                    }
                }

                let play_btn_text = if state.is_playing { "⏸ 暫停 (Space)" } else { "▶ 播放 (Space)" };
                let play_btn = egui::Button::new(RichText::new(play_btn_text).strong())
                    .fill(if state.is_playing {
                        Color32::from_rgb(234, 179, 8)
                    } else {
                        Color32::from_rgb(16, 185, 129)
                    });

                if ui.add(play_btn).clicked() {
                    if state.is_playing {
                        state.pause_playback(audio_player);
                    } else {
                        state.start_playback(audio_player);
                    }
                }

                if ui.button("⏹ 停止").clicked() {
                    state.stop_playback(audio_player);
                }

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(8.0);

                // 片段操作：剪切 (Split) 與刪除 (Delete)
                let split_btn = egui::Button::new("✂️ 剪切 (S / Ctrl+B)")
                    .corner_radius(4);
                if ui.add(split_btn).on_hover_text("在播放指針處將選取片段一分為二 (非破壞性修剪)").clicked() {
                    state.split_selected_clip_at_playhead();
                }

                let has_sel = state.selected_clip_id.is_some();
                if ui.add_enabled(has_sel, egui::Button::new("📋 複製")).clicked() {
                    state.duplicate_selected_clip();
                }

                if ui.add_enabled(has_sel, egui::Button::new("🗑️ 刪除 (Del)")).clicked() {
                    state.delete_selected_clip();
                }

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(8.0);

                // 時間軸縮放 (Zoom) 控制
                ui.label("🔍 縮放:");
                if ui.button("-").clicked() {
                    state.zoom_px_per_sec = (state.zoom_px_per_sec - 15.0).max(25.0);
                }
                ui.add(egui::Slider::new(&mut state.zoom_px_per_sec, 25.0..=220.0).show_value(false));
                if ui.button("+").clicked() {
                    state.zoom_px_per_sec = (state.zoom_px_per_sec + 15.0).min(220.0);
                }

            });
            ui.horizontal_wrapped(|ui| {
                    // 另存混音檔案
                    if ui.button("💾 另存混音...").clicked() {
                        let default_name = format!("timeline_mix_{}.wav", Local::now().format("%Y%m%d_%H%M%S"));
                        if let Some(path) = rfd::FileDialog::new()
                            .set_file_name(&default_name)
                            .add_filter("WAV 混音", &["wav"])
                            .save_file()
                        {
                            match state.export_timeline_mix(&path) {
                                Ok(()) => {
                                    let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                                    state.status_message = format!("混音已成功另存至: {}", name);
                                    state.success_toast = Some((format!("混音另存成功: {}", name), Instant::now()));
                                }
                                Err(e) => {
                                    state.error_message = Some(format!("混音匯出失敗: {}", e));
                                }
                            }
                        }
                    }

                    // 快速匯出至 outputs 目錄
                    let export_btn = egui::Button::new(RichText::new("🎛️ 匯出全軌混音").strong())
                        .fill(Color32::from_rgb(79, 70, 229));
                    if ui.add(export_btn).clicked() {
                        let _ = fs::create_dir_all("outputs");
                        let filename = format!("outputs/timeline_mix_{}.wav", Local::now().format("%Y%m%d_%H%M%S"));
                        let path = PathBuf::from(&filename);
                        match state.export_timeline_mix(&path) {
                            Ok(()) => {
                                state.status_message = format!("混音已成功匯出至: {}", filename);
                                state.success_toast = Some((format!("混音匯出成功: {}", filename), Instant::now()));
                            }
                            Err(e) => {
                                state.error_message = Some(format!("混音匯出失敗: {}", e));
                            }
                        }
                    }

                    // 匯入本機音訊 (BGM/SFX)
                    if ui.button("📂 匯入音訊檔").clicked()
                        && let Some(path) = rfd::FileDialog::new()
                            .add_filter("Audio Files", &["wav", "mp3"])
                            .pick_file()
                            && let Ok(bytes) = fs::read(&path) {
                                let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("音訊片段").to_string();
                                let target_track = state.tracks.first().map(|t| t.id).unwrap_or(0);
                                match TimelineClip::new(
                                    state.next_clip_id,
                                    target_track,
                                    name,
                                    "本地音訊".to_string(),
                                    String::new(),
                                    state.playhead_sec,
                                    bytes,
                                    Some(path.to_string_lossy().to_string()),
                                    [16, 185, 129],
                                ) {
                                    Ok(clip) => {
                                        state.next_clip_id += 1;
                                        state.clips.push(clip);
                                        state.status_message = "已匯入本地音訊檔至時間軸".to_string();
                                    }
                                    Err(e) => {
                                        state.error_message = Some(format!("匯入音訊失敗: {}", e));
                                    }
                                }
                            }

                    // 新增 TTS 語音按鈕
                    if ui.button("🎙️ 新增 TTS 片段").clicked() {
                        state.show_tts_modal = true;
                        state.tts_modal_start_sec = state.playhead_sec;
                    }

                    // 載入故事範本下拉選單
                    let templates = crate::storytelling::get_story_script_templates();
                    egui::ComboBox::from_id_salt("toolbar_story_template_combo")
                        .selected_text("🎭 載入名家故事劇本...")
                        .show_ui(ui, |ui| {
                            for tmpl in &templates {
                                if ui.selectable_label(false, format!("📖 {}", tmpl.title)).clicked() {
                                    state.load_story_template_project(tmpl);
                                }
                            }
                        });

                    // 載入預設示範工程
                    if ui.button("📦 示範工程").clicked() {
                        state.load_demo_project();
                    }
                });

        });

    ui.add_space(8.0);

    ui.label("拖曳片段中央調整出現時間／跨軌移動；拖曳兩端修剪。按住 Shift 以 0.1 秒對齊。");
    // 2. 主時間軸畫布 (Ruler + Multi-Track Canvas)
    let total_dur = state.total_timeline_duration();
    let zoom = state.zoom_px_per_sec;
    let timeline_width = (total_dur * zoom + 300.0).max(ui.available_width() - 220.0);
    let track_header_width = 200.0;

    egui::Frame::canvas(ui.style())
        .corner_radius(6)
        .inner_margin(0.0)
        .show(ui, |ui| {
            egui::ScrollArea::vertical().id_salt("timeline_tracks").max_height((ui.available_height() - 180.0).max(220.0)).auto_shrink([false, true]).show(ui, |ui| {
            ui.horizontal_top(|ui| {
                // 左側：軌道控制面板 (Track Headers)
                ui.allocate_ui_with_layout(Vec2::new(track_header_width, 32.0 + state.tracks.iter().map(|t| t.height + 4.0).sum::<f32>()), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.spacing_mut().slider_width = 70.0;
                    // 標頭頂部對齊標尺高度
                    ui.allocate_ui(Vec2::new(track_header_width, 28.0), |ui| {
                        ui.horizontal(|ui| {
                            ui.strong("軌道名稱與控制");
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.small_button("＋加軌").clicked() {
                                    state.add_track("新角色軌道".to_string(), TrackType::Dialogue);
                                }
                            });
                        });
                    });
                    ui.add_space(4.0);

                    for track in &mut state.tracks {
                        ui.allocate_ui(Vec2::new(track_header_width, track.height), |ui| {
                            ui.set_min_size(Vec2::new(track_header_width, track.height));
                            egui::Frame::NONE
                                .fill(ui.visuals().extreme_bg_color)
                                .inner_margin(6.0)
                                .corner_radius(4)
                                .show(ui, |ui| {
                                    ui.set_width(track_header_width - 12.0);
                                    ui.horizontal(|ui| {
                                        ui.label(RichText::new(track.track_type.icon()).size(14.0));
                                        ui.add(egui::Label::new(RichText::new(&track.name).size(12.0).strong()).truncate()).on_hover_text(&track.name);
                                    });

                                    ui.horizontal(|ui| {
                                        // 靜音 (Mute)
                                        let mute_btn = ui.selectable_label(track.is_muted, "M");
                                        if mute_btn.clicked() {
                                            track.is_muted = !track.is_muted;
                                        }

                                        // 獨奏 (Solo)
                                        let solo_btn = ui.selectable_label(track.is_solo, "S");
                                        if solo_btn.clicked() {
                                            track.is_solo = !track.is_solo;
                                        }

                                        // 軌道音量滑桿
                                        ui.label(RichText::new("音量:").size(10.0));
                                        ui.add(egui::Slider::new(&mut track.volume, 0.0..=2.0).show_value(false));
                                    });
                                });
                        });
                        ui.add_space(4.0);
                    }
                });

                // 右側：水平滾動時間標尺與多軌道畫布
                egui::ScrollArea::horizontal()
                    .id_salt("timeline_time")
                    .max_height(32.0 + state.tracks.iter().map(|t| t.height + 4.0).sum::<f32>())
                    .drag_to_scroll(false)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let total_tracks_height: f32 = state.tracks.iter().map(|t| t.height + 4.0).sum();
                        let canvas_height = 32.0 + total_tracks_height;

                        let (response, painter) = ui.allocate_painter(
                            Vec2::new(timeline_width, canvas_height.max(340.0)),
                            egui::Sense::click_and_drag(),
                        );

                        let rect = response.rect;
                        let ruler_rect = Rect::from_min_size(rect.min, Vec2::new(rect.width(), 28.0));

                        // 繪製時間軸標尺 (Ruler)
                        painter.rect_filled(ruler_rect, 0.0, Color32::from_rgb(17, 24, 39));
                        painter.line_segment(
                            [ruler_rect.left_bottom(), ruler_rect.right_bottom()],
                            Stroke::new(1.0, Color32::from_rgb(55, 65, 81)),
                        );

                        // 刻度與時間數字
                        let tick_interval_sec = if zoom > 120.0 { 1.0 } else if zoom > 60.0 { 2.0 } else { 5.0 };
                        let total_ticks = (timeline_width / zoom / tick_interval_sec).ceil() as usize;

                        for i in 0..=total_ticks {
                            let sec = i as f32 * tick_interval_sec;
                            let x = rect.min.x + sec * zoom;
                            if x > rect.max.x {
                                break;
                            }

                            // 大刻度
                            painter.line_segment(
                                [Pos2::new(x, ruler_rect.max.y - 10.0), Pos2::new(x, ruler_rect.max.y)],
                                Stroke::new(1.0, Color32::from_rgb(156, 163, 175)),
                            );

                            let mins = (sec / 60.0).floor() as u32;
                            let rem_secs = (sec % 60.0).floor() as u32;
                            let label = format!("{:02}:{:02}", mins, rem_secs);
                            painter.text(
                                Pos2::new(x + 4.0, ruler_rect.min.y + 4.0),
                                egui::Align2::LEFT_TOP,
                                label,
                                egui::FontId::monospace(10.0),
                                Color32::from_rgb(156, 163, 175),
                            );

                            // 小刻度
                            let sub_sec = sec + tick_interval_sec * 0.5;
                            let sub_x = rect.min.x + sub_sec * zoom;
                            if sub_x < rect.max.x {
                                painter.line_segment(
                                    [Pos2::new(sub_x, ruler_rect.max.y - 5.0), Pos2::new(sub_x, ruler_rect.max.y)],
                                    Stroke::new(1.0, Color32::from_rgb(75, 85, 99)),
                                );
                            }
                        }

                        // 繪製軌道泳道背景 (Track Lanes)
                        let mut curr_y = ruler_rect.max.y + 4.0;
                        let mut track_rects = Vec::new();

                        for (idx, track) in state.tracks.iter().enumerate() {
                            let lane_rect = Rect::from_min_size(
                                Pos2::new(rect.min.x, curr_y),
                                Vec2::new(rect.width(), track.height),
                            );
                            track_rects.push((track.id, lane_rect));

                            let bg_color = if idx % 2 == 0 {
                                Color32::from_rgb(24, 32, 47)
                            } else {
                                Color32::from_rgb(30, 41, 59)
                            };
                            painter.rect_filled(lane_rect, 4.0, bg_color);
                            painter.line_segment(
                                [lane_rect.left_bottom(), lane_rect.right_bottom()],
                                Stroke::new(0.8, Color32::from_rgb(51, 65, 85)),
                            );

                            curr_y += track.height + 4.0;
                        }

                        // 繪製音訊片段 (Visual Audio Clips with Waveforms & Handles)
                        let mut clip_rect_map = Vec::new();

                        for clip in &state.clips {
                            let lane_opt = track_rects.iter().find(|(tid, _)| *tid == clip.track_id);
                            if let Some((_, lane_rect)) = lane_opt {
                                let clip_x = rect.min.x + clip.start_sec * zoom;
                                let clip_w = (clip.duration_sec * zoom).max(18.0);
                                let clip_rect = Rect::from_min_size(
                                    Pos2::new(clip_x, lane_rect.min.y + 2.0),
                                    Vec2::new(clip_w, lane_rect.height() - 4.0),
                                );
                                clip_rect_map.push((clip.id, clip_rect));

                                let is_selected = state.selected_clip_id == Some(clip.id);

                                // 片段背景框
                                let base_color = Color32::from_rgb(clip.color[0], clip.color[1], clip.color[2]);
                                let fill_color = if is_selected {
                                    Color32::from_rgba_premultiplied(clip.color[0], clip.color[1], clip.color[2], 220)
                                } else {
                                    Color32::from_rgba_premultiplied(clip.color[0], clip.color[1], clip.color[2], 160)
                                };

                                let stroke = if is_selected {
                                    Stroke::new(2.5, Color32::from_rgb(250, 204, 21)) // Yellow highlight
                                } else {
                                    Stroke::new(1.0, base_color)
                                };

                                painter.rect_filled(clip_rect, 4.0, fill_color);
                                painter.rect_stroke(clip_rect, 4.0, stroke, egui::StrokeKind::Inside);

                                // 片段標題與時長文字
                                let title = format!("{} [{:.1}s]", clip.name, clip.duration_sec);
                                painter.text(
                                    Pos2::new(clip_rect.min.x + 8.0, clip_rect.min.y + 4.0),
                                    egui::Align2::LEFT_TOP,
                                    title,
                                    egui::FontId::proportional(11.0),
                                    Color32::WHITE,
                                );

                                // 片段台詞預覽 (若有)
                                if !clip.text.is_empty() {
                                    let preview = if clip.text.chars().count() > 16 {
                                        format!("{}...", clip.text.chars().take(16).collect::<String>())
                                    } else {
                                        clip.text.clone()
                                    };
                                    painter.text(
                                        Pos2::new(clip_rect.min.x + 8.0, clip_rect.min.y + 18.0),
                                        egui::Align2::LEFT_TOP,
                                        preview,
                                        egui::FontId::proportional(10.0),
                                        Color32::from_rgb(229, 231, 235),
                                    );
                                }

                                // 繪製自適應波形 (Waveform Peak Bars)
                                let wf_rect = Rect::from_min_max(
                                    Pos2::new(clip_rect.min.x + 6.0, clip_rect.min.y + 32.0),
                                    Pos2::new(clip_rect.max.x - 6.0, clip_rect.max.y - 4.0),
                                );
                                if wf_rect.width() > 6.0 && wf_rect.height() > 4.0 && !clip.waveform_peaks.is_empty() {
                                    let num_bars = ((wf_rect.width() / 4.0) as usize).clamp(4, clip.waveform_peaks.len());
                                    let bar_step = wf_rect.width() / num_bars as f32;
                                    let mid_y = wf_rect.center().y;
                                    let step_factor = clip.waveform_peaks.len() as f32 / num_bars as f32;

                                    for b_idx in 0..num_bars {
                                        let peak_idx = ((b_idx as f32 * step_factor) as usize).min(clip.waveform_peaks.len() - 1);
                                        let peak = clip.waveform_peaks[peak_idx];
                                        let bx = wf_rect.min.x + b_idx as f32 * bar_step + 1.5;
                                        let half_h = (peak * (wf_rect.height() * 0.45)).max(1.0);
                                        let bar_color = if is_selected {
                                            Color32::from_rgb(254, 240, 138)
                                        } else {
                                            Color32::from_rgba_premultiplied(255, 255, 255, 180)
                                        };
                                        painter.line_segment(
                                            [Pos2::new(bx, mid_y - half_h), Pos2::new(bx, mid_y + half_h)],
                                            Stroke::new(2.0, bar_color),
                                        );
                                    }
                                }

                                // 左右兩側邊緣修剪把手 (Trim Handles)
                                let handle_w = 6.0;
                                let left_handle_rect = Rect::from_min_size(
                                    Pos2::new(clip_rect.min.x, clip_rect.min.y),
                                    Vec2::new(handle_w, clip_rect.height()),
                                );
                                painter.rect_filled(
                                    left_handle_rect,
                                    egui::CornerRadius { nw: 4, sw: 4, ne: 0, se: 0 },
                                    Color32::from_rgba_premultiplied(255, 255, 255, if is_selected { 120 } else { 60 }),
                                );

                                let right_handle_rect = Rect::from_min_size(
                                    Pos2::new(clip_rect.max.x - handle_w, clip_rect.min.y),
                                    Vec2::new(handle_w, clip_rect.height()),
                                );
                                painter.rect_filled(
                                    right_handle_rect,
                                    egui::CornerRadius { ne: 4, se: 4, nw: 0, sw: 0 },
                                    Color32::from_rgba_premultiplied(255, 255, 255, if is_selected { 120 } else { 60 }),
                                );
                            }
                        }

                        // 滑鼠拖曳互動狀態機處理
                        let pointer_pos = response.interact_pointer_pos();

                        // 1. 滑鼠按下或點擊開始瞬間
                        if (response.drag_started() || (response.clicked() && state.drag_mode == DragMode::None))
                            && let Some(pos) = ui.input(|i| i.pointer.press_origin()).or(pointer_pos)
                        {
                            if ruler_rect.contains(pos) {
                                // 點擊或拖曳標尺：Seek 播放頭並進入 Scrub 模式
                                let target_sec = ((pos.x - rect.min.x) / zoom).max(0.0);
                                state.playhead_sec = target_sec;
                                if state.is_playing {
                                    let _ = audio_player.seek(Duration::from_secs_f32(target_sec));
                                    state.playback_start_sec = target_sec;
                                    state.playback_start_instant = Some(Instant::now());
                                }
                                state.drag_mode = DragMode::ScrubPlayhead;
                            } else {
                                // 軌道區域：倒序尋找命中的 Clip
                                let mut hit = None;
                                for &(cid, c_rect) in clip_rect_map.iter().rev() {
                                    if c_rect.contains(pos) {
                                        let left_h = Rect::from_min_max(c_rect.left_top(), Pos2::new(c_rect.left() + 8.0, c_rect.bottom()));
                                        let right_h = Rect::from_min_max(Pos2::new(c_rect.right() - 8.0, c_rect.top()), c_rect.right_bottom());

                                        if let Some(clip) = state.clips.iter().find(|c| c.id == cid) {
                                            if left_h.contains(pos) {
                                                hit = Some((cid, DragMode::TrimLeft {
                                                    clip_id: cid,
                                                    initial_trim_start: clip.trim_start_sec,
                                                    initial_start_sec: clip.start_sec,
                                                    pointer_start_x: pos.x,
                                                }));
                                            } else if right_h.contains(pos) {
                                                hit = Some((cid, DragMode::TrimRight {
                                                    clip_id: cid,
                                                    initial_trim_end: clip.trim_end_sec,
                                                    pointer_start_x: pos.x,
                                                }));
                                            } else {
                                                hit = Some((cid, DragMode::MoveClip {
                                                    clip_id: cid,
                                                    initial_start_sec: clip.start_sec,
                                                    initial_track_id: clip.track_id,
                                                    pointer_start_x: pos.x,
                                                    pointer_start_y: pos.y,
                                                }));
                                            }
                                        }
                                        break;
                                    }
                                }

                                if let Some((cid, mode)) = hit {
                                    state.selected_clip_id = Some(cid);
                                    state.drag_mode = mode;
                                } else {
                                    // 點擊空白軌道區域：Seek 播放頭並取消選取
                                    let target_sec = ((pos.x - rect.min.x) / zoom).max(0.0);
                                    state.playhead_sec = target_sec;
                                    state.selected_clip_id = None;
                                    if state.is_playing {
                                        let _ = audio_player.seek(Duration::from_secs_f32(target_sec));
                                        state.playback_start_sec = target_sec;
                                        state.playback_start_instant = Some(Instant::now());
                                    }
                                    state.drag_mode = DragMode::ScrubPlayhead;
                                }
                            }
                        }

                        // 2. 進行中的拖曳更新
                        if response.dragged() && let Some(pos) = pointer_pos {
                            match state.drag_mode {
                                    DragMode::ScrubPlayhead => {
                                        let target_sec = ((pos.x - rect.min.x) / zoom).max(0.0);
                                        state.playhead_sec = target_sec;
                                        if state.is_playing {
                                            let _ = audio_player.seek(Duration::from_secs_f32(target_sec));
                                            state.playback_start_sec = target_sec;
                                            state.playback_start_instant = Some(Instant::now());
                                        }
                                    }
                                    DragMode::MoveClip { clip_id, initial_start_sec, initial_track_id, pointer_start_x, .. } => {
                                        let delta_sec = (pos.x - pointer_start_x) / zoom;
                                        // 依據鼠標 Y 軸位置判斷目標軌道 (跨軌自由拖曳)
                                        let new_track_id = track_rects
                                            .iter()
                                            .find(|(_, r)| pos.y >= r.min.y && pos.y <= r.max.y)
                                            .map(|(tid, _)| *tid)
                                            .unwrap_or(initial_track_id);

                                        if let Some(c) = state.clips.iter_mut().find(|c| c.id == clip_id) {
                                            let start = (initial_start_sec + delta_sec).max(0.0);
                                            c.start_sec = if ui.input(|i| i.modifiers.shift) { (start * 10.0).round() / 10.0 } else { start };
                                            c.track_id = new_track_id;
                                        }
                                    }
                                    DragMode::TrimLeft { clip_id, initial_trim_start, initial_start_sec, pointer_start_x } => {
                                        let delta_sec = (pos.x - pointer_start_x) / zoom;
                                        if let Some(c) = state.clips.iter_mut().find(|c| c.id == clip_id) {
                                            let raw_delta = delta_sec * c.speed;
                                            let max_trim = (c.raw_duration_sec - c.trim_end_sec - 0.05).max(0.0);
                                            let new_trim = (initial_trim_start + raw_delta).clamp(0.0, max_trim);
                                            let trim_diff = new_trim - initial_trim_start;
                                            c.trim_start_sec = new_trim;
                                            c.start_sec = (initial_start_sec + trim_diff / c.speed).max(0.0);
                                            c.recalculate_duration();
                                        }
                                    }
                                    DragMode::TrimRight { clip_id, initial_trim_end, pointer_start_x } => {
                                        let delta_sec = (pos.x - pointer_start_x) / zoom;
                                        if let Some(c) = state.clips.iter_mut().find(|c| c.id == clip_id) {
                                            let raw_delta = -delta_sec * c.speed;
                                            let max_trim = (c.raw_duration_sec - c.trim_start_sec - 0.05).max(0.0);
                                            c.trim_end_sec = (initial_trim_end + raw_delta).clamp(0.0, max_trim);
                                            c.recalculate_duration();
                                        }
                                    }
                                    DragMode::None => {}
                                }
                            }

                        // 3. 拖曳結束
                        if response.drag_stopped() || (!response.dragged() && state.drag_mode != DragMode::None) {
                            state.drag_mode = DragMode::None;
                        }

                        // 4. 動態滑鼠游標反饋
                        if let Some(pos) = response.hover_pos() {
                            match state.drag_mode {
                                DragMode::TrimLeft { .. } | DragMode::TrimRight { .. } => {
                                    ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                                }
                                DragMode::MoveClip { .. } => {
                                    ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                                }
                                DragMode::ScrubPlayhead => {
                                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                                }
                                DragMode::None => {
                                    let mut hovered_cursor = None;
                                    for &(_cid, c_rect) in &clip_rect_map {
                                        if c_rect.contains(pos) {
                                            let left_h = Rect::from_min_max(c_rect.left_top(), Pos2::new(c_rect.left() + 8.0, c_rect.bottom()));
                                            let right_h = Rect::from_min_max(Pos2::new(c_rect.right() - 8.0, c_rect.top()), c_rect.right_bottom());
                                            if left_h.contains(pos) || right_h.contains(pos) {
                                                hovered_cursor = Some(egui::CursorIcon::ResizeHorizontal);
                                            } else {
                                                hovered_cursor = Some(egui::CursorIcon::Grab);
                                            }
                                            break;
                                        }
                                    }
                                    if let Some(cursor) = hovered_cursor {
                                        ui.ctx().set_cursor_icon(cursor);
                                    }
                                }
                            }
                        }

                        // 繪製紅色播放指針線 (Playhead Line & Needle)
                        let ph_x = rect.min.x + state.playhead_sec * zoom;
                        let ph_color = Color32::from_rgb(239, 68, 68); // Red

                        // 頂部倒三角形游標
                        let tri_top_y = ruler_rect.min.y;
                        let tri_bot_y = ruler_rect.max.y;
                        let tri_half_w = 6.0;
                        painter.add(egui::Shape::convex_polygon(
                            vec![
                                Pos2::new(ph_x - tri_half_w, tri_top_y),
                                Pos2::new(ph_x + tri_half_w, tri_top_y),
                                Pos2::new(ph_x, tri_bot_y),
                            ],
                            ph_color,
                            Stroke::NONE,
                        ));

                        // 直貫所有軌道的指針紅線
                        painter.line_segment(
                            [Pos2::new(ph_x, tri_bot_y), Pos2::new(ph_x, rect.max.y)],
                            Stroke::new(2.0, ph_color),
                        );
                    });
            });
            });
        });

    ui.add_space(8.0);

    // 3. 底部選取片段詳細屬性檢視器 (Clip Inspector)
    if let Some(sel_id) = state.selected_clip_id
        && let Some(clip_idx) = state.clips.iter().position(|c| c.id == sel_id)
    {
        let track_names: Vec<(usize, String)> = state.tracks.iter().map(|t| (t.id, t.name.clone())).collect();
            let any_synth = state.is_busy();
            let is_synth = state.synthesizing_clip_id == Some(sel_id);
            let mut trigger_synth_id = None;
            let mut new_status = None;

            let clip = &mut state.clips[clip_idx];

            egui::Frame::group(ui.style())
                .corner_radius(8)
                .inner_margin(12.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.strong(RichText::new(format!("🎞️ 片段屬性: {}", clip.name)).size(14.0));
                        ui.label(format!("(說話者: {})", clip.speaker));

                        // 語音載入狀態徽章
                        if clip.audio_bytes.is_some() {
                            ui.label(RichText::new("✓ 已載入真實語音").color(Color32::from_rgb(34, 197, 94)).size(11.0).strong());
                        } else {
                            ui.label(RichText::new("⚠️ 草稿片段 (可點擊右側立即合成語音)").color(Color32::from_rgb(234, 179, 8)).size(11.0));
                        }

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            // 立即為此片段生成 TTS 語音
                            let synth_btn_text = if is_synth { "🎙️ 正在生成中..." } else { "🎙️ 為此片段生成 TTS 語音" };
                            let can_synth = !any_synth && !api_key.trim().is_empty() && !clip.text.trim().is_empty();
                            let synth_btn = egui::Button::new(RichText::new(synth_btn_text).strong())
                                .fill(Color32::from_rgb(79, 70, 229));
                            if ui.add_enabled(can_synth, synth_btn).on_hover_text("使用片段的台詞與角色，直接呼叫 OpenRouter 合成 Fish Audio 語音").clicked() {
                                trigger_synth_id = Some(clip.id);
                            }
                            if is_synth {
                                ui.spinner();
                            }

                            // 試聽此片段單獨音訊
                            if ui.button("▶ 單獨試聽此片段").clicked() {
                                if let Some(bytes) = &clip.audio_bytes {
                                    let _ = audio_player.play_bytes(bytes.clone());
                                } else if let Some((pcm, sr, ch)) = clip.pcm_samples.as_ref().map(|s| (s, clip.sample_rate, clip.channels)) {
                                    let wav = encode_pcm_to_wav(pcm, sr, ch);
                                    let _ = audio_player.play_bytes(wav);
                                }
                            }

                            // 另存此片段
                            if ui.button("💾 另存此片段").clicked()
                                && let Some(path) = rfd::FileDialog::new()
                                    .set_file_name(format!("{}.wav", clip.name))
                                    .save_file()
                                    && let Some(samples) = &clip.pcm_samples {
                                        let wav = encode_pcm_to_wav(samples, clip.sample_rate, clip.channels);
                                        let _ = fs::write(path, wav);
                                        new_status = Some("片段已成功另存".to_string());
                                    }

                            // 重設修剪
                            if ui.button("↺ 重設修剪").clicked() {
                                clip.trim_start_sec = 0.0;
                                clip.trim_end_sec = 0.0;
                                clip.recalculate_duration();
                            }
                        });
                    });

                    ui.separator();

                    ui.horizontal(|ui| {
                        // 所在軌道切換
                        ui.label("移動至軌道:");
                        let cur_track_name = track_names
                            .iter()
                            .find(|(id, _)| *id == clip.track_id)
                            .map(|(_, name)| name.as_str())
                            .unwrap_or("未知軌道");
                        egui::ComboBox::from_id_salt(format!("track_select_{}", clip.id))
                            .selected_text(cur_track_name)
                            .show_ui(ui, |ui| {
                                for (tid, tname) in &track_names {
                                    ui.selectable_value(&mut clip.track_id, *tid, tname);
                                }
                            });

                        ui.add_space(16.0);

                        // 起始時間 (Start Time)
                        ui.label("起始時間 (秒):");
                        if ui.add(egui::DragValue::new(&mut clip.start_sec).speed(0.1).range(0.0..=600.0)).changed() {
                            clip.start_sec = clip.start_sec.max(0.0);
                        }

                        ui.add_space(16.0);

                        // 音量增益 (Gain)
                        ui.label("音量增益:");
                        let mut gain_pct = (clip.gain * 100.0) as u32;
                        if ui.add(egui::Slider::new(&mut gain_pct, 0..=200).suffix("%")).changed() {
                            clip.gain = gain_pct as f32 / 100.0;
                        }

                        ui.add_space(16.0);

                        // 播放速度 (Speed)
                        ui.label("播放速度:");
                        if ui.add(egui::Slider::new(&mut clip.speed, 0.5..=2.0).suffix("x")).changed() {
                            clip.recalculate_duration();
                        }
                    });

                    ui.add_space(4.0);

                    // 修剪控制 (Trim in / Trim out)
                    ui.horizontal(|ui| {
                        ui.label("開頭剪裁 (Trim In):");
                        let max_trim = (clip.raw_duration_sec - clip.trim_end_sec - 0.05).max(0.0);
                        if ui.add(egui::DragValue::new(&mut clip.trim_start_sec).speed(0.05).range(0.0..=max_trim).suffix("s")).changed() {
                            clip.recalculate_duration();
                        }

                        ui.add_space(16.0);

                        ui.label("結尾剪裁 (Trim Out):");
                        let max_trim_out = (clip.raw_duration_sec - clip.trim_start_sec - 0.05).max(0.0);
                        if ui.add(egui::DragValue::new(&mut clip.trim_end_sec).speed(0.05).range(0.0..=max_trim_out).suffix("s")).changed() {
                            clip.recalculate_duration();
                        }

                        ui.add_space(16.0);
                        ui.label(format!("原始時長: {:.2}s  /  有效時長: {:.2}s", clip.raw_duration_sec, clip.duration_sec));
                    });

                    ui.add_space(4.0);

                    // 台詞編輯
                    ui.horizontal(|ui| {
                        ui.label("台詞文本:");
                        ui.add(egui::TextEdit::singleline(&mut clip.text).desired_width(f32::INFINITY));
                    });
                });

            if let Some(msg) = new_status {
                state.status_message = msg;
            }
            if let Some(id) = trigger_synth_id {
                state.synthesize_clip_speech(id, api_key, characters);
            }
        }

    // 4. 新增 TTS 語音片段彈出視窗 (Add TTS Clip Modal)
    if state.show_tts_modal {
        egui::Window::new("🎙️ 新增 Fish Audio TTS 語音片段至時間軸")
            .collapsible(false)
            .resizable(false)
            .min_width(450.0)
            .show(ui.ctx(), |ui| {
                ui.label("選擇目標軌道與角色，輸入文字後直接生成並置入時間軸：");
                ui.add_space(8.0);

                ui.horizontal(|ui| {
                    ui.label("置入軌道:");
                    egui::ComboBox::from_id_salt("modal_track_select")
                        .selected_text(
                            state
                                .tracks
                                .iter()
                                .find(|t| t.id == state.tts_modal_track_id)
                                .map(|t| t.name.as_str())
                                .unwrap_or("選擇軌道"),
                        )
                        .show_ui(ui, |ui| {
                            for t in &state.tracks {
                                ui.selectable_value(&mut state.tts_modal_track_id, t.id, &t.name);
                            }
                        });

                    ui.add_space(12.0);

                    ui.label("角色聲線:");
                    egui::ComboBox::from_id_salt("modal_char_select")
                        .selected_text(
                            characters
                                .get(state.tts_modal_character_idx)
                                .map(|c| c.name.as_str())
                                .unwrap_or("自訂角色"),
                        )
                        .show_ui(ui, |ui| {
                            for (idx, c) in characters.iter().enumerate() {
                                if ui.selectable_value(&mut state.tts_modal_character_idx, idx, &c.name).clicked() {
                                    state.tts_modal_speaker_name = c.name.clone();
                                }
                            }
                        });
                });

                ui.add_space(6.0);

                ui.horizontal(|ui| {
                    ui.label("放置起始秒數:");
                    ui.add(egui::DragValue::new(&mut state.tts_modal_start_sec).speed(0.1).range(0.0..=600.0).suffix("s"));
                });

                ui.add_space(8.0);
                ui.label("台詞文本 (支援 [happy], [whispering], [sigh] 等標籤):");
                ui.add(
                    egui::TextEdit::multiline(&mut state.tts_modal_text)
                        .desired_rows(4)
                        .desired_width(f32::INFINITY),
                );

                ui.add_space(12.0);

                ui.horizontal(|ui| {
                    let can_gen = ui.is_enabled() && !state.is_busy() && !api_key.trim().is_empty() && !state.tts_modal_text.trim().is_empty();
                    let gen_btn = egui::Button::new(if state.tts_modal_is_generating {
                        "🎙️ 正在向 OpenRouter 生成中..."
                    } else {
                        "✨ 立即合成並放置於時間軸"
                    })
                    .fill(Color32::from_rgb(79, 70, 229));

                    if ui.add_enabled(can_gen, gen_btn).clicked() {
                        let key = api_key.trim().to_string();
                        if key.is_empty() {
                            state.error_message = Some("請先在單人語音頁面設定 OpenRouter API Key".to_string());
                        } else {
                            state.tts_modal_is_generating = true;
                            let tx = state.sender.clone();
                            let track_id = state.tts_modal_track_id;
                            let char_preset = characters
                                .get(state.tts_modal_character_idx)
                                .cloned()
                                .unwrap_or_else(|| characters[0].clone());
                            let speaker = char_preset.name.clone();
                            let raw_text = state.tts_modal_text.trim().to_string();
                            let start_sec = state.tts_modal_start_sec;

                            let speech_input = if !raw_text.contains(&char_preset.prompt_tag) {
                                format!("{} {}", char_preset.prompt_tag, raw_text)
                            } else {
                                raw_text.clone()
                            };

                            let req = SpeechRequest {
                                model: DEFAULT_MODEL.to_string(),
                                input: speech_input,
                                voice: char_preset.voice_id.clone(),
                                response_format: Some("mp3".to_string()),
                                speed: Some(char_preset.recommended_speed),
                            };

                            thread::spawn(move || {
                                let client = OpenRouterClient::new();
                                let res = client.synthesize(&key, &req);
                                let name = format!("TTS_{}", speaker);
                                let _ = tx.send(TimelineWorkerMessage::TtsClipGenerated {
                                    track_id,
                                    name,
                                    speaker,
                                    text: raw_text,
                                    start_sec,
                                    result: res,
                                });
                            });
                        }
                    }

                    if state.tts_modal_is_generating {
                        ui.spinner();
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("取消").clicked() {
                            state.show_tts_modal = false;
                        }
                    });
                });
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_synthesis_prevents_duplicate_work_and_target_deletion() {
        let mut state = TimelineState::new();
        state.load_demo_project();
        let id = state.clips[0].id;
        state.selected_clip_id = Some(id);
        state.synthesizing_clip_id = Some(id);
        let count = state.clips.len();
        state.delete_selected_clip();
        state.load_demo_project();
        assert_eq!(state.clips.len(), count);
        assert!(state.clips.iter().any(|c| c.id == id));
        state.sender.send(TimelineWorkerMessage::ClipAudioSynthesized {
            clip_id: id, result: Ok(encode_pcm_to_wav(&[100; 800], 8000, 1)),
        }).unwrap();
        state.poll_worker_messages();
        assert!(!state.is_busy());
        assert!(state.clips[0].audio_bytes.is_some());
    }

    #[test]
    fn test_timeline_state_initialization() {
        let state = TimelineState::new();
        assert_eq!(state.tracks.len(), 4);
        assert_eq!(state.playhead_sec, 0.0);
        assert!(!state.is_playing);
        assert_eq!(state.zoom_px_per_sec, 80.0);
    }

    #[test]
    fn test_timeline_mock_clip_creation_and_duration() {
        let mut clip = TimelineClip::new_mock(
            1,
            0,
            "測試片段".to_string(),
            "旁白".to_string(),
            "你好".to_string(),
            1.5,
            3.0,
            [59, 130, 246],
        );

        assert_eq!(clip.start_sec, 1.5);
        assert_eq!(clip.duration_sec, 3.0);
        assert_eq!(clip.raw_duration_sec, 3.0);
        assert_eq!(clip.waveform_peaks.len(), 120);

        // 測試修改速度
        clip.speed = 1.5;
        clip.recalculate_duration();
        assert!((clip.duration_sec - 2.0).abs() < 0.05);

        // 測試修剪
        clip.speed = 1.0;
        clip.trim_start_sec = 0.5;
        clip.trim_end_sec = 0.5;
        clip.recalculate_duration();
        assert!((clip.duration_sec - 2.0).abs() < 0.05);
    }

    #[test]
    fn test_timeline_clip_split() {
        let clip = TimelineClip::new_mock(
            10,
            0,
            "長篇片段".to_string(),
            "主角".to_string(),
            "一段很長的話".to_string(),
            2.0,
            4.0,
            [59, 130, 246],
        );

        // 在 3.5s 處剪切 (在片段 2.0 ~ 6.0 範圍內)
        let (first, second) = clip.split_at(3.5, 11).expect("剪切應成功");

        assert_eq!(first.id, 10);
        assert_eq!(first.start_sec, 2.0);
        assert!((first.duration_sec - 1.5).abs() < 0.05);

        assert_eq!(second.id, 11);
        assert_eq!(second.start_sec, 3.5);
        assert!((second.duration_sec - 2.5).abs() < 0.05);
    }

    #[test]
    fn test_timeline_clip_split_out_of_bounds() {
        let clip = TimelineClip::new_mock(
            10,
            0,
            "片段".to_string(),
            "主角".to_string(),
            "話".to_string(),
            2.0,
            4.0,
            [59, 130, 246],
        );

        // 剪切點在片段前
        let err1 = clip.split_at(1.0, 11);
        assert!(err1.is_err());

        // 剪切點在片段後
        let err2 = clip.split_at(7.0, 11);
        assert!(err2.is_err());
    }

    #[test]
    fn test_timeline_mixdown_and_export() {
        let mut state = TimelineState::new();
        state.load_demo_project();
        assert_eq!(state.clips.len(), 4);

        assert!(state.mix_timeline_to_pcm().is_err(), "Drafts must not export synthesized placeholder tones");
        for clip in &mut state.clips {
            let samples = vec![1200i16; (clip.raw_duration_sec * 44100.0) as usize];
            clip.audio_bytes = Some(encode_pcm_to_wav(&samples, 44100, 1));
            clip.pcm_samples = Some(samples);
        }
        let (pcm, sr, ch) = state.mix_timeline_to_pcm().expect("混音應成功");
        assert_eq!(sr, 44100);
        assert_eq!(ch, 2);
        assert!(!pcm.is_empty());

        // 測試匯出成 WAV 檔案
        let temp_dir = std::env::temp_dir();
        let test_wav_path = temp_dir.join("test_timeline_export.wav");
        let res = state.export_timeline_mix(&test_wav_path);
        assert!(res.is_ok());
        assert!(test_wav_path.exists());

        // 讀回並驗證 WAV 檔頭與可解碼性
        let bytes = fs::read(&test_wav_path).unwrap();
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");

        let _ = fs::remove_file(test_wav_path);
    }
}
