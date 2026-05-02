use std::path::PathBuf;
use std::time::{Duration, Instant};

use eframe::egui;
use rfd::FileDialog;

use crate::crypto::{self, ContentKind, DecryptedPayload, EncryptedFileMetadata, EncryptionSuite};
use crate::io;

// ---------------------------------------------------------------------------
// 样式常量
// ---------------------------------------------------------------------------
//
// 视觉数值集中放在这里，避免散落在各个 render 函数里。调整 UI 时优先改这些
// 常量；具体组件 helper 只负责把常量应用到 egui 控件上。

// 浅色主色调（indigo）。主按钮、选中态、焦点框都从这里取色。
const PRIMARY: egui::Color32 = egui::Color32::from_rgb(79, 70, 229);
const SUCCESS: egui::Color32 = egui::Color32::from_rgb(16, 185, 129);
const SUCCESS_BG: egui::Color32 = egui::Color32::from_rgb(236, 253, 245);
const ERROR: egui::Color32 = egui::Color32::from_rgb(239, 68, 68);
const ERROR_BG: egui::Color32 = egui::Color32::from_rgb(254, 242, 242);

// 深色模式下同一套语义色。保持语义一致，只替换适合暗色背景的明度。
const DARK_PRIMARY: egui::Color32 = egui::Color32::from_rgb(99, 102, 241);
const DARK_SUCCESS: egui::Color32 = egui::Color32::from_rgb(52, 211, 153);
const DARK_SUCCESS_BG: egui::Color32 = egui::Color32::from_rgb(20, 50, 40);
const DARK_ERROR: egui::Color32 = egui::Color32::from_rgb(248, 113, 113);
const DARK_ERROR_BG: egui::Color32 = egui::Color32::from_rgb(50, 25, 25);

// 顶部导航栏高度。横向分隔线会复用这个值，保证和左侧竖线起点对齐。
const TOP_BAR_HEIGHT: f32 = 52.0;

// 操作按钮固定尺寸。egui 默认会按内容撑开按钮，这里固定尺寸能避免中英文文案
// 或禁用态导致布局轻微跳动。
const PRIMARY_BUTTON_SIZE: [f32; 2] = [140.0, 42.0];
const SECONDARY_BUTTON_SIZE: [f32; 2] = [130.0, 34.0];
const SAVE_AS_BUTTON_SIZE: [f32; 2] = [90.0, 34.0];
const CLEAR_BUTTON_SIZE: f32 = 14.0;
const SELECTED_PATH_ROW_HEIGHT: f32 = 24.0;
const CARD_PADDING: f32 = 16.0;

// 左侧栏宽度和卡片宽度分开计算：
// - SidePanel 自带 8px 左右内边距；
// - `SIDEBAR_CONTENT_LEFT_INSET` 抵消这 8px，让卡片实际距离窗口左边是 24px；
// - 卡片宽度按 300 - 24 * 2 计算，左右留白和靠分隔线一侧保持一致。
const SIDEBAR_WIDTH: f32 = 300.0;
const SIDEBAR_PADDING: f32 = 24.0;
const SIDEBAR_FRAME_INNER_X: f32 = 8.0;
const SIDEBAR_CARD_WIDTH: f32 = SIDEBAR_WIDTH - SIDEBAR_PADDING * 2.0;
const SIDEBAR_CONTENT_LEFT_INSET: f32 = SIDEBAR_PADDING - SIDEBAR_FRAME_INNER_X;
const SIDEBAR_CARD_PADDING: i8 = 14;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OperationMode {
    Encrypt,
    Decrypt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EncryptInputMode {
    File,
    Text,
}

#[derive(Debug, Clone)]
enum Notice {
    Success(String),
    Error(String),
}

#[derive(Debug, Clone)]
struct Toast {
    notice: Notice,
    created_at: Instant,
}

pub struct EncrustApp {
    operation_mode: OperationMode,
    encrypt_input_mode: EncryptInputMode,
    selected_file: Option<PathBuf>,
    text_input: String,
    passphrase: String,
    selected_suite: EncryptionSuite,
    encrypted_output_path: Option<PathBuf>,
    encrypted_input_path: Option<PathBuf>,
    decrypted_text: String,
    decrypted_file_bytes: Option<Vec<u8>>,
    decrypted_file_name: Option<String>,
    decrypted_output_path: Option<PathBuf>,
    toast: Option<Toast>,
    drag_hovered: bool,
}

impl Default for EncrustApp {
    fn default() -> Self {
        Self {
            operation_mode: OperationMode::Encrypt,
            encrypt_input_mode: EncryptInputMode::File,
            selected_file: None,
            text_input: String::new(),
            passphrase: String::new(),
            selected_suite: EncryptionSuite::Aes256Gcm,
            encrypted_output_path: None,
            encrypted_input_path: None,
            decrypted_text: String::new(),
            decrypted_file_bytes: None,
            decrypted_file_name: None,
            decrypted_output_path: None,
            toast: None,
            drag_hovered: false,
        }
    }
}

impl eframe::App for EncrustApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        apply_app_style(ctx);
        self.capture_dropped_files(ctx);

        // 顶部导航栏
        egui::TopBottomPanel::top("menu_bar")
            .resizable(false)
            .show_separator_line(false)
            .exact_height(TOP_BAR_HEIGHT)
            .frame({
                let colors = theme_colors(ctx);
                let mut frame = egui::Frame::new().fill(colors.app_bg);
                frame.inner_margin = egui::Margin::symmetric(16, 0);
                frame
            })
            .show(ctx, |ui| {
                let colors = theme_colors(ctx);
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;

                    // Logo / 标题
                    ui.label(egui::RichText::new("🔐").size(18.0));
                    ui.label(
                        egui::RichText::new("Encrust")
                            .size(16.0)
                            .color(colors.text_main)
                            .strong(),
                    );

                    ui.add_space(24.0);

                    self.render_operation_tabs(ui);
                });
            });

        // 左侧边栏
        egui::SidePanel::left("settings")
            .resizable(false)
            .exact_width(SIDEBAR_WIDTH)
            // 保留 egui 原生 SidePanel frame，使用它自带的 1px 分隔线。
            // 这里只移除外框 stroke，避免额外画出第二条竖线。
            .frame(egui::Frame::side_top_panel(&ctx.style()).stroke(egui::Stroke::NONE))
            .show(ctx, |ui| {
                ui.add_space(16.0);
                ui.horizontal(|ui| {
                    ui.add_space(SIDEBAR_CONTENT_LEFT_INSET);
                    ui.vertical(|ui| {
                        ui.set_width(SIDEBAR_CARD_WIDTH);
                        self.render_sidebar(ui);
                    });
                });
            });

        // 主内容区（外层包裹 ScrollArea，防止内容超出窗口边界）
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(16.0);
            // 主内容最大宽度固定在 720px，窗口尺寸固定时能维持清爽的阅读宽度；
            // 小宽度下仍保留 320px 下限，避免控件被压到不可用。
            let horizontal_padding = 20.0;
            let content_width = (ui.available_width() - horizontal_padding * 2.0)
                .max(320.0)
                .min(720.0);

            match self.operation_mode {
                OperationMode::Encrypt => {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false; 2])
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.add_space(horizontal_padding);
                                ui.vertical(|ui| {
                                    ui.set_width(content_width);
                                    self.render_encrypt_view(ui);
                                });
                            });
                        });
                }
                OperationMode::Decrypt => {
                    let content_height = ui.available_height();
                    ui.horizontal(|ui| {
                        ui.add_space(horizontal_padding);
                        ui.allocate_ui_with_layout(
                            egui::vec2(content_width, content_height),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| {
                                ui.set_width(content_width);
                                self.render_decrypt_view(ui);
                            },
                        );
                    });
                }
            }
        });

        draw_top_panel_separator(ctx);
        self.render_toast(ctx);
    }
}

impl EncrustApp {
    fn capture_dropped_files(&mut self, ctx: &egui::Context) {
        let (dropped_files, hovered_files) = ctx.input(|input| {
            (
                input.raw.dropped_files.clone(),
                input.raw.hovered_files.clone(),
            )
        });

        self.drag_hovered = !hovered_files.is_empty();

        if let Some(path) = dropped_files.into_iter().find_map(|file| file.path) {
            self.drag_hovered = false;
            match self.operation_mode {
                OperationMode::Encrypt => {
                    self.encrypt_input_mode = EncryptInputMode::File;
                    self.selected_file = Some(path);
                    self.encrypted_output_path = None;
                }
                OperationMode::Decrypt => {
                    self.set_encrypted_input_path(path);
                }
            }
            self.toast = None;
        }
    }

    fn render_sidebar(&mut self, ui: &mut egui::Ui) {
        let colors = theme_colors(ui.ctx());

        ui.label(
            egui::RichText::new("选项")
                .size(13.0)
                .color(colors.text_muted)
                .strong(),
        );
        ui.add_space(12.0);

        if self.operation_mode == OperationMode::Encrypt {
            sidebar_card(ui, "输入类型", |ui| {
                self.render_encrypt_input_tabs(ui);
            });
            ui.add_space(16.0);

            sidebar_card(ui, "加密方式", |ui| {
                self.render_encryption_suite_picker(ui);
            });
            ui.add_space(16.0);
        } else {
            sidebar_card(ui, "输入", |ui| {
                ui.label(
                    egui::RichText::new("右侧选择或拖入 .encrust 文件")
                        .color(colors.text_muted)
                        .size(13.0),
                );
            });
            ui.add_space(16.0);
        }

        sidebar_card(ui, "密钥", |ui| {
            self.render_passphrase_input(ui);
        });
    }

    fn render_encryption_suite_picker(&mut self, ui: &mut egui::Ui) {
        let colors = theme_colors(ui.ctx());

        egui::ComboBox::from_id_salt("encryption_suite")
            .selected_text(self.selected_suite.display_name())
            .width(ui.available_width())
            .show_ui(ui, |ui| {
                for suite in EncryptionSuite::available_for_encryption() {
                    ui.selectable_value(&mut self.selected_suite, *suite, suite.display_name());
                }
            });

        ui.add_space(6.0);
        ui.label(
            egui::RichText::new("解密时会自动识别文件里的加密方式")
                .color(colors.text_muted)
                .size(12.0),
        );
    }

    fn render_operation_tabs(&mut self, ui: &mut egui::Ui) {
        let colors = theme_colors(ui.ctx());

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;

            for (mode, label) in [
                (OperationMode::Encrypt, "加密"),
                (OperationMode::Decrypt, "解密"),
            ] {
                // 顶部 tab 使用自绘文字和下划线，而不是 Button，方便精确控制
                // hover/active 下划线的长度、粗细和位置。
                let active = self.operation_mode == mode;
                let (rect, response) =
                    ui.allocate_exact_size(egui::vec2(76.0, 30.0), egui::Sense::click());

                ui.painter().text(
                    rect.center() - egui::vec2(0.0, 8.0),
                    egui::Align2::CENTER_CENTER,
                    label,
                    egui::FontId::proportional(14.0),
                    if active || response.hovered() {
                        colors.primary
                    } else {
                        colors.text_muted
                    },
                );

                let underline_color = if active {
                    colors.primary
                } else if response.hovered() {
                    colors.border_hover
                } else {
                    colors.border
                };
                let underline_height = if active { 3.0 } else { 1.0 };
                let underline_y = rect.bottom() - 5.0;
                let underline = egui::Rect::from_min_max(
                    egui::pos2(rect.left() + 18.0, underline_y - underline_height),
                    egui::pos2(rect.right() - 18.0, underline_y),
                );
                ui.painter()
                    .rect_filled(underline, underline_height, underline_color);

                if response.clicked() {
                    self.set_operation_mode(mode);
                }
            }
        });
    }

    fn set_operation_mode(&mut self, mode: OperationMode) {
        if self.operation_mode != mode {
            self.operation_mode = mode;
            self.toast = None;
        }
    }

    fn render_encrypt_view(&mut self, ui: &mut egui::Ui) {
        match self.encrypt_input_mode {
            EncryptInputMode::File => self.render_file_encrypt_input(ui),
            EncryptInputMode::Text => self.render_text_encrypt_input(ui),
        }

        ui.add_space(16.0);
        self.render_encrypted_output_picker(ui);
        ui.add_space(16.0);
        self.render_encrypt_action(ui);
        ui.add_space(20.0);
    }

    fn render_decrypt_view(&mut self, ui: &mut egui::Ui) {
        let colors = theme_colors(ui.ctx());
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("解密")
                    .size(20.0)
                    .color(colors.text_main)
                    .strong(),
            );
        });
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new("将 .encrust 文件拖拽到窗口中，或通过系统文件选择器手动选择")
                .color(colors.text_muted)
                .size(13.0),
        );
        ui.add_space(16.0);

        let has_encrypted_input = self.encrypted_input_path.is_some();
        // 选择文件后拖拽区收缩成单行路径，释放垂直空间给解密结果。
        let drop_area_height = if has_encrypted_input { 32.0 } else { 160.0 };
        let drop_stroke = if self.drag_hovered {
            egui::Stroke::new(2.0, colors.primary)
        } else {
            egui::Stroke::new(1.5, colors.border)
        };
        let drop_fill = if self.drag_hovered {
            colors.primary_soft
        } else {
            colors.surface
        };

        let drop_response = egui::Frame::new()
            .fill(drop_fill)
            .stroke(drop_stroke)
            .corner_radius(10)
            .inner_margin(if has_encrypted_input {
                egui::Margin::symmetric(16, 8)
            } else {
                egui::Margin::same(16)
            })
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.set_min_height(drop_area_height);
                ui.with_layout(
                    egui::Layout::top_down(egui::Align::Center)
                        .with_main_align(egui::Align::Center),
                    |ui| {
                        if let Some(path) = &self.encrypted_input_path {
                            let selected_path = path.display().to_string();
                            if selected_path_row(ui, "加密文件", &selected_path, colors) {
                                self.encrypted_input_path = None;
                                self.decrypted_text.clear();
                                self.decrypted_file_bytes = None;
                                self.decrypted_file_name = None;
                                self.decrypted_output_path = None;
                                self.toast = None;
                            }
                        } else {
                            let icon = if self.drag_hovered { "↓" } else { "🔒" };
                            ui.label(egui::RichText::new(icon).size(32.0).color(
                                if self.drag_hovered {
                                    colors.primary
                                } else {
                                    colors.text_muted
                                },
                            ));
                            ui.add_space(8.0);
                            ui.label(
                                egui::RichText::new(if self.drag_hovered {
                                    "释放以选择加密文件"
                                } else {
                                    "拖拽 .encrust 文件到此处"
                                })
                                .color(if self.drag_hovered {
                                    colors.primary
                                } else {
                                    colors.text_muted
                                })
                                .size(14.0)
                                .strong(),
                            );
                            ui.add_space(10.0);
                            if ui.add(secondary_button("或点击选择文件", colors)).clicked() {
                                if let Some(path) = FileDialog::new()
                                    .add_filter("Encrust 加密文件", &["encrust"])
                                    .pick_file()
                                {
                                    self.set_encrypted_input_path(path);
                                }
                            }
                        }
                    },
                );
            })
            .response;

        if self.encrypted_input_path.is_some() && drop_response.clicked() {
            if let Some(path) = FileDialog::new()
                .add_filter("Encrust 加密文件", &["encrust"])
                .pick_file()
            {
                self.set_encrypted_input_path(path);
            }
        }

        ui.add_space(12.0);
        self.render_decrypt_action(ui);
        self.render_decrypt_result(ui);
        ui.add_space(20.0);
    }

    fn render_encrypt_input_tabs(&mut self, ui: &mut egui::Ui) {
        let colors = theme_colors(ui.ctx());
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            for (mode, label) in [
                (EncryptInputMode::File, "文件"),
                (EncryptInputMode::Text, "文本"),
            ] {
                let active = self.encrypt_input_mode == mode;
                let text = egui::RichText::new(label)
                    .size(13.0)
                    .color(if active {
                        colors.text_main
                    } else {
                        colors.text_muted
                    })
                    .strong();
                let btn = egui::Button::new(text)
                    .fill(if active {
                        colors.surface_alt
                    } else {
                        egui::Color32::TRANSPARENT
                    })
                    .corner_radius(6)
                    .stroke(egui::Stroke::NONE)
                    .min_size([60.0, 30.0].into());
                if ui.add(btn).clicked() {
                    self.encrypt_input_mode = mode;
                    match mode {
                        EncryptInputMode::File | EncryptInputMode::Text => {
                            self.encrypted_output_path = None;
                        }
                    }
                }
            }
        });
    }

    fn render_file_encrypt_input(&mut self, ui: &mut egui::Ui) {
        let colors = theme_colors(ui.ctx());

        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("文件加密")
                    .size(20.0)
                    .color(colors.text_main)
                    .strong(),
            );
        });
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new("将文件拖拽到窗口中，或通过系统文件选择器手动选择")
                .color(colors.text_muted)
                .size(13.0),
        );

        let has_selected_file = self.selected_file.is_some();
        // 选择文件后拖拽区收缩成单行路径，避免输出路径卡片被挤到太下面。
        let drop_area_height = if has_selected_file { 20.0 } else { 150.0 };
        let drop_stroke = if self.drag_hovered {
            egui::Stroke::new(2.0, colors.primary)
        } else {
            egui::Stroke::new(1.5, colors.border)
        };
        let drop_fill = if self.drag_hovered {
            colors.primary_soft
        } else {
            colors.surface
        };

        let drop_response = egui::Frame::new()
            .fill(drop_fill)
            .stroke(drop_stroke)
            .corner_radius(10)
            .inner_margin(if has_selected_file {
                egui::Margin::symmetric(16, 8)
            } else {
                egui::Margin::same(16)
            })
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.set_min_height(drop_area_height);
                ui.with_layout(
                    egui::Layout::top_down(egui::Align::Center)
                        .with_main_align(egui::Align::Center),
                    |ui| {
                        if let Some(path) = &self.selected_file {
                            let selected_path = path.display().to_string();
                            if selected_path_row(ui, "文件", &selected_path, colors) {
                                self.selected_file = None;
                                self.encrypted_output_path = None;
                                self.toast = None;
                            }
                        } else {
                            let icon = if self.drag_hovered { "↓" } else { "📁" };
                            ui.label(egui::RichText::new(icon).size(32.0).color(
                                if self.drag_hovered {
                                    colors.primary
                                } else {
                                    colors.text_muted
                                },
                            ));
                            ui.add_space(8.0);
                            ui.label(
                                egui::RichText::new(if self.drag_hovered {
                                    "释放以选择文件"
                                } else {
                                    "拖拽文件到此处"
                                })
                                .color(if self.drag_hovered {
                                    colors.primary
                                } else {
                                    colors.text_muted
                                })
                                .size(14.0)
                                .strong(),
                            );
                            ui.add_space(10.0);
                            if ui.add(secondary_button("或点击选择文件", colors)).clicked() {
                                if let Some(path) = FileDialog::new().pick_file() {
                                    self.selected_file = Some(path);
                                    self.encrypted_output_path = None;
                                    self.toast = None;
                                }
                            }
                        }
                    },
                );
            })
            .response;

        if self.selected_file.is_some() && drop_response.clicked() {
            if let Some(path) = FileDialog::new().pick_file() {
                self.selected_file = Some(path);
                self.encrypted_output_path = None;
                self.toast = None;
            }
        }
    }

    fn render_text_encrypt_input(&mut self, ui: &mut egui::Ui) {
        let colors = theme_colors(ui.ctx());

        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("文本加密")
                    .size(20.0)
                    .color(colors.text_main)
                    .strong(),
            );
        });
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new("输入需要加密为 .encrust 文件的文本")
                .color(colors.text_muted)
                .size(13.0),
        );

        egui::Frame::new()
            .fill(colors.surface)
            .stroke(egui::Stroke::new(1.5, colors.border))
            .corner_radius(10)
            .inner_margin(egui::Margin::same(11))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                scrollable_text_edit(ui, &mut self.text_input, 160.0, "在这里输入文本...");
            });
    }

    fn render_passphrase_input(&mut self, ui: &mut egui::Ui) {
        let colors = theme_colors(ui.ctx());

        // 使用 TextEdit 自带 frame，通过 background_color 和 margin 控制外观，
        // 这样 hint text 会在内部 rect 中正确垂直居中。
        ui.add(
            egui::TextEdit::singleline(&mut self.passphrase)
                .password(true)
                .hint_text("至少 8 个字符")
                .margin(egui::Margin::symmetric(12, 11))
                .background_color(colors.surface_alt)
                .min_size(egui::vec2(ui.available_width(), 44.0))
                .desired_width(ui.available_width()),
        );

        if !self.passphrase.is_empty() {
            if let Err(err) = crypto::validate_passphrase(&self.passphrase) {
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(err.to_string())
                        .color(colors.error)
                        .size(12.0),
                );
            }
        }
    }

    fn render_encrypted_output_picker(&mut self, ui: &mut egui::Ui) {
        let colors = theme_colors(ui.ctx());

        egui::Frame::new()
            .fill(colors.surface)
            .stroke(egui::Stroke::new(1.5, colors.border))
            .corner_radius(10)
            .inner_margin(egui::Margin::same(16))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());

                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("输出路径")
                            .color(colors.text_main)
                            .strong(),
                    );
                });
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("加密前需要手动选择保存位置")
                        .color(colors.text_muted)
                        .size(12.0),
                );
                ui.add_space(12.0);

                let output_label = self
                    .encrypted_output_path
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| "未选择保存路径".to_owned());

                path_display(ui, "保存到", &output_label, colors);
                ui.add_space(8.0);

                if ui.add(save_as_button(colors)).clicked() {
                    let default_name = self.default_encrypted_output_file_name();
                    let dialog = FileDialog::new().set_file_name(default_name);
                    if let Some(path) = dialog.save_file() {
                        self.encrypted_output_path = Some(path);
                        self.toast = None;
                    }
                }
            });
    }

    fn render_encrypt_action(&mut self, ui: &mut egui::Ui) {
        let colors = theme_colors(ui.ctx());
        let can_encrypt = self.can_encrypt();

        let btn = primary_button("加密并保存", colors);
        let inner = ui.allocate_ui_with_layout(
            egui::vec2(PRIMARY_BUTTON_SIZE[0], PRIMARY_BUTTON_SIZE[1]),
            egui::Layout::left_to_right(egui::Align::Center).with_main_align(egui::Align::Center),
            |ui| ui.add_enabled(can_encrypt, btn),
        );
        if inner.inner.clicked() {
            self.encrypt_active_input();
        }
    }

    fn render_decrypt_action(&mut self, ui: &mut egui::Ui) {
        let colors = theme_colors(ui.ctx());
        let can_decrypt = self.encrypted_input_path.is_some()
            && crypto::validate_passphrase(&self.passphrase).is_ok();

        let btn = primary_button("解密", colors);
        let inner = ui.allocate_ui_with_layout(
            egui::vec2(PRIMARY_BUTTON_SIZE[0], PRIMARY_BUTTON_SIZE[1]),
            egui::Layout::left_to_right(egui::Align::Center).with_main_align(egui::Align::Center),
            |ui| ui.add_enabled(can_decrypt, btn),
        );
        if inner.inner.clicked() {
            self.decrypt_selected_file();
        }
    }

    fn render_decrypt_result(&mut self, ui: &mut egui::Ui) {
        if !self.decrypted_text.is_empty() {
            ui.add_space(12.0);
            let colors = theme_colors(ui.ctx());
            let remaining_height = (ui.max_rect().bottom() - ui.cursor().min.y - 16.0).max(0.0);
            let result_height = remaining_height.max(180.0);
            let (rect, _) = ui.allocate_exact_size(
                egui::vec2(ui.available_width(), result_height),
                egui::Sense::hover(),
            );
            ui.painter().rect(
                rect,
                10.0,
                colors.surface,
                egui::Stroke::new(1.5, colors.border),
                egui::StrokeKind::Outside,
            );

            let inner_rect = rect.shrink(CARD_PADDING);
            let mut result_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(inner_rect)
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            result_ui.set_width(inner_rect.width());
            result_ui.horizontal(|ui| {
                ui.label(egui::RichText::new("解密后的文本").strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(primary_button("复制文本", colors)).clicked() {
                        ui.ctx().copy_text(self.decrypted_text.clone());
                        self.clear_decrypt_inputs();
                        self.show_toast(Notice::Success("已复制解密后的文本".to_owned()));
                    }
                });
            });
            result_ui.add_space(2.0);
            let text_bottom = rect.bottom() - CARD_PADDING;
            let text_height = (text_bottom - result_ui.cursor().min.y).max(72.0);
            scrollable_text_edit(&mut result_ui, &mut self.decrypted_text, text_height, "");
        }

        if self.decrypted_file_bytes.is_some() {
            ui.add_space(10.0);
            let colors = theme_colors(ui.ctx());

            egui::Frame::new()
                .fill(colors.surface)
                .stroke(egui::Stroke::new(1.5, colors.border))
                .corner_radius(10)
                .inner_margin(egui::Margin::same(16))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(egui::RichText::new("解密后的文件").strong());
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new("解密内容已准备好，选择路径后保存")
                            .color(colors.text_muted)
                            .size(12.0),
                    );
                    ui.add_space(12.0);

                    if let Some(name) = &self.decrypted_file_name {
                        path_display(ui, "原文件名", name, colors);
                        ui.add_space(8.0);
                    }

                    let output_label = self
                        .decrypted_output_path
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "未选择保存路径".to_owned());
                    path_display(ui, "保存到", &output_label, colors);
                    ui.add_space(8.0);

                    if ui.add(save_as_button(colors)).clicked() {
                        let file_name = self
                            .decrypted_file_name
                            .clone()
                            .unwrap_or_else(|| "decrypted-output".to_owned());
                        if let Some(path) = FileDialog::new().set_file_name(&file_name).save_file()
                        {
                            self.decrypted_output_path = Some(path);
                            self.toast = None;
                        }
                    }

                    ui.add_space(12.0);
                    let can_save = self.decrypted_output_path.is_some();
                    let save_btn = primary_button("保存解密文件", colors);
                    let inner = ui.allocate_ui_with_layout(
                        egui::vec2(PRIMARY_BUTTON_SIZE[0], PRIMARY_BUTTON_SIZE[1]),
                        egui::Layout::left_to_right(egui::Align::Center)
                            .with_main_align(egui::Align::Center),
                        |ui| ui.add_enabled(can_save, save_btn),
                    );
                    if inner.inner.clicked() {
                        self.save_decrypted_file();
                    }
                });
        }
    }

    fn render_toast(&mut self, ctx: &egui::Context) {
        let Some(toast) = &self.toast else {
            return;
        };

        if toast.created_at.elapsed() > Duration::from_secs(4) {
            self.toast = None;
            return;
        }

        let colors = theme_colors(ctx);
        let (message, fill, stroke, text_color, status) = match &toast.notice {
            Notice::Success(message) => (
                message,
                colors.success_bg,
                colors.success,
                colors.success,
                "成功",
            ),
            Notice::Error(message) => {
                (message, colors.error_bg, colors.error, colors.error, "错误")
            }
        };

        let progress = 1.0 - (toast.created_at.elapsed().as_secs_f32() / 4.0).clamp(0.0, 1.0);

        egui::Area::new("toast".into())
            .anchor(egui::Align2::CENTER_TOP, [0.0, 60.0])
            .interactable(false)
            .show(ctx, |ui| {
                let response = notice_frame(fill, stroke).show(ui, |ui| {
                    ui.set_max_width(400.0);
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(status)
                                .size(13.0)
                                .color(text_color)
                                .strong(),
                        );
                        ui.add_space(8.0);
                        ui.label(egui::RichText::new(message).color(text_color).strong());
                    });
                });
                let rect = response.response.rect;
                let bar_width = rect.width() * progress;
                ui.painter().rect_filled(
                    egui::Rect::from_min_max(
                        egui::pos2(rect.left(), rect.bottom() - 2.5),
                        egui::pos2(rect.left() + bar_width, rect.bottom()),
                    ),
                    0.0,
                    text_color,
                );
            });

        ctx.request_repaint_after(Duration::from_millis(250));
    }

    fn show_toast(&mut self, notice: Notice) {
        self.toast = Some(Toast {
            notice,
            created_at: Instant::now(),
        });
    }

    fn can_encrypt(&self) -> bool {
        let has_valid_input = match self.encrypt_input_mode {
            EncryptInputMode::File => self.selected_file.is_some(),
            EncryptInputMode::Text => !self.text_input.trim().is_empty(),
        };

        has_valid_input
            && self.encrypted_output_path.is_some()
            && crypto::validate_passphrase(&self.passphrase).is_ok()
    }

    fn encrypt_active_input(&mut self) {
        let result = self
            .load_active_plaintext()
            .and_then(|(plaintext, kind, file_name)| {
                if self.selected_suite == EncryptionSuite::Aes256Gcm {
                    crypto::encrypt_bytes(&plaintext, &self.passphrase, kind, file_name.as_deref())
                        .map_err(|err| err.to_string())
                } else {
                    crypto::encrypt_bytes_with_suite(
                        &plaintext,
                        &self.passphrase,
                        kind,
                        file_name.as_deref(),
                        self.selected_suite,
                    )
                    .map_err(|err| err.to_string())
                }
            })
            .and_then(|encrypted| {
                let output_path = self
                    .encrypted_output_path
                    .clone()
                    .ok_or_else(|| "请选择保存路径".to_owned())?;
                io::write_file(&output_path, &encrypted)
                    .map_err(|err| format!("保存失败：{err}"))?;
                Ok(output_path)
            });

        let notice = match result {
            Ok(path) => {
                let message = format!("已保存加密文件：{}", path.display());
                self.clear_encrypt_inputs();
                Notice::Success(message)
            }
            Err(err) => Notice::Error(err),
        };
        self.show_toast(notice);
    }

    fn decrypt_selected_file(&mut self) {
        let result = self
            .encrypted_input_path
            .as_ref()
            .ok_or_else(|| "请选择要解密的 .encrust 文件".to_owned())
            .and_then(|path| {
                io::read_file(path)
                    .map(|bytes| (path.clone(), bytes))
                    .map_err(|err| format!("读取加密文件失败：{err}"))
            })
            .and_then(|(path, encrypted)| {
                let metadata =
                    crypto::inspect_encrypted_file(&encrypted).map_err(|err| err.to_string())?;
                crypto::decrypt_bytes(&encrypted, &self.passphrase)
                    .map(|payload| (path, payload, metadata))
                    .map_err(|err| err.to_string())
            });

        match result {
            Ok((path, payload, metadata)) => self.apply_decrypted_payload(path, payload, metadata),
            Err(err) => self.show_toast(Notice::Error(err)),
        }
    }

    fn save_decrypted_file(&mut self) {
        let result = self
            .decrypted_file_bytes
            .as_ref()
            .ok_or_else(|| "没有可保存的解密文件".to_owned())
            .and_then(|bytes| {
                let output_path = self
                    .decrypted_output_path
                    .clone()
                    .ok_or_else(|| "请选择解密文件保存路径".to_owned())?;
                io::write_file(&output_path, bytes)
                    .map_err(|err| format!("保存解密文件失败：{err}"))?;
                Ok(output_path)
            });

        let notice = match result {
            Ok(path) => {
                let message = format!("已保存解密文件：{}", path.display());
                self.clear_decrypt_inputs();
                Notice::Success(message)
            }
            Err(err) => Notice::Error(err),
        };
        self.show_toast(notice);
    }

    fn default_encrypted_output_file_name(&self) -> String {
        match self.encrypt_input_mode {
            EncryptInputMode::File => self
                .selected_file
                .as_ref()
                .and_then(|path| path.file_name())
                .and_then(|name| name.to_str())
                .map(|name| format!("{name}.encrust"))
                .unwrap_or_else(|| "encrypted.encrust".to_owned()),
            EncryptInputMode::Text => "encrypted-text.encrust".to_owned(),
        }
    }

    fn load_active_plaintext(&self) -> Result<(Vec<u8>, ContentKind, Option<String>), String> {
        match self.encrypt_input_mode {
            EncryptInputMode::File => {
                let path = self
                    .selected_file
                    .as_ref()
                    .ok_or_else(|| "请选择要加密的文件".to_owned())?;
                let bytes = io::read_file(path).map_err(|err| format!("读取文件失败：{err}"))?;
                let file_name = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(ToOwned::to_owned);
                Ok((bytes, ContentKind::File, file_name))
            }
            EncryptInputMode::Text => {
                if self.text_input.trim().is_empty() {
                    return Err("请输入要加密的文本".to_owned());
                }

                Ok((self.text_input.as_bytes().to_vec(), ContentKind::Text, None))
            }
        }
    }

    fn set_encrypted_input_path(&mut self, path: PathBuf) {
        self.encrypted_input_path = Some(path);
        self.decrypted_text.clear();
        self.decrypted_file_bytes = None;
        self.decrypted_file_name = None;
        self.decrypted_output_path = None;
        self.toast = None;
    }

    fn apply_decrypted_payload(
        &mut self,
        encrypted_path: PathBuf,
        payload: DecryptedPayload,
        metadata: EncryptedFileMetadata,
    ) {
        self.decrypted_text.clear();
        self.decrypted_file_bytes = None;
        self.decrypted_file_name = None;
        self.decrypted_output_path = None;

        let suite_label = metadata.suite.display_name();
        let version_label = format!("Encrust v{}", metadata.format_version);
        let metadata_kind = metadata.kind;

        match payload.kind {
            ContentKind::Text => match String::from_utf8(payload.plaintext) {
                Ok(text) => {
                    self.decrypted_text = text;
                    self.passphrase.clear();
                    let message = if metadata_kind == ContentKind::Text {
                        format!("文本解密成功（{version_label} / {suite_label}）")
                    } else {
                        "文本解密成功".to_owned()
                    };
                    self.show_toast(Notice::Success(message));
                }
                Err(_) => {
                    self.show_toast(Notice::Error(
                        "解密成功，但内容不是有效的 UTF-8 文本".to_owned(),
                    ));
                }
            },
            ContentKind::File => {
                self.decrypted_output_path = Some(io::default_decrypted_output_path(
                    &encrypted_path,
                    payload.file_name.as_deref(),
                ));
                self.decrypted_file_name = payload.file_name;
                self.decrypted_file_bytes = Some(payload.plaintext);
                self.passphrase.clear();
                let message = if metadata_kind == ContentKind::File {
                    format!("文件解密成功（{version_label} / {suite_label}），请选择保存位置")
                } else {
                    "文件解密成功，请选择保存位置".to_owned()
                };
                self.show_toast(Notice::Success(message));
            }
        }
    }

    fn clear_encrypt_inputs(&mut self) {
        self.selected_file = None;
        self.text_input.clear();
        self.passphrase.clear();
        self.encrypted_output_path = None;
    }

    fn clear_decrypt_inputs(&mut self) {
        self.encrypted_input_path = None;
        self.decrypted_text.clear();
        self.decrypted_file_bytes = None;
        self.decrypted_file_name = None;
        self.decrypted_output_path = None;
        self.passphrase.clear();
    }
}

// ---------------------------------------------------------------------------
// 全局样式
// ---------------------------------------------------------------------------

fn apply_app_style(ctx: &egui::Context) {
    // 每帧跟随系统明暗模式重新套用主题。这里没有用户自定义主题开关，所以直接
    // 使用系统偏好；如果以后增加设置项，可以把 ThemePreference 提到 app state。
    ctx.set_theme(egui::ThemePreference::System);
    let mut style = (*ctx.style()).clone();

    // 全局间距只设置基础密度。卡片、按钮等固定格式控件仍在对应 helper 里
    // 单独指定尺寸，避免全局 spacing 改动影响关键布局。
    style.spacing.item_spacing = egui::vec2(12.0, 10.0);
    style.spacing.button_padding = egui::vec2(16.0, 8.0);

    let colors = theme_colors(ctx);

    // 面板和窗口共用应用背景色，保证 TopPanel / SidePanel / CentralPanel
    // 的底色一致。
    style.visuals.panel_fill = colors.app_bg;
    style.visuals.window_fill = colors.app_bg;
    style.visuals.extreme_bg_color = colors.surface_alt;

    // egui 的大部分内置控件都会读取 widgets.* 状态色。这里定义的是“默认控件”
    // 的基础风格；自绘按钮、卡片、拖拽框会在各自 helper 中覆盖这些值。
    style.visuals.widgets.noninteractive.bg_fill = colors.surface;
    style.visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, colors.border);
    style.visuals.widgets.noninteractive.fg_stroke.color = colors.text_main;

    style.visuals.widgets.inactive.bg_fill = colors.surface_alt;
    style.visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, colors.border);
    style.visuals.widgets.inactive.fg_stroke.color = colors.text_main;

    style.visuals.widgets.hovered.bg_fill = colors.surface_alt;
    style.visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, colors.border_hover);
    style.visuals.widgets.hovered.fg_stroke.color = colors.text_main;

    style.visuals.widgets.active.bg_fill = colors.primary_soft;
    style.visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, colors.primary);
    style.visuals.widgets.active.fg_stroke.color = colors.text_main;

    // 统一内置控件圆角。卡片和拖拽框是更大的容器，单独使用 10px 圆角。
    let radius = egui::CornerRadius::same(8);
    style.visuals.widgets.noninteractive.corner_radius = radius;
    style.visuals.widgets.inactive.corner_radius = radius;
    style.visuals.widgets.hovered.corner_radius = radius;
    style.visuals.widgets.active.corner_radius = radius;

    // 文本选择、输入光标和键盘焦点使用主色，和 tab 选中态保持一致。
    style.visuals.selection.stroke = egui::Stroke::new(1.5, colors.primary);
    style.visuals.selection.bg_fill = colors.primary_soft;
    style.visuals.text_cursor.stroke = egui::Stroke::new(1.5, colors.primary);

    ctx.set_style(style);
}

// ---------------------------------------------------------------------------
// 配色系统
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct ThemeColors {
    app_bg: egui::Color32,
    surface: egui::Color32,
    surface_alt: egui::Color32,
    border: egui::Color32,
    border_hover: egui::Color32,
    primary: egui::Color32,
    primary_soft: egui::Color32,
    text_main: egui::Color32,
    text_muted: egui::Color32,
    text_on_primary: egui::Color32,
    success: egui::Color32,
    success_bg: egui::Color32,
    error: egui::Color32,
    error_bg: egui::Color32,
}

fn theme_colors(ctx: &egui::Context) -> ThemeColors {
    // 只在这里把 egui 的 dark_mode 映射为 Encrust 自己的语义色。
    // 组件层不要直接写 RGB，避免浅色/深色模式出现遗漏。
    let visuals = ctx.style().visuals.clone();

    if visuals.dark_mode {
        ThemeColors {
            app_bg: egui::Color32::from_rgb(28, 28, 30),
            surface: egui::Color32::from_rgb(44, 44, 46),
            surface_alt: egui::Color32::from_rgb(58, 58, 60),
            border: egui::Color32::from_rgb(72, 72, 74),
            border_hover: egui::Color32::from_rgb(100, 100, 102),
            primary: DARK_PRIMARY,
            primary_soft: egui::Color32::from_rgb(40, 40, 55),
            text_main: egui::Color32::from_rgb(235, 235, 235),
            text_muted: egui::Color32::from_rgb(152, 152, 157),
            text_on_primary: egui::Color32::from_rgb(255, 255, 255),
            success: DARK_SUCCESS,
            success_bg: DARK_SUCCESS_BG,
            error: DARK_ERROR,
            error_bg: DARK_ERROR_BG,
        }
    } else {
        ThemeColors {
            app_bg: egui::Color32::from_rgb(248, 249, 252),
            surface: egui::Color32::from_rgb(255, 255, 255),
            surface_alt: egui::Color32::from_rgb(241, 243, 247),
            border: egui::Color32::from_rgb(222, 226, 233),
            border_hover: egui::Color32::from_rgb(190, 195, 205),
            primary: PRIMARY,
            primary_soft: egui::Color32::from_rgb(238, 240, 255),
            text_main: egui::Color32::from_rgb(31, 35, 40),
            text_muted: egui::Color32::from_rgb(107, 112, 123),
            text_on_primary: egui::Color32::from_rgb(255, 255, 255),
            success: SUCCESS,
            success_bg: SUCCESS_BG,
            error: ERROR,
            error_bg: ERROR_BG,
        }
    }
}

fn draw_top_panel_separator(ctx: &egui::Context) {
    let colors = theme_colors(ctx);
    let screen_rect = ctx.screen_rect();
    let separator_y = screen_rect.top() + TOP_BAR_HEIGHT;
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("top_panel_separator"),
    ));

    // TopPanel 默认分隔线只覆盖顶部面板自身，不一定和 SidePanel 的竖线视觉上
    // 完全接上。这里手动画一条全宽横线，并复用同一个 border 色值和 1px 粗细。
    painter.line_segment(
        [
            egui::pos2(screen_rect.left(), separator_y),
            egui::pos2(screen_rect.right(), separator_y),
        ],
        egui::Stroke::new(1.0, colors.border),
    );
}

// ---------------------------------------------------------------------------
// UI 组件辅助函数
// ---------------------------------------------------------------------------

fn primary_button(text: &str, colors: ThemeColors) -> egui::Button<'static> {
    // 主操作按钮用于真正执行加密/解密/保存，固定尺寸让禁用态和可点击态不跳动。
    egui::Button::new(
        egui::RichText::new(text)
            .color(colors.text_on_primary)
            .strong(),
    )
    .fill(colors.primary)
    .corner_radius(8)
    .stroke(egui::Stroke::NONE)
    .min_size(PRIMARY_BUTTON_SIZE.into())
}

fn secondary_button(text: &str, colors: ThemeColors) -> egui::Button<'static> {
    // 次级按钮用于选择文件等辅助动作，保持浅底色和 1px 边框。
    egui::Button::new(egui::RichText::new(text).color(colors.text_main).size(13.0))
        .fill(colors.surface_alt)
        .corner_radius(6)
        .stroke(egui::Stroke::new(1.0, colors.border))
        .min_size(SECONDARY_BUTTON_SIZE.into())
}

fn save_as_button(colors: ThemeColors) -> egui::Button<'static> {
    // “另存为...”在加密输出和解密输出中复用，尺寸单独固定，避免不同卡片里宽度不一。
    egui::Button::new(
        egui::RichText::new("另存为...")
            .color(colors.text_main)
            .size(13.0),
    )
    .fill(colors.surface_alt)
    .corner_radius(6)
    .stroke(egui::Stroke::new(1.0, colors.border))
    .min_size(SAVE_AS_BUTTON_SIZE.into())
}

fn clear_icon_button(ui: &mut egui::Ui, colors: ThemeColors) -> egui::Response {
    // 选择路径行末尾的小删除按钮是自绘圆形，避免普通按钮占用过多高度。
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(CLEAR_BUTTON_SIZE, CLEAR_BUTTON_SIZE),
        egui::Sense::click(),
    );
    let fill = if response.hovered() {
        colors.error
    } else {
        colors.error_bg
    };
    let text_color = if response.hovered() {
        colors.text_on_primary
    } else {
        colors.error
    };

    ui.painter()
        .circle_filled(rect.center(), rect.width() / 2.0, fill);
    ui.painter().circle_stroke(
        rect.center(),
        rect.width() / 2.0,
        egui::Stroke::new(1.0, colors.error),
    );
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        "x",
        egui::FontId::proportional(10.0),
        text_color,
    );

    response
}

fn selected_path_row(ui: &mut egui::Ui, kind: &str, path: &str, colors: ThemeColors) -> bool {
    // 拖拽框选中文件后会压缩高度，这一行负责展示状态、路径和清除按钮。
    // 路径使用 truncate，保证长路径不会把清除按钮挤出可视区域。
    let mut clear_clicked = false;

    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), SELECTED_PATH_ROW_HEIGHT),
        egui::Layout::left_to_right(egui::Align::Center).with_cross_align(egui::Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing.x = 8.0;

            egui::Frame::new()
                .fill(colors.primary_soft)
                .stroke(egui::Stroke::new(1.0, colors.primary))
                .corner_radius(6)
                .inner_margin(egui::Margin::symmetric(8, 3))
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new("已选择")
                            .color(colors.primary)
                            .size(12.0)
                            .strong(),
                    );
                });

            ui.label(
                egui::RichText::new(format!("{kind}："))
                    .color(colors.text_muted)
                    .size(13.0),
            );

            let path_width = (ui.available_width() - CLEAR_BUTTON_SIZE - 8.0).max(120.0);
            ui.add_sized(
                [path_width, SELECTED_PATH_ROW_HEIGHT],
                egui::Label::new(
                    egui::RichText::new(path)
                        .color(colors.text_main)
                        .size(13.0)
                        .strong(),
                )
                .truncate(),
            );

            clear_clicked = clear_icon_button(ui, colors).clicked();
        },
    );

    clear_clicked
}

fn notice_frame(fill: egui::Color32, stroke: egui::Color32) -> egui::Frame {
    // Toast 和状态提示共用的边框样式。颜色由调用方按成功/失败语义传入。
    egui::Frame::new()
        .fill(fill)
        .stroke(egui::Stroke::new(1.0, stroke))
        .corner_radius(8)
        .inner_margin(egui::Margin::symmetric(16, 12))
}

fn scrollable_text_edit(ui: &mut egui::Ui, text: &mut String, height: f32, hint: &str) {
    // 多行文本框外层固定 ScrollArea 高度，避免大量文本把主界面撑高。
    egui::ScrollArea::vertical()
        .max_height(height)
        .min_scrolled_height(height)
        .auto_shrink(false)
        .show(ui, |ui| {
            ui.add_sized(
                [ui.available_width(), height],
                egui::TextEdit::multiline(text)
                    .desired_width(f32::INFINITY)
                    .hint_text(hint)
                    .margin(egui::Margin::symmetric(4, 4)),
            );
        });
}

fn path_display(ui: &mut egui::Ui, label: &str, value: &str, colors: ThemeColors) {
    // 路径展示是只读信息行。右侧值截断显示，完整路径仍保存在 state 中。
    egui::Frame::new()
        .fill(colors.surface_alt)
        .stroke(egui::Stroke::new(1.0, colors.border))
        .corner_radius(6)
        .inner_margin(egui::Margin::symmetric(12, 9))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("{label}："))
                        .color(colors.text_muted)
                        .size(13.0),
                );
                let available = ui.available_width();
                ui.add_sized(
                    [available.max(0.0), 18.0],
                    egui::Label::new(
                        egui::RichText::new(value)
                            .color(colors.text_main)
                            .size(13.0),
                    )
                    .truncate(),
                );
            });
        });
}

fn sidebar_card(ui: &mut egui::Ui, title: &str, content: impl FnOnce(&mut egui::Ui)) {
    // 左侧所有配置块都走这个 helper，确保加密/解密模式下卡片宽度和内边距一致。
    let colors = theme_colors(ui.ctx());
    let inner_width = (SIDEBAR_CARD_WIDTH - f32::from(SIDEBAR_CARD_PADDING) * 2.0).max(0.0);

    egui::Frame::new()
        .fill(colors.surface)
        .stroke(egui::Stroke::new(1.5, colors.border))
        .corner_radius(10)
        .inner_margin(egui::Margin::same(SIDEBAR_CARD_PADDING))
        .show(ui, |ui| {
            ui.set_min_width(inner_width);
            ui.set_width(inner_width);
            ui.label(
                egui::RichText::new(title)
                    .size(12.0)
                    .color(colors.text_muted)
                    .strong(),
            );
            ui.add_space(10.0);
            content(ui);
        });
}
