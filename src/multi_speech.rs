use crate::api::{OpenRouterClient, SpeechRequest, DEFAULT_MODEL};
use crate::audio::{estimate_audio_duration, export_audio_bytes};
use crate::models::{get_default_characters, CharacterPreset};
use chrono::Local;
use eframe::egui;
use egui::{Color32, RichText, Stroke, Vec2};
use rodio::Source;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::thread;
use std::time::Duration;

/// 登場角色定義
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CastMember {
    pub speaker_id: usize,
    pub name: String,
    pub character_preset_idx: usize,
    pub prompt_tag: String,
    pub custom_voice_id: Option<String>,
    pub default_tone: String,
    pub speed: f32,
    pub badge_color: [u8; 3],
}

/// 分行劇本對白
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DialogLine {
    pub id: u64,
    pub speaker_id: usize,
    pub tone: String,
    pub text: String,
    pub pause_after_ms: u32,
}

/// 多角色生成模式
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum MultiGenMode {
    /// 原生 Fish Audio S2.1 語法合成 (<|speaker:X|>)
    NativeSpeakerTags,
    /// 循序分行生成並拼接為單一音訊檔案
    SequentialConcat,
}

impl MultiGenMode {
    pub fn label(&self) -> &'static str {
        match self {
            MultiGenMode::NativeSpeakerTags => "原生 Fish Audio <|speaker:X|> 語法合成 (推薦，單次呼叫自然流暢)",
            MultiGenMode::SequentialConcat => "分行獨立生成與拼接 (逐句自訂語速與精準停頓時間)",
        }
    }
}

/// 劇本範本
#[derive(Clone, Debug)]
pub struct ScriptTemplate {
    pub title: &'static str,
    pub description: &'static str,
    pub cast: Vec<CastMember>,
    pub lines: Vec<DialogLine>,
}

/// 多角色狀態
pub struct MultiSpeechState {
    pub cast: Vec<CastMember>,
    pub lines: Vec<DialogLine>,
    pub gen_mode: MultiGenMode,
    pub selected_model: String,
    pub selected_format: String,
    pub default_pause_ms: u32,
    pub next_line_id: u64,
    pub is_generating: bool,
    pub current_step: usize,
    pub total_steps: usize,
    pub status_message: String,
    pub error_message: Option<String>,
    pub success_toast: Option<String>,
    pub last_generated_bytes: Option<Vec<u8>>,
    pub last_generated_path: Option<String>,
    pub preview_line_id: Option<u64>,
}

impl Default for MultiSpeechState {
    fn default() -> Self {
        Self::new()
    }
}

impl MultiSpeechState {
    pub fn new() -> Self {
        let presets = get_default_characters();

        // 預設兩位角色
        let cast = vec![
            CastMember {
                speaker_id: 0,
                name: "溫柔知性御姐".to_string(),
                character_preset_idx: 1.min(presets.len().saturating_sub(1)),
                prompt_tag: "[溫柔知性御姐音色]".to_string(),
                custom_voice_id: None,
                default_tone: "[calm]".to_string(),
                speed: 0.95,
                badge_color: [59, 130, 246], // Blue
            },
            CastMember {
                speaker_id: 1,
                name: "沉穩磁性青年".to_string(),
                character_preset_idx: 3.min(presets.len().saturating_sub(1)),
                prompt_tag: "[沉穩磁性男性音色]".to_string(),
                custom_voice_id: None,
                default_tone: "[happy]".to_string(),
                speed: 1.0,
                badge_color: [249, 115, 22], // Orange
            },
        ];

        let lines = vec![
            DialogLine {
                id: 1,
                speaker_id: 0,
                tone: "[calm]".to_string(),
                text: "你終於回來了，今天在外面辛苦了吧？晚餐已經為你熱好了。".to_string(),
                pause_after_ms: 300,
            },
            DialogLine {
                id: 2,
                speaker_id: 1,
                tone: "[happy] [chuckle]".to_string(),
                text: "只要回到家看見你，今天所有的疲憊就全都煙消雲散了。聞起來好香啊！".to_string(),
                pause_after_ms: 300,
            },
            DialogLine {
                id: 3,
                speaker_id: 0,
                tone: "[溫柔細語]".to_string(),
                text: "快坐下來趁熱吃吧，有你最喜歡的料理喔。".to_string(),
                pause_after_ms: 300,
            },
        ];

        Self {
            cast,
            lines,
            gen_mode: MultiGenMode::NativeSpeakerTags,
            selected_model: DEFAULT_MODEL.to_string(),
            selected_format: "mp3".to_string(),
            default_pause_ms: 300,
            next_line_id: 4,
            is_generating: false,
            current_step: 0,
            total_steps: 0,
            status_message: "就緒。設定角色與對白台詞後點擊「立即合成多角色語音」。".to_string(),
            error_message: None,
            success_toast: None,
            last_generated_bytes: None,
            last_generated_path: None,
            preview_line_id: None,
        }
    }

    /// 新增角色
    pub fn add_speaker(&mut self, presets: &[CharacterPreset]) {
        let speaker_id = self
            .cast
            .iter()
            .map(|c| c.speaker_id)
            .max()
            .map(|m| m + 1)
            .unwrap_or(0);

        let colors: &[[u8; 3]] = &[
            [59, 130, 246],  // Blue
            [249, 115, 22],  // Orange
            [168, 85, 247],  // Purple
            [16, 185, 129],  // Green
            [236, 72, 153],  // Pink
            [234, 179, 8],   // Yellow
            [20, 184, 166],  // Teal
        ];
        let badge_color = colors[speaker_id % colors.len()];

        let preset_idx = speaker_id % presets.len();
        let default_preset = &presets[preset_idx];
        let def_tone = default_preset
            .default_tone
            .clone()
            .unwrap_or_else(|| "[calm]".to_string());

        self.cast.push(CastMember {
            speaker_id,
            name: format!("角色 {}", speaker_id),
            character_preset_idx: preset_idx,
            prompt_tag: default_preset.prompt_tag.clone(),
            custom_voice_id: None,
            default_tone: def_tone,
            speed: default_preset.recommended_speed,
            badge_color,
        });
    }

    /// 移除角色 (若至少有 2 位)
    pub fn remove_speaker(&mut self, speaker_id: usize) {
        if self.cast.len() > 2 {
            self.cast.retain(|c| c.speaker_id != speaker_id);
            // 若該角色有台詞，重設為第一位現存角色
            let fallback_id = self.cast.first().map(|c| c.speaker_id).unwrap_or(0);
            for line in &mut self.lines {
                if line.speaker_id == speaker_id {
                    line.speaker_id = fallback_id;
                }
            }
        }
    }

    /// 新增一行台詞 (預設套用該說話者的 default_tone)
    pub fn add_line(&mut self, default_speaker_id: usize) {
        let line_id = self.next_line_id;
        self.next_line_id += 1;
        let def_tone = self
            .cast
            .iter()
            .find(|c| c.speaker_id == default_speaker_id)
            .map(|c| c.default_tone.clone())
            .unwrap_or_default();
        self.lines.push(DialogLine {
            id: line_id,
            speaker_id: default_speaker_id,
            tone: def_tone,
            text: String::new(),
            pause_after_ms: self.default_pause_ms,
        });
    }

    /// 刪除一行台詞
    pub fn delete_line(&mut self, line_id: u64) {
        self.lines.retain(|l| l.id != line_id);
    }

    /// 上移台詞
    pub fn move_line_up(&mut self, idx: usize) {
        if idx > 0 && idx < self.lines.len() {
            self.lines.swap(idx, idx - 1);
        }
    }

    /// 下移台詞
    pub fn move_line_down(&mut self, idx: usize) {
        if idx + 1 < self.lines.len() {
            self.lines.swap(idx, idx + 1);
        }
    }

    /// 複製台詞
    pub fn duplicate_line(&mut self, idx: usize) {
        if let Some(line) = self.lines.get(idx).cloned() {
            let new_id = self.next_line_id;
            self.next_line_id += 1;
            let new_line = DialogLine {
                id: new_id,
                ..line
            };
            self.lines.insert(idx + 1, new_line);
        }
    }

    /// 載入劇本範本
    pub fn load_template(&mut self, template: ScriptTemplate) {
        self.cast = template.cast;
        self.lines = template.lines;
        self.next_line_id = self.lines.iter().map(|l| l.id).max().unwrap_or(0) + 1;
        self.status_message = format!("已載入劇本範本: {}", template.title);
    }

    /// 匯出純文字劇本
    pub fn export_text_script(&self) -> String {
        let mut out = String::new();
        out.push_str("【登場角色配置】\n");
        for c in &self.cast {
            out.push_str(&format!("- [Speaker {}] {} (聲線: {}, 語速: {:.2}x)\n", c.speaker_id, c.name, c.prompt_tag, c.speed));
        }
        out.push_str("\n【劇本對白內容】\n");
        for (i, line) in self.lines.iter().enumerate() {
            let spk_name = self.cast.iter().find(|c| c.speaker_id == line.speaker_id)
                .map(|c| c.name.as_str())
                .unwrap_or("未知角色");
            let tone_str = if line.tone.trim().is_empty() { String::new() } else { format!(" {}", line.tone.trim()) };
            out.push_str(&format!("{}. 【{}】{}{}: {}\n", i + 1, spk_name, line.speaker_id, tone_str, line.text.trim()));
        }
        out
    }
}

/// 取得內建多角色劇本範本清單
pub fn get_script_templates() -> Vec<ScriptTemplate> {
    vec![
        ScriptTemplate {
            title: "溫馨日常（男女對話）",
            description: "知性成熟女性與沉穩青年的日常歸家對話",
            cast: vec![
                CastMember {
                    speaker_id: 0,
                    name: "溫柔知性御姐".to_string(),
                    character_preset_idx: 1,
                    prompt_tag: "[溫柔知性御姐音色]".to_string(),
                    custom_voice_id: None,
                    default_tone: "[calm]".to_string(),
                    speed: 0.95,
                    badge_color: [59, 130, 246],
                },
                CastMember {
                    speaker_id: 1,
                    name: "沉穩磁性青年".to_string(),
                    character_preset_idx: 3,
                    prompt_tag: "[沉穩磁性男性音色]".to_string(),
                    custom_voice_id: None,
                    default_tone: "[happy]".to_string(),
                    speed: 1.0,
                    badge_color: [249, 115, 22],
                },
            ],
            lines: vec![
                DialogLine {
                    id: 1,
                    speaker_id: 0,
                    tone: "[calm]".to_string(),
                    text: "你終於回來了，今天工作辛苦了吧？晚餐已經熱好了。".to_string(),
                    pause_after_ms: 300,
                },
                DialogLine {
                    id: 2,
                    speaker_id: 1,
                    tone: "[happy] [chuckle]".to_string(),
                    text: "只要回到家看見你，今天所有的疲憊都煙消雲散了。今天做什麼好吃的？".to_string(),
                    pause_after_ms: 300,
                },
                DialogLine {
                    id: 3,
                    speaker_id: 0,
                    tone: "[溫柔細語]".to_string(),
                    text: "是你最喜歡的牛肉燉馬鈴薯喔，快洗洗手準備開飯吧！".to_string(),
                    pause_after_ms: 300,
                },
                DialogLine {
                    id: 4,
                    speaker_id: 1,
                    tone: "[excited]".to_string(),
                    text: "太棒了！有你在家等我，真的是世界上最幸福的事。".to_string(),
                    pause_after_ms: 300,
                },
            ],
        },
        ScriptTemplate {
            title: "冒險史詩（旁白＋勇者＋刺客）",
            description: "大氣磅礴的旁白鋪陳，搭配熱血勇者與冷酷刺客宿命對峙",
            cast: vec![
                CastMember {
                    speaker_id: 0,
                    name: "大氣紀錄片旁白".to_string(),
                    character_preset_idx: 5,
                    prompt_tag: "[大氣專業紀錄片旁白播音員]".to_string(),
                    custom_voice_id: None,
                    default_tone: "[廣播播音腔]".to_string(),
                    speed: 0.95,
                    badge_color: [168, 85, 247],
                },
                CastMember {
                    speaker_id: 1,
                    name: "熱血陽光少年".to_string(),
                    character_preset_idx: 4,
                    prompt_tag: "[熱血清亮少年音色]".to_string(),
                    custom_voice_id: None,
                    default_tone: "[excited]".to_string(),
                    speed: 1.05,
                    badge_color: [239, 68, 68],
                },
                CastMember {
                    speaker_id: 2,
                    name: "冷酷神秘刺客".to_string(),
                    character_preset_idx: 9,
                    prompt_tag: "[冷酷低沉神秘刺客]".to_string(),
                    custom_voice_id: None,
                    default_tone: "[whispering]".to_string(),
                    speed: 0.95,
                    badge_color: [75, 85, 99],
                },
            ],
            lines: vec![
                DialogLine {
                    id: 1,
                    speaker_id: 0,
                    tone: "[calm] [廣播播音腔]".to_string(),
                    text: "殘陽如血，在古老神殿的廢墟之上，宿命的二人終於再度相會。".to_string(),
                    pause_after_ms: 400,
                },
                DialogLine {
                    id: 2,
                    speaker_id: 1,
                    tone: "[angry] [充滿決心]".to_string(),
                    text: "住手吧！你背叛同伴所奪取的黑暗力量，絕不可能帶來你想要的救贖！".to_string(),
                    pause_after_ms: 350,
                },
                DialogLine {
                    id: 3,
                    speaker_id: 2,
                    tone: "[sarcastic] [whispering]".to_string(),
                    text: "同伴？天真。弱者才會抱團取暖，這片大地的規則，從來只由勝者書寫。".to_string(),
                    pause_after_ms: 350,
                },
                DialogLine {
                    id: 4,
                    speaker_id: 1,
                    tone: "[excited] [angry]".to_string(),
                    text: "那就拔劍吧！我會用這把劍證明，我們大家一路走來的信念絕不會輸！".to_string(),
                    pause_after_ms: 400,
                },
                DialogLine {
                    id: 5,
                    speaker_id: 0,
                    tone: "[calm]".to_string(),
                    text: "風止，劍動。破曉之際的最終決戰，即刻爆發。".to_string(),
                    pause_after_ms: 300,
                },
            ],
        },
        ScriptTemplate {
            title: "傲嬌大小姐與歡樂侍從",
            description: "二次元動漫經典傲嬌少女與元氣侍從的趣味鬥嘴",
            cast: vec![
                CastMember {
                    speaker_id: 0,
                    name: "傲嬌大小姐".to_string(),
                    character_preset_idx: 7,
                    prompt_tag: "[傲嬌大小姐少女音色]".to_string(),
                    custom_voice_id: None,
                    default_tone: "[sarcastic]".to_string(),
                    speed: 1.0,
                    badge_color: [236, 72, 153],
                },
                CastMember {
                    speaker_id: 1,
                    name: "活力元氣少女".to_string(),
                    character_preset_idx: 2,
                    prompt_tag: "[活力元氣少女音色]".to_string(),
                    custom_voice_id: None,
                    default_tone: "[happy]".to_string(),
                    speed: 1.05,
                    badge_color: [16, 185, 129],
                },
            ],
            lines: vec![
                DialogLine {
                    id: 1,
                    speaker_id: 0,
                    tone: "[sarcastic] [proud]".to_string(),
                    text: "哼！你今天遲到了整整三分鍾！到底有沒有把本小姐放在眼裡啊？".to_string(),
                    pause_after_ms: 300,
                },
                DialogLine {
                    id: 2,
                    speaker_id: 1,
                    tone: "[gasp] [excited]".to_string(),
                    text: "大小姐對不起！因為路上的限量草莓大福正在排隊，我拼了命才買到最後一份！".to_string(),
                    pause_after_ms: 300,
                },
                DialogLine {
                    id: 3,
                    speaker_id: 0,
                    tone: "[shy]".to_string(),
                    text: "草、草莓大福？！……既然你誠心誠意奉上供品，那本小姐就勉強原諒你這一次好了！".to_string(),
                    pause_after_ms: 300,
                },
                DialogLine {
                    id: 4,
                    speaker_id: 1,
                    tone: "[laughing] [happy]".to_string(),
                    text: "我就知道大小姐最通情達理了！紅茶已經泡好了，我們快趁熱吃吧！".to_string(),
                    pause_after_ms: 300,
                },
            ],
        },
    ]
}

/// 組合 Fish Audio 原生 <|speaker:X|> 多角色合成提示詞 (若行口氣為空則自動帶入說話者預設口氣)
pub fn build_native_multi_speaker_prompt(cast: &[CastMember], lines: &[DialogLine]) -> String {
    let mut parts = Vec::new();

    for line in lines {
        let text = line.text.trim();
        if text.is_empty() {
            continue;
        }

        let spk = cast.iter().find(|c| c.speaker_id == line.speaker_id);
        let tag = spk.map(|c| c.prompt_tag.trim()).unwrap_or("");
        let effective_tone = if !line.tone.trim().is_empty() {
            line.tone.trim()
        } else {
            spk.map(|c| c.default_tone.trim()).unwrap_or("")
        };

        // 格式: <|speaker:X|> [角色音色標籤] [口氣] 台詞文字
        let mut line_content = format!("<|speaker:{}|>", line.speaker_id);

        if !tag.is_empty() && !text.contains(tag) {
            line_content.push(' ');
            line_content.push_str(tag);
        }

        if !effective_tone.is_empty() && !text.contains(effective_tone) {
            line_content.push(' ');
            line_content.push_str(effective_tone);
        }

        line_content.push(' ');
        line_content.push_str(text);

        parts.push(line_content);
    }

    parts.join(" ")
}

/// 拼接多個 WAV 音訊緩衝區，並依序在句間插入指定靜音毫秒
pub fn concatenate_wav_buffers_with_pauses(
    buffers: &[&[u8]],
    pauses_ms: &[u32],
) -> Result<Vec<u8>, String> {
    if buffers.is_empty() {
        return Err("音訊緩衝區清單為空".to_string());
    }

    // 讀取第一個 WAV 檔頭參數
    let first = buffers[0];
    if first.len() < 44 || &first[0..4] != b"RIFF" || &first[8..12] != b"WAVE" {
        return Err("第一個檔案非標準 RIFF/WAVE 格式".to_string());
    }

    let (channels, sample_rate, byte_rate, block_align, bits_per_sample) = parse_wav_fmt(first)?;

    let mut combined_pcm = Vec::new();

    for (i, buf) in buffers.iter().enumerate() {
        let pcm_data = extract_wav_pcm_data(buf)?;
        combined_pcm.extend_from_slice(pcm_data);

        // 若非最後一段，插入指定停頓靜音
        if i + 1 < buffers.len() {
            let pause_ms = pauses_ms.get(i).copied().unwrap_or(300);
            let raw_silence = (byte_rate as u64 * pause_ms as u64) / 1000;
            let align = block_align as u64;
            let silence_len = if align > 0 {
                ((raw_silence / align) * align) as usize
            } else {
                raw_silence as usize
            };
            if silence_len > 0 {
                combined_pcm.extend_from_slice(&vec![0u8; silence_len]);
            }
        }
    }

    // 重新封裝標準 WAV 檔頭 (44 bytes)
    let total_pcm_len = combined_pcm.len() as u32;
    let riff_chunk_size = total_pcm_len + 36;

    let mut out = Vec::with_capacity(44 + combined_pcm.len());
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&riff_chunk_size.to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // Subchunk1Size = 16 for PCM
    out.extend_from_slice(&1u16.to_le_bytes());  // AudioFormat = 1 (PCM)
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&block_align.to_le_bytes());
    out.extend_from_slice(&bits_per_sample.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&total_pcm_len.to_le_bytes());
    out.extend_from_slice(&combined_pcm);

    Ok(out)
}

/// 拼接多個 WAV 音訊緩衝區，並在中間插入固定靜音毫秒
pub fn concatenate_wav_buffers(buffers: &[&[u8]], pause_silence_ms: u32) -> Result<Vec<u8>, String> {
    let pauses = vec![pause_silence_ms; buffers.len().saturating_sub(1)];
    concatenate_wav_buffers_with_pauses(buffers, &pauses)
}

fn parse_wav_fmt(bytes: &[u8]) -> Result<(u16, u32, u32, u16, u16), String> {
    let mut offset = 12;
    while offset + 8 <= bytes.len() {
        let chunk_id = &bytes[offset..offset + 4];
        let chunk_size = u32::from_le_bytes([
            bytes[offset + 4],
            bytes[offset + 5],
            bytes[offset + 6],
            bytes[offset + 7],
        ]) as usize;

        if chunk_id == b"fmt " && offset + 8 + 16 <= bytes.len() {
            let channels = u16::from_le_bytes([bytes[offset + 10], bytes[offset + 11]]);
            let sample_rate = u32::from_le_bytes([
                bytes[offset + 12],
                bytes[offset + 13],
                bytes[offset + 14],
                bytes[offset + 15],
            ]);
            let byte_rate = u32::from_le_bytes([
                bytes[offset + 16],
                bytes[offset + 17],
                bytes[offset + 18],
                bytes[offset + 19],
            ]);
            let block_align = u16::from_le_bytes([bytes[offset + 20], bytes[offset + 21]]);
            let bits_per_sample = u16::from_le_bytes([bytes[offset + 22], bytes[offset + 23]]);
            return Ok((channels, sample_rate, byte_rate, block_align, bits_per_sample));
        }

        offset += 8 + chunk_size;
    }

    Err("找不到 fmt chunk".to_string())
}

fn extract_wav_pcm_data(bytes: &[u8]) -> Result<&[u8], String> {
    if bytes.len() < 44 {
        return Err("WAV 檔案長度不足".to_string());
    }

    let mut offset = 12;
    while offset + 8 <= bytes.len() {
        let chunk_id = &bytes[offset..offset + 4];
        let chunk_size = u32::from_le_bytes([
            bytes[offset + 4],
            bytes[offset + 5],
            bytes[offset + 6],
            bytes[offset + 7],
        ]) as usize;

        if chunk_id == b"data" {
            let data_start = offset + 8;
            let data_end = (data_start + chunk_size).min(bytes.len());
            return Ok(&bytes[data_start..data_end]);
        }

        offset += 8 + chunk_size;
    }

    // 後備：若未找到標籤，跳過標準 44 bytes
    Ok(&bytes[44..])
}

/// 產生指定毫秒長度的 MPEG-1 Layer III 44.1kHz 128kbps 靜音幀緩衝
pub fn make_mp3_silence(duration_ms: u32) -> Vec<u8> {
    if duration_ms == 0 {
        return Vec::new();
    }
    // 每一幀 1152 個取樣點，在 44100Hz 下約為 26.122 毫秒
    let frame_ms = 26.12245f32;
    let num_frames = (duration_ms as f32 / frame_ms).round().max(1.0) as usize;
    let mut silence = Vec::with_capacity(num_frames * 417);
    let mut frame = [0u8; 417];
    // MPEG-1 Layer III, 128kbps, 44.1kHz, Joint Stereo
    frame[0] = 0xFF;
    frame[1] = 0xFB;
    frame[2] = 0x90;
    frame[3] = 0x64;
    for _ in 0..num_frames {
        silence.extend_from_slice(&frame);
    }
    silence
}

/// 拼接多個 MP3 音訊緩衝區，並依序在句間插入指定靜音毫秒
pub fn concatenate_mp3_buffers_with_pauses(
    buffers: &[&[u8]],
    pauses_ms: &[u32],
) -> Result<Vec<u8>, String> {
    if buffers.is_empty() {
        return Err("音訊緩衝區清單為空".to_string());
    }

    let mut combined = Vec::new();

    for (i, buf) in buffers.iter().enumerate() {
        let clean_data = if i == 0 {
            // 保留第一個檔案的標頭
            *buf
        } else {
            // 剝離 ID3v2 標頭
            strip_mp3_id3(buf)
        };
        combined.extend_from_slice(clean_data);

        // 若非最後一段，插入指定停頓靜音幀
        if i + 1 < buffers.len() {
            let pause_ms = pauses_ms.get(i).copied().unwrap_or(0);
            if pause_ms > 0 {
                combined.extend_from_slice(&make_mp3_silence(pause_ms));
            }
        }
    }

    Ok(combined)
}

/// 拼接多個 MP3 音訊緩衝區（剝除後續 ID3v2 標頭確保音軌連續無縫）
pub fn concatenate_mp3_buffers(buffers: &[&[u8]]) -> Result<Vec<u8>, String> {
    concatenate_mp3_buffers_with_pauses(buffers, &[])
}

fn strip_mp3_id3(bytes: &[u8]) -> &[u8] {
    if bytes.len() >= 10 && &bytes[0..3] == b"ID3" {
        let size = ((bytes[6] as usize & 0x7F) << 21)
            | ((bytes[7] as usize & 0x7F) << 14)
            | ((bytes[8] as usize & 0x7F) << 7)
            | (bytes[9] as usize & 0x7F);
        let offset = 10 + size;
        if offset < bytes.len() {
            return &bytes[offset..];
        }
    }
    bytes
}

/// 觸發多角色語音生成非同步流程
pub fn start_multi_generation(
    api_key: String,
    state: &mut MultiSpeechState,
    tx: Sender<crate::app::WorkerMessage>,
) {
    if state.is_generating { return; }
    let key = api_key.trim().to_string();
    if key.is_empty() {
        state.error_message = Some("請先在左側設定中輸入 OpenRouter API Key".to_string());
        return;
    }

    if state.lines.is_empty() {
        state.error_message = Some("劇本台詞清單不可為空".to_string());
        return;
    }

    let valid_lines: Vec<DialogLine> = state
        .lines
        .iter()
        .filter(|l| !l.text.trim().is_empty())
        .cloned()
        .collect();

    if valid_lines.is_empty() {
        state.error_message = Some("請至少填寫一句非空白的對白台詞".to_string());
        return;
    }

    state.is_generating = true;
    state.error_message = None;
    state.status_message = "正在啟動多角色語音生成流程...".to_string();

    let cast = state.cast.clone();
    let mode = state.gen_mode;
    let model = state.selected_model.clone();
    let format = state.selected_format.clone();

    match mode {
        MultiGenMode::NativeSpeakerTags => {
            state.status_message = "正在透過 Fish Audio S2.1 原生多角色語法合成...".to_string();
            let full_prompt = build_native_multi_speaker_prompt(&cast, &valid_lines);

            let req = SpeechRequest {
                model,
                input: full_prompt,
                voice: None,
                response_format: Some(format.clone()),
                speed: Some(1.0),
            };

            let character_label = format!("多角色劇本 ({} 位登場)", cast.len());

            thread::spawn(move || {
                let client = OpenRouterClient::new();
                let res = client.synthesize(&key, &req);
                match res {
                    Ok(bytes) => {
                        let timestamp_str = Local::now().format("%Y%m%d_%H%M%S").to_string();
                        let filename = format!("dialog_{}.{}", timestamp_str, format);
                        let output_path = PathBuf::from("outputs").join(&filename);
                        // Saved by the completion handler with visible error reporting.

                        let duration_secs = rodio::Decoder::new(std::io::Cursor::new(bytes.clone()))
                            .ok()
                            .and_then(|d| d.total_duration())
                            .or_else(|| estimate_audio_duration(&bytes))
                            .map(|d: Duration| d.as_secs_f32());

                        let _ = tx.send(crate::app::WorkerMessage::SpeechGenerated {
                            result: Ok(bytes),
                            req,
                            character_name: character_label,
                            file_path: output_path.to_string_lossy().to_string(),
                            duration_secs,
                        });
                    }
                    Err(e) => {
                        let _ = tx.send(crate::app::WorkerMessage::SpeechGenerated {
                            result: Err(e),
                            req,
                            character_name: character_label,
                            file_path: String::new(),
                            duration_secs: None,
                        });
                    }
                }
            });
        }
        MultiGenMode::SequentialConcat => {
            state.total_steps = valid_lines.len();
            state.current_step = 1;
            state.status_message = format!("正在循序合成第 1 / {} 句台詞...", valid_lines.len());

            let character_label = format!("多角色對白拼接 ({} 句)", valid_lines.len());
            let tx_clone = tx.clone();

            thread::spawn(move || {
                let client = OpenRouterClient::new();
                let mut line_buffers = Vec::new();
                let mut full_script_text = Vec::new();
                let mut line_pauses = Vec::new();

                for (idx, line) in valid_lines.iter().enumerate() {
                    let spk = cast.iter().find(|c| c.speaker_id == line.speaker_id);
                    let tag = spk.map(|c| c.prompt_tag.trim()).unwrap_or("");
                    let speed = spk.map(|c| c.speed).unwrap_or(1.0);
                    let effective_tone = if !line.tone.trim().is_empty() {
                        line.tone.trim()
                    } else {
                        spk.map(|c| c.default_tone.trim()).unwrap_or("")
                    };

                    let _ = tx_clone.send(crate::app::WorkerMessage::SpeechProgress {
                        current: idx + 1,
                        total: valid_lines.len(),
                        message: format!(
                            "正在循序合成第 {} / {} 句 (Speaker {})...",
                            idx + 1,
                            valid_lines.len(),
                            line.speaker_id
                        ),
                    });

                    let mut input = String::new();
                    if !tag.is_empty() {
                        input.push_str(tag);
                        input.push(' ');
                    }
                    if !effective_tone.is_empty() {
                        input.push_str(effective_tone);
                        input.push(' ');
                    }
                    input.push_str(line.text.trim());
                    full_script_text.push(input.clone());
                    line_pauses.push(line.pause_after_ms);

                    let line_req = SpeechRequest {
                        model: model.clone(),
                        input,
                        voice: spk
                            .and_then(|c| c.custom_voice_id.clone())
                            .filter(|s| !s.trim().is_empty()),
                        response_format: Some(format.clone()),
                        speed: Some(speed),
                    };

                    match client.synthesize(&key, &line_req) {
                        Ok(b) => {
                            line_buffers.push(b);
                        }
                        Err(e) => {
                            let _ = tx_clone.send(crate::app::WorkerMessage::SpeechGenerated {
                                result: Err(format!("第 {} 句台詞合成失敗: {}", idx + 1, e)),
                                req: line_req,
                                character_name: character_label,
                                file_path: String::new(),
                                duration_secs: None,
                            });
                            return;
                        }
                    }
                }

                let _ = tx_clone.send(crate::app::WorkerMessage::SpeechProgress {
                    current: valid_lines.len(),
                    total: valid_lines.len(),
                    message: "所有台詞已合成完畢，正在進行音訊拼接與停頓處理...".to_string(),
                });

                // 進行拼接 (依各行自訂的 pause_after_ms 精準插入靜音)
                let refs: Vec<&[u8]> = line_buffers.iter().map(|b| b.as_slice()).collect();
                let concat_res = if format.eq_ignore_ascii_case("wav") {
                    concatenate_wav_buffers_with_pauses(&refs, &line_pauses)
                } else {
                    concatenate_mp3_buffers_with_pauses(&refs, &line_pauses)
                };

                match concat_res {
                    Ok(final_bytes) => {
                        let timestamp_str = Local::now().format("%Y%m%d_%H%M%S").to_string();
                        let filename = format!("dialog_concat_{}.{}", timestamp_str, format);
                        let output_path = PathBuf::from("outputs").join(&filename);
                        // Saved by the completion handler with visible error reporting.

                        let duration_secs = estimate_audio_duration(&final_bytes)
                            .map(|d| d.as_secs_f32());

                        let dummy_req = SpeechRequest {
                            model,
                            input: full_script_text.join("\n"),
                            voice: None,
                            response_format: Some(format),
                            speed: Some(1.0),
                        };

                        let _ = tx_clone.send(crate::app::WorkerMessage::SpeechGenerated {
                            result: Ok(final_bytes),
                            req: dummy_req,
                            character_name: character_label,
                            file_path: output_path.to_string_lossy().to_string(),
                            duration_secs,
                        });
                    }
                    Err(e) => {
                        let dummy_req = SpeechRequest {
                            model,
                            input: "拼接失敗".to_string(),
                            voice: None,
                            response_format: Some(format),
                            speed: Some(1.0),
                        };
                        let _ = tx_clone.send(crate::app::WorkerMessage::SpeechGenerated {
                            result: Err(format!("音訊拼接失敗: {}", e)),
                            req: dummy_req,
                            character_name: character_label,
                            file_path: String::new(),
                            duration_secs: None,
                        });
                    }
                }
            });
        }
    }
}

/// 繪製多角色語音生成管理頁面 UI
pub fn render_multi_speech_page(
    ui: &mut egui::Ui,
    state: &mut MultiSpeechState,
    presets: &[CharacterPreset],
    api_key: &str,
    audio_player: &mut crate::audio::AudioPlayer,
    tx: Sender<crate::app::WorkerMessage>,
) {
    ui.add_space(6.0);

    // 錯誤與成功訊息橫幅
    let mut dismiss_err = false;
    if let Some(err) = &state.error_message {
        egui::Frame::NONE
            .fill(Color32::from_rgba_premultiplied(239, 68, 68, 35))
            .stroke(Stroke::new(1.0, Color32::from_rgb(239, 68, 68)))
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
        state.error_message = None;
    }

    let mut dismiss_toast = false;
    if let Some(toast) = &state.success_toast {
        egui::Frame::NONE
            .fill(Color32::from_rgba_premultiplied(34, 197, 94, 30))
            .stroke(Stroke::new(1.0, Color32::from_rgb(34, 197, 94)))
            .corner_radius(6)
            .inner_margin(8.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("✓").color(Color32::from_rgb(34, 197, 94)).strong());
                    ui.label(RichText::new(toast).color(Color32::from_rgb(34, 197, 94)));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("✕").clicked() {
                            dismiss_toast = true;
                        }
                    });
                });
            });
        ui.add_space(4.0);
    }
    if dismiss_toast {
        state.success_toast = None;
    }

    egui::ScrollArea::vertical().show(ui, |ui| {
        // ======================= 1. 登場角色配置卡片 =======================
        egui::Frame::group(ui.style())
            .corner_radius(8)
            .inner_margin(12.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.heading(RichText::new("👥 劇本登場角色配置 (Cast)").strong());

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // 載入劇本範本下拉選單
                        egui::ComboBox::from_id_salt("script_template_combo")
                            .selected_text("📋 載入示範對白劇本")
                            .show_ui(ui, |ui| {
                                for t in get_script_templates() {
                                    if ui.selectable_label(false, t.title).clicked() {
                                        state.load_template(t);
                                    }
                                }
                            });

                        if ui.button("➕ 新增登場角色").clicked() {
                            state.add_speaker(presets);
                        }
                    });
                });

                ui.add_space(6.0);

                // 角色卡片列表
                let mut remove_id = None;
                let can_remove = state.cast.len() > 2;

                ui.horizontal_wrapped(|ui| {
                    for member in &mut state.cast {
                        let badge_c = Color32::from_rgb(
                            member.badge_color[0],
                            member.badge_color[1],
                            member.badge_color[2],
                        );

                        egui::Frame::group(ui.style())
                            .corner_radius(6)
                            .stroke(Stroke::new(1.0, badge_c))
                            .inner_margin(10.0)
                            .show(ui, |ui| {
                                ui.set_min_width(260.0);

                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new(format!("Speaker {}", member.speaker_id))
                                            .color(badge_c)
                                            .strong(),
                                    );

                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        if can_remove
                                            && ui.small_button(RichText::new("✕").color(Color32::from_rgb(239, 68, 68))).clicked()
                                        {
                                            remove_id = Some(member.speaker_id);
                                        }
                                    });
                                });

                                ui.add_space(4.0);
                                ui.horizontal(|ui| {
                                    ui.label("名稱/角色:");
                                    ui.add(egui::TextEdit::singleline(&mut member.name).desired_width(150.0));
                                });

                                ui.add_space(4.0);
                                // 預設角色聲線選單
                                let current_preset_name = presets
                                    .get(member.character_preset_idx)
                                    .map(|p| p.name.as_str())
                                    .unwrap_or("自訂");

                                egui::ComboBox::from_id_salt(format!("spk_preset_{}", member.speaker_id))
                                    .selected_text(current_preset_name)
                                    .show_ui(ui, |ui| {
                                        for (p_idx, p) in presets.iter().enumerate() {
                                            if ui.selectable_value(&mut member.character_preset_idx, p_idx, &p.name).clicked() {
                                                member.prompt_tag = p.prompt_tag.clone();
                                                member.speed = p.recommended_speed;
                                                if let Some(ref def_t) = p.default_tone {
                                                    member.default_tone = def_t.clone();
                                                }
                                            }
                                        }
                                    });

                                ui.add_space(2.0);
                                ui.horizontal(|ui| {
                                    ui.label("音色標籤:");
                                    ui.add(egui::TextEdit::singleline(&mut member.prompt_tag).desired_width(150.0));
                                });

                                ui.add_space(2.0);
                                ui.horizontal(|ui| {
                                    ui.label("預設口氣:");
                                    ui.add(egui::TextEdit::singleline(&mut member.default_tone).hint_text("[calm] 或 [happy]").desired_width(150.0));
                                });

                                ui.add_space(2.0);
                                ui.horizontal(|ui| {
                                    ui.label("Voice ID:");
                                    let mut voice_str = member.custom_voice_id.clone().unwrap_or_default();
                                    let edit = egui::TextEdit::singleline(&mut voice_str).hint_text("自訂 reference ID (可選)").desired_width(150.0);
                                    if ui.add(edit).changed() {
                                        member.custom_voice_id = if voice_str.trim().is_empty() {
                                            None
                                        } else {
                                            Some(voice_str.trim().to_string())
                                        };
                                    }
                                });

                                ui.add_space(2.0);
                                ui.horizontal(|ui| {
                                    ui.label("語速:");
                                    ui.add(egui::Slider::new(&mut member.speed, 0.6..=1.8).step_by(0.05).suffix("x"));
                                });
                            });
                    }
                });

                if let Some(id) = remove_id {
                    state.remove_speaker(id);
                }
            });

        ui.add_space(10.0);

        // ======================= 2. 分行劇本對白編輯器 =======================
        egui::Frame::group(ui.style())
            .corner_radius(8)
            .inner_margin(12.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.heading(RichText::new(format!("📝 分行劇本對白編輯 ({})", state.lines.len())).strong());

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // 匯出純文字
                        if ui.button("📄 匯出文字劇本").clicked() {
                            let text = state.export_text_script();
                            if let Some(path) = rfd::FileDialog::new()
                                .set_file_name("dialog_script.txt")
                                .add_filter("Text File", &["txt"])
                                .save_file()
                            {
                                let _ = fs::write(&path, text);
                                state.success_toast = Some("劇本文本已匯出".to_string());
                            }
                        }

                        // 快速新增台詞
                        let default_spk = state.cast.first().map(|c| c.speaker_id).unwrap_or(0);
                        if ui.button("➕ 新增一行台詞").clicked() {
                            state.add_line(default_spk);
                        }

                        if !state.lines.is_empty() && ui.button("🗑️ 清空台詞").clicked() {
                            state.lines.clear();
                        }
                    });
                });

                ui.add_space(6.0);

                // 快速口氣快捷按鈕
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("快捷常用標籤:").size(11.5).color(Color32::from_rgb(156, 163, 175)));
                    let quick_tags = ["[calm]", "[happy]", "[whispering]", "[serious]", "[angry]", "[excited]", "[sigh]", "[chuckle]"];
                    for q in quick_tags {
                        if ui.small_button(q).clicked()
                            && let Some(last_line) = state.lines.last_mut()
                            && !last_line.tone.contains(q)
                        {
                            if !last_line.tone.is_empty() {
                                last_line.tone.push(' ');
                            }
                            last_line.tone.push_str(q);
                        }
                    }
                });

                ui.add_space(6.0);

                // 對白行清單
                let mut move_up_idx = None;
                let mut move_down_idx = None;
                let mut duplicate_idx = None;
                let mut delete_line_id = None;
                let mut single_preview_line = None;

                let cast_cloned = state.cast.clone();

                for (idx, line) in state.lines.iter_mut().enumerate() {
                    let spk_info = cast_cloned.iter().find(|c| c.speaker_id == line.speaker_id);
                    let badge_c = spk_info.map(|c| Color32::from_rgb(c.badge_color[0], c.badge_color[1], c.badge_color[2]))
                        .unwrap_or(Color32::from_rgb(156, 163, 175));

                    egui::Frame::group(ui.style())
                        .corner_radius(6)
                        .stroke(Stroke::new(1.0, badge_c.linear_multiply(0.6)))
                        .inner_margin(8.0)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                // 序號與說話者選單
                                ui.label(RichText::new(format!("#{}.", idx + 1)).strong());

                                let current_spk_label = spk_info
                                    .map(|c| format!("Speaker {} ({})", c.speaker_id, c.name))
                                    .unwrap_or_else(|| format!("Speaker {}", line.speaker_id));

                                egui::ComboBox::from_id_salt(format!("line_spk_{}", line.id))
                                    .selected_text(RichText::new(current_spk_label).color(badge_c))
                                    .show_ui(ui, |ui| {
                                        for c in &cast_cloned {
                                            let label = format!("Speaker {} ({})", c.speaker_id, c.name);
                                            if ui.selectable_value(&mut line.speaker_id, c.speaker_id, label).clicked()
                                                && line.tone.trim().is_empty()
                                                && !c.default_tone.trim().is_empty()
                                            {
                                                line.tone = c.default_tone.clone();
                                            }
                                        }
                                    });

                                ui.add_space(6.0);
                                ui.label("口氣:");
                                ui.add(
                                    egui::TextEdit::singleline(&mut line.tone)
                                        .hint_text("[happy] [whispering]")
                                        .desired_width(110.0),
                                );

                                if let Some(c) = spk_info
                                    && !c.default_tone.is_empty()
                                    && ui.small_button("↺ 預設").on_hover_text("帶入該說話者的預設口氣").clicked()
                                {
                                    line.tone = c.default_tone.clone();
                                }

                                ui.add_space(6.0);
                                ui.label("停頓:");
                                ui.add(egui::DragValue::new(&mut line.pause_after_ms).suffix("ms").range(0..=5000).speed(50))
                                    .on_hover_text("此句朗讀完畢後的靜音停頓時間 (毫秒)");

                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    // 刪除行
                                    if ui.small_button(RichText::new("🗑️").color(Color32::from_rgb(239, 68, 68))).on_hover_text("刪除此行").clicked() {
                                        delete_line_id = Some(line.id);
                                    }

                                    // 下移
                                    if ui.small_button("▼").on_hover_text("下移一行").clicked() {
                                        move_down_idx = Some(idx);
                                    }

                                    // 上移
                                    if ui.small_button("▲").on_hover_text("上移一行").clicked() {
                                        move_up_idx = Some(idx);
                                    }

                                    // 複製
                                    if ui.small_button("📄").on_hover_text("複製此行").clicked() {
                                        duplicate_idx = Some(idx);
                                    }

                                    // 試聽單句按鈕
                                    let is_previewing = state.preview_line_id == Some(line.id);
                                    if is_previewing {
                                        ui.horizontal(|ui| {
                                            ui.spinner();
                                            ui.label(RichText::new("試聽中...").color(Color32::from_rgb(99, 102, 241)).size(11.0));
                                        });
                                    } else if ui.add_enabled(!state.is_generating && !api_key.trim().is_empty() && !line.text.trim().is_empty(), egui::Button::new("▶ 試聽單句")).on_hover_text("單獨合成並試聽此句台詞").clicked() {
                                        single_preview_line = Some(line.clone());
                                    }
                                });
                            });

                            ui.add_space(4.0);
                            let text_edit = egui::TextEdit::multiline(&mut line.text)
                                .desired_rows(2)
                                .desired_width(f32::INFINITY)
                                .hint_text("在此輸入該角色的對白台詞...");
                            ui.add(text_edit);
                        });

                    ui.add_space(4.0);
                }

                // 處理台詞排序與刪除動作
                if let Some(idx) = move_up_idx {
                    state.move_line_up(idx);
                }
                if let Some(idx) = move_down_idx {
                    state.move_line_down(idx);
                }
                if let Some(idx) = duplicate_idx {
                    state.duplicate_line(idx);
                }
                if let Some(id) = delete_line_id {
                    state.delete_line(id);
                }

                // 處理單句試聽
                if let Some(line) = single_preview_line {
                    let spk = state.cast.iter().find(|c| c.speaker_id == line.speaker_id);
                    let tag = spk.map(|c| c.prompt_tag.trim()).unwrap_or("");
                    let speed = spk.map(|c| c.speed).unwrap_or(1.0);
                    let effective_tone = if !line.tone.trim().is_empty() {
                        line.tone.trim()
                    } else {
                        spk.map(|c| c.default_tone.trim()).unwrap_or("")
                    };

                    let mut input = String::new();
                    if !tag.is_empty() {
                        input.push_str(tag);
                        input.push(' ');
                    }
                    if !effective_tone.is_empty() {
                        input.push_str(effective_tone);
                        input.push(' ');
                    }
                    input.push_str(line.text.trim());

                    let req = SpeechRequest {
                        model: state.selected_model.clone(),
                        input,
                        voice: spk
                            .and_then(|c| c.custom_voice_id.clone())
                            .filter(|s| !s.trim().is_empty()),
                        response_format: Some("mp3".to_string()),
                        speed: Some(speed),
                    };

                    let key = api_key.trim().to_string();
                    let tx_clone = tx.clone();
                    state.preview_line_id = Some(line.id);
                    state.is_generating = true;
                    state.status_message = format!("正在試聽 Speaker {} 的單句台詞...", line.speaker_id);

                    thread::spawn(move || {
                        let client = OpenRouterClient::new();
                        let res = client.synthesize(&key, &req);
                        let _ = tx_clone.send(crate::app::WorkerMessage::SpeechGenerated {
                            result: res,
                            req,
                            character_name: format!("單句試聽 (Speaker {})", line.speaker_id),
                            file_path: String::new(),
                            duration_secs: None,
                        });
                    });
                }
            });

        ui.add_space(10.0);

        // ======================= 3. 生成設定與播放卡片 =======================
        egui::Frame::group(ui.style())
            .corner_radius(8)
            .inner_margin(14.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.strong("⚙️ 多角色合成引擎設定");

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new("Fish Audio S2.1 Pro TTS").color(Color32::from_rgb(16, 185, 129)).size(11.0));
                    });
                });

                ui.add_space(6.0);

                ui.horizontal(|ui| {
                    ui.label("合成模式:");
                    ui.selectable_value(
                        &mut state.gen_mode,
                        MultiGenMode::NativeSpeakerTags,
                        "原生 <|speaker:X|> 語法 (推薦)",
                    );
                    ui.selectable_value(
                        &mut state.gen_mode,
                        MultiGenMode::SequentialConcat,
                        "分行生成與拼接",
                    );
                });

                ui.add_space(4.0);

                ui.horizontal(|ui| {
                    ui.label("輸出格式:");
                    ui.selectable_value(&mut state.selected_format, "mp3".to_string(), "MP3");
                    ui.selectable_value(&mut state.selected_format, "wav".to_string(), "WAV");

                    ui.add_space(16.0);
                    ui.separator();
                    ui.add_space(16.0);

                    if state.gen_mode == MultiGenMode::SequentialConcat {
                        ui.label("預設台詞間停頓 (毫秒):");
                        ui.add(egui::Slider::new(&mut state.default_pause_ms, 50..=1200).suffix("ms"));
                    }
                });

                ui.add_space(8.0);

                // 合成按鈕與播放控制器
                ui.horizontal(|ui| {
                    let gen_text = if state.is_generating {
                        "🎙️ 正在產生多角色語音..."
                    } else {
                        "🎙️ 立即合成多角色語音"
                    };

                    let btn = egui::Button::new(RichText::new(gen_text).size(16.0).strong())
                        .fill(Color32::from_rgb(79, 70, 229))
                        .min_size(Vec2::new(220.0, 40.0))
                        .corner_radius(6);

                    if ui.add_enabled(!state.is_generating, btn).clicked() {
                        start_multi_generation(api_key.to_string(), state, tx.clone());
                    }

                    if state.is_generating {
                        ui.spinner();
                        if state.total_steps > 0 {
                            let pct = (state.current_step as f32 / state.total_steps as f32).clamp(0.0, 1.0);
                            ui.add(egui::ProgressBar::new(pct).text(format!("{}/{}", state.current_step, state.total_steps)).desired_width(120.0));
                        }
                    }

                    ui.add_space(16.0);
                    ui.separator();
                    ui.add_space(16.0);

                    // 播放器控制
                    let is_playing = audio_player.is_playing();
                    let is_paused = audio_player.is_paused();

                    let play_icon = if is_playing { "⏸ 暫停" } else { "▶ 播放" };
                    if ui
                        .add_enabled(
                            audio_player.current_bytes().is_some(),
                            egui::Button::new(play_icon).min_size(Vec2::new(75.0, 36.0)),
                        )
                        .clicked()
                    {
                        if is_playing {
                            audio_player.pause();
                        } else if is_paused {
                            audio_player.resume();
                        } else {
                            let _ = audio_player.replay();
                        }
                    }

                    if ui
                        .add_enabled(
                            is_playing || is_paused,
                            egui::Button::new("⏹ 停止").min_size(Vec2::new(70.0, 36.0)),
                        )
                        .clicked()
                    {
                        audio_player.stop();
                    }

                    if ui
                        .add_enabled(
                            audio_player.current_bytes().is_some(),
                            egui::Button::new("🔁 重播").min_size(Vec2::new(70.0, 36.0)),
                        )
                        .clicked()
                    {
                        let _ = audio_player.replay();
                    }

                    // 另存對白音檔
                    if ui
                        .add_enabled(
                            audio_player.current_bytes().is_some(),
                            egui::Button::new("💾 另存對白音檔").min_size(Vec2::new(95.0, 36.0)),
                        )
                        .clicked()
                        && let Some(bytes) = audio_player.current_bytes().cloned()
                    {
                        let ext = &state.selected_format;
                        let default_name = format!("dialog_{}.{}", Local::now().format("%Y%m%d_%H%M%S"), ext);
                        if let Some(path) = rfd::FileDialog::new()
                            .set_file_name(&default_name)
                            .add_filter("Audio File", &[ext.as_str(), "mp3", "wav"])
                            .save_file()
                        {
                            if let Err(e) = export_audio_bytes(&bytes, &path) {
                                state.error_message = Some(format!("另存失敗: {}", e));
                            } else {
                                state.success_toast = Some(format!("已成功儲存至: {}", path.display()));
                            }
                        }
                    }
                });

                ui.add_space(8.0);

                // 進度與音量控制
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

                    let bar_width = (ui.available_width() - 170.0).max(100.0);
                    if total_secs > 0.0 {
                        let slider = egui::Slider::new(&mut current_secs, 0.0..=total_secs)
                            .show_value(false);
                        let response = ui.add_sized([bar_width, 18.0], slider);
                        if response.drag_stopped() {
                            let _ = audio_player.seek(Duration::from_secs_f32(current_secs));
                        }
                    } else {
                        let progress_bar = egui::ProgressBar::new(0.0)
                            .animate(audio_player.is_playing());
                        ui.add_sized([bar_width, 18.0], progress_bar);
                    }

                    // 音量
                    ui.label("🔊");
                    let mut vol = audio_player.get_volume();
                    if ui.add(egui::Slider::new(&mut vol, 0.0..=1.0).show_value(false)).changed() {
                        audio_player.set_volume(vol);
                    }
                });
            });

        ui.add_space(16.0);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_multi_speech_state_creation_and_cast() {
        let mut state = MultiSpeechState::new();
        assert_eq!(state.cast.len(), 2);
        assert_eq!(state.lines.len(), 3);

        let presets = get_default_characters();
        state.add_speaker(&presets);
        assert_eq!(state.cast.len(), 3);

        state.remove_speaker(state.cast.last().unwrap().speaker_id);
        assert_eq!(state.cast.len(), 2);
    }

    #[test]
    fn test_build_native_multi_speaker_prompt() {
        let cast = vec![
            CastMember {
                speaker_id: 0,
                name: "角色A".to_string(),
                character_preset_idx: 0,
                prompt_tag: "[女僕聲線]".to_string(),
                custom_voice_id: None,
                default_tone: "[calm]".to_string(),
                speed: 1.0,
                badge_color: [0, 0, 0],
            },
            CastMember {
                speaker_id: 1,
                name: "角色B".to_string(),
                character_preset_idx: 1,
                prompt_tag: "[青年聲線]".to_string(),
                custom_voice_id: None,
                default_tone: "[happy]".to_string(),
                speed: 1.0,
                badge_color: [0, 0, 0],
            },
        ];

        let lines = vec![
            DialogLine {
                id: 1,
                speaker_id: 0,
                tone: "[calm]".to_string(),
                text: "主人，歡迎回家。".to_string(),
                pause_after_ms: 300,
            },
            DialogLine {
                id: 2,
                speaker_id: 1,
                tone: "[happy]".to_string(),
                text: "我回來了！".to_string(),
                pause_after_ms: 300,
            },
        ];

        let prompt = build_native_multi_speaker_prompt(&cast, &lines);
        assert!(prompt.contains("<|speaker:0|>"));
        assert!(prompt.contains("[女僕聲線]"));
        assert!(prompt.contains("主人，歡迎回家。"));
        assert!(prompt.contains("<|speaker:1|>"));
        assert!(prompt.contains("[青年聲線]"));
        assert!(prompt.contains("我回來了！"));
    }

    #[test]
    fn test_script_templates() {
        let templates = get_script_templates();
        assert!(templates.len() >= 3);
        for t in &templates {
            assert!(!t.title.is_empty());
            assert!(t.cast.len() >= 2);
            assert!(t.lines.len() >= 3);
        }
    }

    #[test]
    fn test_wav_concatenation() {
        // 構造兩個 44-byte WAV 檔案 (各含 1000 samples = 2000 bytes PCM)
        fn make_dummy_wav(pcm_size: usize) -> Vec<u8> {
            let mut wav = vec![0u8; 44 + pcm_size];
            wav[0..4].copy_from_slice(b"RIFF");
            let riff_size = (pcm_size + 36) as u32;
            wav[4..8].copy_from_slice(&riff_size.to_le_bytes());
            wav[8..12].copy_from_slice(b"WAVE");
            wav[12..16].copy_from_slice(b"fmt ");
            wav[16..20].copy_from_slice(&16u32.to_le_bytes());
            wav[20..22].copy_from_slice(&1u16.to_le_bytes()); // PCM
            wav[22..24].copy_from_slice(&1u16.to_le_bytes()); // 1 channel
            let sample_rate = 44100u32;
            wav[24..28].copy_from_slice(&sample_rate.to_le_bytes());
            let byte_rate = 88200u32;
            wav[28..32].copy_from_slice(&byte_rate.to_le_bytes());
            let block_align = 2u16;
            wav[32..34].copy_from_slice(&block_align.to_le_bytes());
            let bits = 16u16;
            wav[34..36].copy_from_slice(&bits.to_le_bytes());
            wav[36..40].copy_from_slice(b"data");
            let data_size = pcm_size as u32;
            wav[40..44].copy_from_slice(&data_size.to_le_bytes());
            wav
        }

        let w1 = make_dummy_wav(1000);
        let w2 = make_dummy_wav(2000);
        let combined = concatenate_wav_buffers(&[&w1, &w2], 100).expect("WAV 拼接應成功");

        assert_eq!(&combined[0..4], b"RIFF");
        assert_eq!(&combined[8..12], b"WAVE");
        // 100ms at 88200 bytes/sec = 8820 bytes of silence
        let expected_min_pcm = 1000 + 2000 + 8820;
        assert!(combined.len() >= 44 + expected_min_pcm);
    }
}
