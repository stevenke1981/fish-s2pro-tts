use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

const CONFIG_FILE_NAME: &str = "fish_tts_config.json";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppConfig {
    pub api_key: String,
    pub remember_api_key: bool,
    pub model: String,
    #[serde(default)]
    pub custom_model: String,
    pub selected_character_index: usize,
    #[serde(default = "default_auto_apply_character_tag")]
    pub auto_apply_character_tag: bool,
    #[serde(default = "default_tag_insert_mode")]
    pub tag_insert_mode: String,
    pub custom_voice_id: String,
    pub response_format: String,
    pub speed: f32,
    pub volume: f32,
    pub auto_play: bool,
    pub dark_mode: bool,
    pub custom_tones: Vec<String>,
}

fn default_auto_apply_character_tag() -> bool {
    true
}

fn default_tag_insert_mode() -> String {
    "append".to_string()
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            remember_api_key: true,
            model: "fish-audio/s2.1-pro-free:free".to_string(),
            custom_model: String::new(),
            selected_character_index: 0,
            auto_apply_character_tag: true,
            tag_insert_mode: "append".to_string(),
            custom_voice_id: String::new(),
            response_format: "mp3".to_string(),
            speed: 1.0,
            volume: 0.8,
            auto_play: true,
            dark_mode: true,
            custom_tones: vec![
                "[溫柔輕聲]".to_string(),
                "[冷靜理性]".to_string(),
                "[廣播主持腔]".to_string(),
                "[說故事口吻]".to_string(),
            ],
        }
    }
}

impl AppConfig {
    /// 取得設定檔儲存路徑（優先以目前工作目錄或執行檔目錄為依歸）
    pub fn config_path() -> PathBuf {
        let local_path = PathBuf::from(CONFIG_FILE_NAME);
        if local_path.exists() {
            return local_path;
        }

        if let Ok(exe_path) = std::env::current_exe()
            && let Some(parent) = exe_path.parent()
        {
            let candidate = parent.join(CONFIG_FILE_NAME);
            if candidate.exists() {
                return candidate;
            }
        }

        local_path
    }

    /// 讀取設定檔，若不存在則回傳預設值
    pub fn load() -> Self {
        let path = Self::config_path();
        if path.exists()
            && let Ok(content) = fs::read_to_string(&path)
            && let Ok(config) = serde_json::from_str::<AppConfig>(&content)
        {
            return config;
        }
        Self::default()
    }

    /// 儲存設定至磁碟
    pub fn save(&self) -> Result<(), String> {
        let path = Self::config_path();
        // 若使用者選擇不記憶 API Key，則在儲存時清空 key 欄位
        let to_save = if self.remember_api_key {
            self.clone()
        } else {
            let mut clone = self.clone();
            clone.api_key.clear();
            clone
        };

        let json = serde_json::to_string_pretty(&to_save)
            .map_err(|e| format!("序列化設定檔失敗: {}", e))?;

        fs::write(&path, json).map_err(|e| format!("寫入設定檔失敗: {}", e))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default_and_serde() {
        let config = AppConfig::default();
        assert_eq!(config.model, "fish-audio/s2.1-pro-free:free");
        assert_eq!(config.speed, 1.0);
        assert!(config.auto_apply_character_tag);
        assert_eq!(config.tag_insert_mode, "append");
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: AppConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.model, config.model);
        assert_eq!(deserialized.auto_apply_character_tag, config.auto_apply_character_tag);
    }
}
