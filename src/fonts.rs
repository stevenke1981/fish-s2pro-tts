use egui::{FontData, FontDefinitions, FontFamily};
use std::fs;

/// 設定中文字型與通用字型載入
pub fn configure_cjk_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();

    let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".to_string());
    let win_font_dir = std::path::PathBuf::from(windir).join("Fonts");

    // 系統字型候選路徑 (Windows 正黑體/雅黑體優先，含 Linux/macOS 備用)
    let candidate_paths = [
        win_font_dir.join("msjh.ttc"),    // 微軟正黑體 (繁體中文優先)
        win_font_dir.join("msjhl.ttc"),   // 微軟正黑體 Light
        win_font_dir.join("msjhbd.ttc"),  // 微軟正黑體 Bold
        win_font_dir.join("msyh.ttc"),    // 微軟雅黑體
        win_font_dir.join("msyhl.ttc"),   // 微軟雅黑體 Light
        win_font_dir.join("simsun.ttc"),   // 宋體
        std::path::PathBuf::from("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc"),
        std::path::PathBuf::from("/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc"),
        std::path::PathBuf::from("/System/Library/Fonts/PingFang.ttc"),
    ];

    let mut loaded_font_name = None;

    for path in candidate_paths {
        if let Ok(bytes) = fs::read(&path) {
            let font_name = "cjk_app_font".to_string();
            fonts
                .font_data
                .insert(font_name.clone(), FontData::from_owned(bytes).into());
            loaded_font_name = Some(font_name);
            break;
        }
    }

    if let Some(font_name) = loaded_font_name {
        // 將中文字型排在 Proportional 與 Monospace 字型族的第一位，作為主要/Fallback 字型
        if let Some(prop) = fonts.families.get_mut(&FontFamily::Proportional) {
            prop.insert(0, font_name.clone());
        }
        if let Some(mono) = fonts.families.get_mut(&FontFamily::Monospace) {
            mono.insert(0, font_name);
        }
    }

    ctx.set_fonts(fonts);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_font_config_structure() {
        let fonts = FontDefinitions::default();
        assert!(fonts.families.contains_key(&FontFamily::Proportional));
    }
}
