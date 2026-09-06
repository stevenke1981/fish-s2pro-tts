use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

pub const DEFAULT_MODEL: &str = "fish-audio/s2.1-pro-free:free";
pub const PRO_MODEL: &str = "fish-audio/s2.1-pro";
pub const DEFAULT_ENDPOINT: &str = "https://openrouter.ai/api/v1/audio/speech";
pub const KEY_AUTH_ENDPOINT: &str = "https://openrouter.ai/api/v1/auth/key";

/// 語音合成請求酬載
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SpeechRequest {
    pub model: String,
    pub input: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voice: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed: Option<f32>,
}

/// 金鑰驗證回傳資料
#[derive(Deserialize, Debug, Clone)]
pub struct KeyAuthResponse {
    pub data: Option<KeyAuthData>,
}

#[derive(Deserialize, Debug, Clone)]
#[allow(dead_code)]
pub struct KeyAuthData {
    pub label: Option<String>,
    pub usage: Option<f64>,
    pub limit: Option<f64>,
    pub is_free_tier: Option<bool>,
    pub rate_limit: Option<RateLimitInfo>,
}

#[derive(Deserialize, Debug, Clone)]
#[allow(dead_code)]
pub struct RateLimitInfo {
    pub requests: Option<i64>,
    pub interval: Option<String>,
}

/// API 錯誤回傳結構
#[derive(Deserialize, Debug)]
struct OpenRouterErrorResponse {
    error: Option<OpenRouterErrorDetail>,
}

#[derive(Deserialize, Debug)]
#[allow(dead_code)]
struct OpenRouterErrorDetail {
    message: Option<String>,
    code: Option<serde_json::Value>,
}

pub struct OpenRouterClient {
    client: Client,
}

impl Default for OpenRouterClient {
    fn default() -> Self {
        Self::new()
    }
}

impl OpenRouterClient {
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(90))
            .build()
            .unwrap_or_default();
        Self { client }
    }

    /// 驗證 OpenRouter API Key
    pub fn verify_key(&self, api_key: &str) -> Result<KeyAuthData, String> {
        let trimmed_key = api_key.trim();
        if trimmed_key.is_empty() {
            return Err("API Key 不能為空".to_string());
        }

        let resp = self
            .client
            .get(KEY_AUTH_ENDPOINT)
            .header("Authorization", format!("Bearer {}", trimmed_key))
            .header("HTTP-Referer", "https://github.com/fish-audio-openrouter-tts")
            .header("X-Title", "Fish Audio TTS Studio")
            .send()
            .map_err(|e| format!("網路連線失敗: {}", e))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().unwrap_or_default();
            if let Ok(err_obj) = serde_json::from_str::<OpenRouterErrorResponse>(&body)
                && let Some(detail) = err_obj.error
            {
                return Err(format!(
                    "認證失敗 (HTTP {}): {}",
                    status.as_u16(),
                    detail.message.unwrap_or_else(|| "未知錯誤".to_string())
                ));
            }
            return Err(format!("認證失敗 (HTTP {}): {}", status.as_u16(), body));
        }

        let auth_resp: KeyAuthResponse = resp
            .json()
            .map_err(|e| format!("解析驗證資訊失敗: {}", e))?;

        auth_resp
            .data
            .ok_or_else(|| "回傳資料缺少 data 欄位".to_string())
    }

    /// 呼叫語音合成 API 產生音訊位元組
    pub fn synthesize(&self, api_key: &str, req: &SpeechRequest) -> Result<Vec<u8>, String> {
        let trimmed_key = api_key.trim();
        if trimmed_key.is_empty() {
            return Err("請先輸入有效的 OpenRouter API Key".to_string());
        }

        if req.input.trim().is_empty() {
            return Err("合成文字不可為空".to_string());
        }

        let resp = self
            .client
            .post(DEFAULT_ENDPOINT)
            .header("Authorization", format!("Bearer {}", trimmed_key))
            .header("Content-Type", "application/json")
            .header("HTTP-Referer", "https://github.com/fish-audio-openrouter-tts")
            .header("X-Title", "Fish Audio TTS Studio")
            .json(req)
            .send()
            .map_err(|e| format!("發送請求失敗: {}", e))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().unwrap_or_default();
            if let Ok(err_obj) = serde_json::from_str::<OpenRouterErrorResponse>(&body)
                && let Some(detail) = err_obj.error
            {
                return Err(format!(
                    "生成失敗 (HTTP {}): {}",
                    status.as_u16(),
                    detail.message.unwrap_or_else(|| "服務端錯誤".to_string())
                ));
            }
            return Err(format!("生成失敗 (HTTP {}): {}", status.as_u16(), body));
        }

        let is_json = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .map(|ct| ct.contains("application/json"))
            .unwrap_or(false);

        let bytes = resp
            .bytes()
            .map_err(|e| format!("接收音訊資料流失敗: {}", e))?
            .to_vec();

        if bytes.is_empty() {
            return Err("伺服器回傳空的音訊資料".to_string());
        }

        // 若回傳為 JSON 格式或含有 error 欄位，嘗試解析為 OpenRouter 錯誤
        if (is_json || bytes.starts_with(b"{\"error") || bytes.starts_with(b"{\n\"error"))
            && let Ok(err_obj) = serde_json::from_slice::<OpenRouterErrorResponse>(&bytes)
            && let Some(msg) = err_obj.error.and_then(|d| d.message)
        {
            return Err(format!("OpenRouter 錯誤: {}", msg));
        }

        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_speech_request_serialization() {
        let req = SpeechRequest {
            model: DEFAULT_MODEL.to_string(),
            input: "[happy] 測試文字".to_string(),
            voice: Some("custom_voice".to_string()),
            response_format: Some("mp3".to_string()),
            speed: Some(1.0),
        };
        let json = serde_json::to_string(&req).expect("Failed to serialize request");
        assert!(json.contains("fish-audio/s2.1-pro-free:free"));
        assert!(json.contains("[happy] 測試文字"));
        assert!(json.contains("custom_voice"));
    }

    #[test]
    fn test_client_empty_key_validation() {
        let client = OpenRouterClient::new();
        let err = client.verify_key("").unwrap_err();
        assert!(err.contains("不能為空"));
    }
}
