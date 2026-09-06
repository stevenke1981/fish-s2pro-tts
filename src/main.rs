use eframe::egui;
use fish_s2pro_tts::app::FishTtsApp;
use fish_s2pro_tts::config::AppConfig;
use fish_s2pro_tts::fonts;

fn main() -> eframe::Result<()> {
    // 預先載入配置以決定主題
    let config = AppConfig::load();

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1120.0, 780.0])
            .with_min_inner_size([860.0, 600.0])
            .with_title("Fish Audio S2.1 Pro TTS Studio (OpenRouter)"),
        ..Default::default()
    };

    eframe::run_native(
        "Fish Audio S2.1 Pro TTS Studio",
        native_options,
        Box::new(move |cc| {
            // 1. 設定微軟正黑體/中文字型渲染
            fonts::configure_cjk_fonts(&cc.egui_ctx);

            // 2. 套用初始視覺主題
            if config.dark_mode {
                cc.egui_ctx.set_visuals(egui::Visuals::dark());
            } else {
                cc.egui_ctx.set_visuals(egui::Visuals::light());
            }

            // 3. 微調邊距與樣式
            cc.egui_ctx.style_mut(|style| {
                style.spacing.item_spacing = egui::Vec2::new(8.0, 8.0);
                style.spacing.button_padding = egui::Vec2::new(10.0, 6.0);
            });

            Ok(Box::new(FishTtsApp::new(cc)))
        }),
    )
}
