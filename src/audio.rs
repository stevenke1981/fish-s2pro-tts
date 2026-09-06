use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink, Source};
use std::io::Cursor;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// 估算音訊資料的時長 (支援 WAV 與 MP3 格式)
pub fn estimate_audio_duration(bytes: &[u8]) -> Option<Duration> {
    estimate_audio_duration_with_size(bytes, bytes.len() as u64)
}

/// 根據音訊標頭資料與檔案總大小估算時長 (支援 WAV 與 MP3 格式，不需讀取完整檔案)
pub fn estimate_audio_duration_with_size(header_bytes: &[u8], total_file_size: u64) -> Option<Duration> {
    if header_bytes.len() < 12 || total_file_size == 0 {
        return None;
    }

    // 1. WAV 格式解析 (RIFF .... WAVE)
    if &header_bytes[0..4] == b"RIFF" && &header_bytes[8..12] == b"WAVE" && header_bytes.len() >= 44 {
        let byte_rate = u32::from_le_bytes([header_bytes[28], header_bytes[29], header_bytes[30], header_bytes[31]]);
        if byte_rate > 0 {
            let data_len = (total_file_size.saturating_sub(44)) as f64;
            let secs = data_len / (byte_rate as f64);
            return Some(Duration::from_secs_f64(secs.max(0.1)));
        }
    }

    // 2. MP3 格式解析
    let mut offset = 0;
    // 跳過 ID3v2 標頭 (若存在)
    if header_bytes.len() >= 10 && &header_bytes[0..3] == b"ID3" {
        let size = ((header_bytes[6] as usize & 0x7F) << 21)
            | ((header_bytes[7] as usize & 0x7F) << 14)
            | ((header_bytes[8] as usize & 0x7F) << 7)
            | (header_bytes[9] as usize & 0x7F);
        offset = 10 + size;
    }

    // 搜尋 MP3 Frame 同步字元 (0xFF followed by 0xE0..=0xFF)
    for i in offset..header_bytes.len().saturating_sub(4) {
        if header_bytes[i] == 0xFF && (header_bytes[i + 1] & 0xE0) == 0xE0 {
            let version_bits = (header_bytes[i + 1] >> 3) & 0x03;
            let layer_bits = (header_bytes[i + 1] >> 1) & 0x03;
            let bitrate_idx = ((header_bytes[i + 2] >> 4) & 0x0F) as usize;

            // MPEG-1 Layer III
            if version_bits == 3 && layer_bits == 1 && bitrate_idx > 0 && bitrate_idx < 15 {
                let bitrates = [0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320];
                let kbps = bitrates[bitrate_idx];
                let audio_bytes = (total_file_size.saturating_sub(i as u64)) as f64;
                let secs = (audio_bytes * 8.0) / (kbps as f64 * 1000.0);
                return Some(Duration::from_secs_f64(secs.max(0.1)));
            }

            // MPEG-2 Layer III
            if version_bits == 2 && layer_bits == 1 && bitrate_idx > 0 && bitrate_idx < 15 {
                let bitrates = [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160];
                let kbps = bitrates[bitrate_idx];
                let audio_bytes = (total_file_size.saturating_sub(i as u64)) as f64;
                let secs = (audio_bytes * 8.0) / (kbps as f64 * 1000.0);
                return Some(Duration::from_secs_f64(secs.max(0.1)));
            }
        }
    }

    // 通用後備：以 Fish Audio 標準 MP3 預設 128kbps 估算
    if total_file_size > 1024 {
        let secs = (total_file_size as f64 * 8.0) / (128.0 * 1000.0);
        return Some(Duration::from_secs_f64(secs.max(0.2)));
    }

    None
}

pub struct AudioPlayer {
    device_access_enabled: bool,
    _stream: Option<OutputStream>,
    stream_handle: Option<OutputStreamHandle>,
    sink: Option<Sink>,
    volume: f32,
    is_paused: bool,
    start_time: Option<Instant>,
    pause_offset: Duration,
    total_duration: Option<Duration>,
    current_bytes: Option<Vec<u8>>,
    is_active: Arc<AtomicBool>,
}

impl Default for AudioPlayer {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioPlayer {
    pub fn new() -> Self {
        let mut player = Self::new_headless();
        player.device_access_enabled = true;
        if let Err(e) = player.reconnect_device() { eprintln!("{}", e); }
        player
    }

    /// Device-free player for deterministic decoding, export and state tests.
    pub fn new_headless() -> Self {
        Self {
            device_access_enabled: false,
            _stream: None,
            stream_handle: None,
            sink: None,
            volume: 1.0,
            is_paused: false,
            start_time: None,
            pause_offset: Duration::ZERO,
            total_duration: None,
            current_bytes: None,
            is_active: Arc::new(AtomicBool::new(false)),
        }
    }

    /// 檢查是否有可用的音訊輸出裝置
    pub fn is_device_available(&self) -> bool {
        self.stream_handle.is_some()
    }

    /// 重新嘗試連接音訊裝置
    pub fn reconnect_device(&mut self) -> Result<(), String> {
        if !self.device_access_enabled {
            return Err("無介面播放器不連接音訊裝置".to_string());
        }
        match OutputStream::try_default() {
            Ok((s, h)) => {
                self._stream = Some(s);
                self.stream_handle = Some(h);
                Ok(())
            }
            Err(e) => Err(format!("無法連接音訊輸出設備: {}", e)),
        }
    }

    /// Load a generated result without requiring a playback device or starting sound.
    pub fn load_bytes(&mut self, bytes: Vec<u8>) {
        self.stop();
        self.total_duration = Decoder::new(Cursor::new(bytes.clone())).ok()
            .and_then(|d| d.total_duration()).or_else(|| estimate_audio_duration(&bytes));
        self.current_bytes = Some(bytes);
    }

    /// 播放給定的音訊位元組（MP3 / WAV 等）
    pub fn play_bytes(&mut self, bytes: Vec<u8>) -> Result<(), String> {
        self.load_bytes(bytes.clone());
        if self.stream_handle.is_none() {
            self.reconnect_device()?;
        }

        // 停止之前的播放
        self.stop();

        let handle = self
            .stream_handle
            .as_ref()
            .ok_or_else(|| "系統未檢測到可用音訊輸出裝置".to_string())?;

        let cursor = Cursor::new(bytes.clone());
        let decoder = Decoder::new(cursor).map_err(|e| format!("音訊解碼失敗: {}", e))?;

        // 優先讀取解碼器時長，若無則透過位元率估算
        let duration = decoder.total_duration().or_else(|| estimate_audio_duration(&bytes));

        let sink = Sink::try_new(handle).map_err(|e| format!("建立音訊輸出通道失敗: {}", e))?;
        sink.set_volume(self.volume);
        sink.append(decoder);

        self.sink = Some(sink);
        self.is_paused = false;
        self.start_time = Some(Instant::now());
        self.pause_offset = Duration::ZERO;
        self.total_duration = duration;
        self.current_bytes = Some(bytes);
        self.is_active.store(true, Ordering::SeqCst);

        Ok(())
    }

    /// 快進/跳轉至指定時長
    pub fn seek(&mut self, target: Duration) -> Result<(), String> {
        let bytes = self
            .current_bytes
            .clone()
            .ok_or_else(|| "沒有可跳轉播放的音訊".to_string())?;

        if self.stream_handle.is_none() {
            self.reconnect_device()?;
        }

        let handle = self
            .stream_handle
            .as_ref()
            .ok_or_else(|| "系統未檢測到可用音訊輸出裝置".to_string())?;

        // 停止目前通道
        if let Some(s) = self.sink.take() {
            s.stop();
        }

        let cursor = Cursor::new(bytes);
        let decoder = Decoder::new(cursor).map_err(|e| format!("跳轉時音訊解碼失敗: {}", e))?;
        let skipped = decoder.skip_duration(target);

        let sink = Sink::try_new(handle).map_err(|e| format!("建立音訊輸出通道失敗: {}", e))?;
        sink.set_volume(self.volume);
        sink.append(skipped);

        if self.is_paused {
            sink.pause();
            self.start_time = None;
        } else {
            self.start_time = Some(Instant::now());
        }

        self.pause_offset = target;
        self.sink = Some(sink);
        self.is_active.store(true, Ordering::SeqCst);

        Ok(())
    }

    /// 暫停播放
    pub fn pause(&mut self) {
        if let Some(sink) = &self.sink
            && !sink.is_paused()
            && !sink.empty()
        {
            sink.pause();
            self.is_paused = true;
            if let Some(start) = self.start_time {
                self.pause_offset += start.elapsed();
                self.start_time = None;
            }
        }
    }

    /// 恢復播放
    pub fn resume(&mut self) {
        if let Some(sink) = &self.sink
            && sink.is_paused()
        {
            sink.play();
            self.is_paused = false;
            self.start_time = Some(Instant::now());
        }
    }

    /// 停止播放
    pub fn stop(&mut self) {
        if let Some(sink) = self.sink.take() {
            sink.stop();
        }
        self.is_paused = false;
        self.start_time = None;
        self.pause_offset = Duration::ZERO;
        self.is_active.store(false, Ordering::SeqCst);
    }

    /// 重播當前音訊
    pub fn replay(&mut self) -> Result<(), String> {
        if let Some(bytes) = self.current_bytes.clone() {
            self.play_bytes(bytes)
        } else {
            Err("沒有可重播的音訊".to_string())
        }
    }

    /// 設定音量 (0.0 ~ 1.0)
    pub fn set_volume(&mut self, volume: f32) {
        let clamped = volume.clamp(0.0, 1.0);
        self.volume = clamped;
        if let Some(sink) = &self.sink {
            sink.set_volume(clamped);
        }
    }

    pub fn get_volume(&self) -> f32 {
        self.volume
    }

    /// 是否正在播放中（非暫停且未結束）
    pub fn is_playing(&self) -> bool {
        if let Some(sink) = &self.sink {
            !sink.is_paused() && !sink.empty()
        } else {
            false
        }
    }

    /// 是否處於暫停狀態
    pub fn is_paused(&self) -> bool {
        self.is_paused
    }

    /// 當前已播放時長 (若已播放完畢則停留於總時長，不無限溢出)
    pub fn elapsed(&self) -> Duration {
        if let Some(sink) = &self.sink
            && sink.empty()
        {
            return self.total_duration.unwrap_or(Duration::ZERO);
        }

        if self.is_paused {
            self.pause_offset
        } else if let Some(start) = self.start_time {
            let total_dur = self.pause_offset + start.elapsed();
            if let Some(max_dur) = self.total_duration {
                total_dur.min(max_dur)
            } else {
                total_dur
            }
        } else {
            Duration::ZERO
        }
    }

    /// 總時長
    pub fn total_duration(&self) -> Option<Duration> {
        self.total_duration
    }

    /// 當前已載入的原始音訊位元組
    pub fn current_bytes(&self) -> Option<&Vec<u8>> {
        self.current_bytes.as_ref()
    }
}

/// 解碼任意受支援音訊位元組（WAV/MP3 等）為 16-bit PCM 取樣點、取樣率與聲道數
pub fn decode_to_pcm(bytes: &[u8]) -> Result<(Vec<i16>, u32, u16), String> {
    let cursor = Cursor::new(bytes.to_vec());
    let decoder = Decoder::new(cursor).map_err(|e| format!("音訊解碼失敗: {}", e))?;
    let channels = decoder.channels();
    let sample_rate = decoder.sample_rate();
    let samples: Vec<i16> = decoder.collect();
    Ok((samples, sample_rate, channels))
}

/// 將 16-bit PCM 取樣點封裝為標準 RIFF/WAVE 44-byte 檔頭與資料
pub fn encode_pcm_to_wav(samples: &[i16], sample_rate: u32, channels: u16) -> Vec<u8> {
    let byte_rate = sample_rate * channels as u32 * 2;
    let block_align = channels * 2;
    let pcm_len = (samples.len() * 2) as u32;
    let riff_size = pcm_len + 36;

    let mut out = Vec::with_capacity(44 + pcm_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&riff_size.to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // Subchunk1Size = 16 for PCM
    out.extend_from_slice(&1u16.to_le_bytes());  // AudioFormat = 1 (PCM)
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&block_align.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes()); // BitsPerSample = 16
    out.extend_from_slice(b"data");
    out.extend_from_slice(&pcm_len.to_le_bytes());
    for &s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

/// 將音訊位元組匯出至指定路徑，若副檔名為 wav 且來源非 wav 則自動轉碼為標準 WAV
pub fn export_audio_bytes(bytes: &[u8], target_path: &std::path::Path) -> Result<(), String> {
    let target_ext = target_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    let is_source_wav = bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WAVE";

    if target_ext == "mp3" && is_source_wav {
        return Err("目前不支援將 WAV 編碼為 MP3，請另存為 .wav。".to_string());
    }
    if !matches!(target_ext.as_str(), "mp3" | "wav") {
        return Err("請使用 .mp3 或 .wav 副檔名。".to_string());
    }
    if target_ext == "wav" && !is_source_wav {
        let (samples, sample_rate, channels) = decode_to_pcm(bytes)?;
        let wav = encode_pcm_to_wav(&samples, sample_rate, channels);
        std::fs::write(target_path, wav).map_err(|e| format!("寫入 WAV 失敗: {}", e))?;
    } else {
        std::fs::write(target_path, bytes).map_err(|e| format!("寫入音訊檔案失敗: {}", e))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_without_autoplay_keeps_audio_available_for_export() {
        let bytes = encode_pcm_to_wav(&vec![0; 8000], 8000, 1);
        let mut player = AudioPlayer::new_headless();
        player.load_bytes(bytes.clone());
        assert_eq!(player.current_bytes(), Some(&bytes));
        assert!(!player.is_playing());
        assert_eq!(player.elapsed(), Duration::ZERO);
        assert!(player.total_duration().is_some());
        let replacement = encode_pcm_to_wav(&[1, 2, 3], 8000, 1);
        player.load_bytes(replacement.clone());
        assert_eq!(player.current_bytes(), Some(&replacement));
    }

    #[test]
    fn wav_export_refuses_misleading_mp3_extension() {
        let bytes = encode_pcm_to_wav(&[0; 8], 8000, 1);
        let target = std::env::temp_dir().join(format!("fish-reject-{}.mp3", std::process::id()));
        let result = export_audio_bytes(&bytes, &target);
        assert!(result.unwrap_err().contains("不支援"));
        assert!(!target.exists());
    }

    #[test]
    fn test_audio_player_initialization() {
        let mut player = AudioPlayer::new_headless();
        assert_eq!(player.get_volume(), 1.0);
        player.set_volume(0.5);
        assert_eq!(player.get_volume(), 0.5);
        assert!(!player.is_playing());
        assert!(!player.is_paused());
        assert_eq!(player.elapsed(), Duration::ZERO);
    }

    #[test]
    fn test_estimate_audio_duration_wav() {
        // 構造一個合法的最小 44-byte WAV header: 1 秒單聲道 16-bit 44100Hz = 88200 bytes
        let mut wav_bytes = vec![0u8; 44 + 88200];
        wav_bytes[0..4].copy_from_slice(b"RIFF");
        wav_bytes[8..12].copy_from_slice(b"WAVE");
        // byte_rate at offset 28 = 88200
        let byte_rate: u32 = 88200;
        wav_bytes[28..32].copy_from_slice(&byte_rate.to_le_bytes());

        let dur = estimate_audio_duration(&wav_bytes).expect("WAV 時長應成功估算");
        assert!((dur.as_secs_f32() - 1.0).abs() < 0.05);
    }

    #[test]
    fn test_estimate_audio_duration_short_bytes() {
        let short = vec![1, 2, 3];
        assert_eq!(estimate_audio_duration(&short), None);
    }

    #[test]
    fn test_pcm_to_wav_encoding_and_decoding() {
        let dummy_samples: Vec<i16> = vec![0, 100, -100, 200, -200, 300, -300];
        let wav = encode_pcm_to_wav(&dummy_samples, 44100, 1);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(wav.len(), 44 + dummy_samples.len() * 2);

        let (decoded, sr, ch) = decode_to_pcm(&wav).expect("Decode WAV 應成功");
        assert_eq!(sr, 44100);
        assert_eq!(ch, 1);
        assert_eq!(decoded, dummy_samples);
    }
}
