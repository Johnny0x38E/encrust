use eframe::egui::IconData;
use eframe::egui::{FontData, FontDefinitions, FontFamily, FontTweak};
use std::sync::Arc;

mod app;
mod crypto;
mod io;

fn main() -> eframe::Result<()> {
    let viewport = eframe::egui::ViewportBuilder::default()
        // 当前设计按固定桌面工具窗口实现，避免不同窗口尺寸下左侧栏、
        // 主内容卡片和顶部导航出现未设计过的响应式状态。
        .with_inner_size([900.0, 680.0])
        .with_min_inner_size([900.0, 680.0])
        .with_max_inner_size([900.0, 680.0])
        .with_resizable(false)
        .with_icon(load_icon());

    let options = eframe::NativeOptions {
        viewport,
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

fn load_icon() -> Arc<IconData> {
    #[cfg(target_os = "macos")]
    let image = image::load_from_memory(include_bytes!(
        "../assets/appicon/icon.iconset/icon_32x32@2x.png"
    ))
    .expect("加载 macOS 运行时图标失败");

    #[cfg(not(target_os = "macos"))]
    let image =
        image::load_from_memory(include_bytes!("../assets/appicon.png")).expect("加载图标失败");

    let image = image.into_rgba8();
    let (width, height) = image.dimensions();
    Arc::new(IconData {
        rgba: image.into_raw(),
        width,
        height,
    })
}

fn configure_fonts(ctx: &eframe::egui::Context) {
    let mut fonts = FontDefinitions::default();

    // egui 默认字体的中文覆盖不稳定，所以按平台常见路径寻找 CJK 字体。
    // 找到第一份可用字体即可插到 Proportional/Monospace 的最前面作为优先字体。
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
            Arc::new(FontData::from_owned(font_bytes).tweak(FontTweak {
                // CJK 字体 ascent 较大，视觉上偏上；
                // 通过向下偏移让文字在按钮、输入框中更接近垂直居中。
                y_offset_factor: 0.18,
                ..Default::default()
            })),
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
