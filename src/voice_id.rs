//! Manual Voice ID validation, selection and the single-speaker GUI editor.
//! Saving/clearing settings is local only; no provider request is made here.
use crate::config::AppConfig;

pub const MAX_VOICE_ID_BYTES: usize = 512;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoiceSource {
    Manual,
    Character,
    Provider,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VoiceSelection {
    pub id: Option<String>,
    pub source: VoiceSource,
}

fn looks_like_api_key(value: &str) -> bool {
    let value = value.trim();
    value.get(..3).is_some_and(|s| s.eq_ignore_ascii_case("sk-"))
        || value.get(..7).is_some_and(|s| s.eq_ignore_ascii_case("bearer "))
}

/// A local typo guard, NOT verification that a voice exists or is authorized.
/// Do not require a fixed hexadecimal length: provider voice IDs can differ.
pub fn normalize_voice_id(value: &str) -> Result<Option<String>, String> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    if looks_like_api_key(value) {
        return Err("這看起來是 API Key，請填在左側連線設定；此處只接受 Voice ID。".into());
    }
    if value.len() > MAX_VOICE_ID_BYTES {
        return Err("Voice ID 過長：上限為 512 bytes，請只貼上聲音識別碼。".into());
    }
    if value.contains("://") || value.to_ascii_lowercase().starts_with("www.") {
        return Err("請貼上 Voice ID 本身，不是聲音頁面網址。".into());
    }
    if value.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("Voice ID 中間不可包含空白、換行或控制字元。".into());
    }
    Ok(Some(value.to_owned()))
}

/// Preserve the existing single-speaker priority: manual > character > omitted.
pub fn resolve_voice_id(manual: &str, character: Option<&str>) -> Result<VoiceSelection, String> {
    if let Some(id) = normalize_voice_id(manual)? {
        return Ok(VoiceSelection { id: Some(id), source: VoiceSource::Manual });
    }
    if let Some(id) = normalize_voice_id(character.unwrap_or_default())? {
        return Ok(VoiceSelection { id: Some(id), source: VoiceSource::Character });
    }
    Ok(VoiceSelection { id: None, source: VoiceSource::Provider })
}

/// Commit in-memory state only AFTER persistence succeeds. The injected writer
/// keeps tests independent of the user's real config path and credentials.
pub fn persist_manual_voice_id(
    config: &mut AppConfig,
    value: &str,
    save: impl FnOnce(&AppConfig) -> Result<(), String>,
) -> Result<String, String> {
    let normalized = normalize_voice_id(value)?.unwrap_or_default();
    let mut updated = config.clone();
    updated.custom_voice_id = normalized.clone();
    save(&updated)?;
    config.custom_voice_id = normalized.clone();
    Ok(normalized)
}

/// Return a notice for the application's existing success/error banners.
#[cfg(feature = "gui")]
pub fn render_editor(
    ui: &mut egui::Ui,
    draft: &mut String,
    config: &mut AppConfig,
    character_voice: Option<&str>,
) -> Option<Result<String, String>> {
    ui.strong("手動設定 Voice ID");
    ui.label("直接輸入或貼上 Fish Audio 的聲音識別碼，不需修改設定檔。");
    let mask = looks_like_api_key(draft);
    ui.add(
        egui::TextEdit::singleline(draft)
            .id_salt("single_manual_voice_id")
            .password(mask)
            .desired_width(f32::INFINITY)
            .hint_text("貼上 Voice ID（不是網址或 API Key）"),
    );

    let valid = normalize_voice_id(draft).is_ok();
    let mut action = None;
    ui.horizontal_wrapped(|ui| {
        if ui.add_enabled(valid, egui::Button::new("儲存 Voice ID")).clicked() {
            action = Some(false);
        }
        if ui.add_enabled(
            !draft.is_empty() || !config.custom_voice_id.is_empty(),
            egui::Button::new("清除並儲存"),
        ).clicked() {
            action = Some(true);
        }
    });

    let notice = action.map(|clear| {
        let requested = if clear { "" } else { draft.as_str() };
        match persist_manual_voice_id(config, requested, AppConfig::save) {
            Ok(saved) => {
                *draft = saved;
                Ok(if draft.is_empty() {
                    "已清除並儲存手動 Voice ID；下次生成不再使用手動覆寫。".into()
                } else {
                    "Voice ID 已儲存；下次啟動會自動載入。".into()
                })
            }
            Err(error) => Err(format!("Voice ID 未儲存：{error}")),
        }
    });

    match resolve_voice_id(draft, character_voice) {
        Ok(selection) => {
            let source = match selection.source {
                VoiceSource::Manual => "下一次生成：使用手動 Voice ID（優先於角色預設）",
                VoiceSource::Character => "下一次生成：使用角色預設 Voice ID",
                VoiceSource::Provider => "下一次生成：不指定 Voice ID；部分供應商可能要求填寫",
            };
            ui.label(source);
            if let Some(id) = selection.id {
                ui.add(egui::Label::new(egui::RichText::new(id).monospace()).wrap());
            }
        }
        Err(error) => {
            ui.colored_label(ui.visuals().error_fg_color, error);
            ui.weak("請修正後再生成；不會將無效的手動 ID 回退成其他聲音。");
        }
    }
    if draft.as_str() != config.custom_voice_id.as_str() {
        ui.weak("輸入尚未儲存。有效 ID 會套用到下一次生成；按儲存可保留到下次啟動。");
    }
    ui.weak("只影響單人配音；多角色劇本與時間軸使用各自的 Voice ID。");
    ui.weak("儲存／清除不會呼叫 API。請使用已獲授權的聲音；ID 是否可用仍由供應商判定。");
    notice
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::SpeechRequest;

    #[test]
    fn trims_outer_whitespace_and_preserves_identifier_case() {
        assert_eq!(normalize_voice_id(" \tVoice_AbC-123\r\n").unwrap().as_deref(), Some("Voice_AbC-123"));
        assert_eq!(normalize_voice_id(" \t\r\n ").unwrap(), None);
        assert!(normalize_voice_id(&"a".repeat(MAX_VOICE_ID_BYTES)).is_ok());
        assert!(normalize_voice_id(&"a".repeat(MAX_VOICE_ID_BYTES + 1)).is_err());
    }

    #[test]
    fn rejects_urls_whitespace_and_credentials_without_echoing_them() {
        for input in ["https://fish.audio/m/example", "HTTP://fish.audio/m/example", "www.fish.audio",
            "voice id", "voice\nid", "voice\u{0}id", "sk-or-v1-do-not-echo", "Bearer do-not-echo"] {
            let error = normalize_voice_id(input).unwrap_err();
            assert!(!error.contains(input));
            assert!(!error.contains("do-not-echo"));
        }
    }

    #[test]
    fn manual_voice_overrides_character_without_rewriting_tags() {
        let selected = resolve_voice_id(" manual-voice ", Some("character-voice")).unwrap();
        assert_eq!(selected.source, VoiceSource::Manual);
        assert_eq!(selected.id.as_deref(), Some("manual-voice"));
        let req = SpeechRequest {
            model: "test-model".into(), input: "[happy] 保留原本的情緒標籤".into(),
            voice: selected.id, response_format: Some("mp3".into()), speed: Some(1.0),
        };
        let json = serde_json::to_value(req).unwrap();
        assert_eq!(json["voice"], "manual-voice");
        assert_eq!(json["input"], "[happy] 保留原本的情緒標籤");
    }

    #[test]
    fn clearing_falls_back_to_character_then_omits_voice() {
        let selected = resolve_voice_id(" ", Some(" preset ")).unwrap();
        assert_eq!(selected.id.as_deref(), Some("preset"));
        assert_eq!(selected.source, VoiceSource::Character);
        assert_eq!(resolve_voice_id("", None).unwrap(), VoiceSelection { id: None, source: VoiceSource::Provider });
        assert_eq!(resolve_voice_id("", Some("  ")).unwrap().id, None);
    }

    #[test]
    fn invalid_manual_voice_never_silently_uses_character_fallback() {
        assert!(resolve_voice_id("https://fish.audio/m/voice", Some("valid-preset")).is_err());
    }

    #[test]
    fn save_round_trips_existing_config_without_losing_other_preferences() {
        let mut config = AppConfig::default();
        config.custom_voice_id = "old".into();
        config.speed = 1.25;
        config.remember_api_key = false;
        let mut saved_json = String::new();
        let saved = persist_manual_voice_id(&mut config, " new-voice ", |updated| {
            saved_json = serde_json::to_string(updated).unwrap();
            Ok(())
        }).unwrap();
        let reopened: AppConfig = serde_json::from_str(&saved_json).unwrap();
        assert_eq!(saved, "new-voice");
        assert_eq!(config.custom_voice_id, reopened.custom_voice_id);
        assert_eq!(reopened.custom_voice_id, "new-voice");
        assert_eq!(reopened.speed, 1.25);
        assert!(!reopened.remember_api_key);
        assert_eq!(reopened.model, config.model);
    }

    #[test]
    fn clear_persists_an_empty_override() {
        let mut config = AppConfig::default();
        config.custom_voice_id = "old-voice".into();
        let result = persist_manual_voice_id(&mut config, "", |updated| {
            assert!(updated.custom_voice_id.is_empty());
            Ok(())
        }).unwrap();
        assert!(result.is_empty());
        assert!(config.custom_voice_id.is_empty());
    }

    #[test]
    fn failed_save_preserves_previous_config_and_reports_failure() {
        let mut config = AppConfig::default();
        config.custom_voice_id = "previous".into();
        let error = persist_manual_voice_id(&mut config, "new-voice", |_| Err("permission denied".into())).unwrap_err();
        assert_eq!(error, "permission denied");
        assert_eq!(config.custom_voice_id, "previous");
    }

    #[test]
    fn invalid_input_never_reaches_the_settings_writer() {
        let mut config = AppConfig::default();
        assert!(persist_manual_voice_id(&mut config, "sk-or-v1-do-not-save", |_| panic!("must not save")).is_err());
        assert!(config.custom_voice_id.is_empty());
    }

    #[cfg(feature = "gui")]
    #[test]
    fn editor_renders_without_a_window_audio_device_or_network() {
        let context = egui::Context::default();
        let mut draft = "my-voice".to_owned();
        let mut config = AppConfig::default();
        let output = context.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                assert!(render_editor(ui, &mut draft, &mut config, Some("preset")).is_none());
            });
        });
        assert!(!output.shapes.is_empty());
        assert_eq!(draft, "my-voice");
        assert!(config.custom_voice_id.is_empty());
    }
}
