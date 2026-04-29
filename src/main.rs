use std::sync::Arc;

use eframe::egui::{FontData, FontDefinitions, FontFamily};

mod app;
mod crypto;
mod io;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([900.0, 680.0])
            .with_min_inner_size([900.0, 680.0])
            .with_max_inner_size([900.0, 680.0])
            .with_resizable(false),
        ..Default::default()
    };

    eframe::run_native(
        "Encrust",
        options,
        Box::new(|creation_context| {
            configure_fonts(&creation_context.egui_ctx);
            Ok(Box::new(app::EncrustApp::default()))
        }),
    )
}

fn configure_fonts(ctx: &eframe::egui::Context) {
    let mut fonts = FontDefinitions::default();

    let font_candidates = [
        "/System/Library/Fonts/Hiragino Sans GB.ttc",
        "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
        "/System/Library/Fonts/STHeiti Medium.ttc",
        "/System/Library/Fonts/STHeiti Light.ttc",
        "/System/Library/Fonts/Supplemental/Songti.ttc",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
    ];

    if let Some(font_bytes) = font_candidates
        .iter()
        .find_map(|path| std::fs::read(path).ok())
    {
        fonts.font_data.insert(
            "cjk-fallback".to_owned(),
            Arc::new(FontData::from_owned(font_bytes)),
        );

        if let Some(family) = fonts.families.get_mut(&FontFamily::Proportional) {
            family.insert(0, "cjk-fallback".to_owned());
        }
        if let Some(family) = fonts.families.get_mut(&FontFamily::Monospace) {
            family.insert(0, "cjk-fallback".to_owned());
        }
    }

    ctx.set_fonts(fonts);
}
