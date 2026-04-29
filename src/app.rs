use std::path::PathBuf;
use std::time::{Duration, Instant};

use eframe::egui;
use rfd::FileDialog;

use crate::crypto::{self, ContentKind, DecryptedPayload};
use crate::io;

// 浅色主色调 (indigo)
const PRIMARY: egui::Color32 = egui::Color32::from_rgb(79, 70, 229);
const SUCCESS: egui::Color32 = egui::Color32::from_rgb(16, 185, 129);
const SUCCESS_BG: egui::Color32 = egui::Color32::from_rgb(236, 253, 245);
const ERROR: egui::Color32 = egui::Color32::from_rgb(239, 68, 68);
const ERROR_BG: egui::Color32 = egui::Color32::from_rgb(254, 242, 242);

// 深色主色调
const DARK_PRIMARY: egui::Color32 = egui::Color32::from_rgb(99, 102, 241);
const DARK_SUCCESS: egui::Color32 = egui::Color32::from_rgb(52, 211, 153);
const DARK_SUCCESS_BG: egui::Color32 = egui::Color32::from_rgb(20, 50, 40);
const DARK_ERROR: egui::Color32 = egui::Color32::from_rgb(248, 113, 113);
const DARK_ERROR_BG: egui::Color32 = egui::Color32::from_rgb(50, 25, 25);

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

        let side_width = 240.0;

        // 顶部导航栏
        egui::TopBottomPanel::top("menu_bar")
            .resizable(false)
            .exact_height(52.0)
            .frame({
                let mut frame = egui::Frame::side_top_panel(&ctx.style());
                frame.inner_margin = egui::Margin::symmetric(16, 0);
                frame.stroke = egui::Stroke::new(1.0, theme_colors(ctx).border);
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

                    // 模式切换按钮
                    for (mode, label) in [
                        (OperationMode::Encrypt, "加密"),
                        (OperationMode::Decrypt, "解密"),
                    ] {
                        let active = self.operation_mode == mode;
                        let text = egui::RichText::new(label)
                            .size(14.0)
                            .color(if active {
                                colors.text_on_primary
                            } else {
                                colors.text_muted
                            })
                            .strong();
                        let btn = egui::Button::new(text)
                            .fill(if active {
                                colors.primary
                            } else {
                                egui::Color32::TRANSPARENT
                            })
                            .corner_radius(8)
                            .stroke(egui::Stroke::NONE)
                            .min_size([72.0, 34.0].into());
                        if ui.add(btn).clicked() {
                            self.set_operation_mode(mode);
                        }
                    }
                });
            });

        // 左侧边栏
        egui::SidePanel::left("settings")
            .resizable(false)
            .exact_width(side_width)
            .frame(egui::Frame::side_top_panel(&ctx.style()).stroke(egui::Stroke::NONE))
            .show(ctx, |ui| {
                ui.add_space(16.0);
                ui.horizontal(|ui| {
                    ui.add_space(16.0);
                    ui.vertical(|ui| {
                        ui.set_width((side_width - 32.0).max(180.0));
                        self.render_sidebar(ui);
                    });
                });
            });

        // 主内容区（外层包裹 ScrollArea，防止内容超出窗口边界）
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(16.0);
            let horizontal_padding = 20.0;
            let content_width = (ui.available_width() - horizontal_padding * 2.0)
                .max(320.0)
                .min(720.0);

            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.add_space(horizontal_padding);
                        ui.vertical(|ui| {
                            ui.set_width(content_width);
                            match self.operation_mode {
                                OperationMode::Encrypt => self.render_encrypt_view(ui),
                                OperationMode::Decrypt => self.render_decrypt_view(ui),
                            }
                        });
                    });
                });
        });

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
                    self.selected_file = Some(path.clone());
                    self.encrypted_output_path = Some(io::default_file_output_path(&path));
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

        let drop_area_height = if self.encrypted_input_path.is_some() {
            88.0
        } else {
            160.0
        };
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
            .inner_margin(egui::Margin::same(16))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.set_min_height(drop_area_height);
                ui.with_layout(
                    egui::Layout::top_down(egui::Align::Center)
                        .with_main_align(egui::Align::Center),
                    |ui| {
                        if let Some(path) = &self.encrypted_input_path {
                            ui.add_space(4.0);
                            ui.label(
                                egui::RichText::new("已选择加密文件")
                                    .color(colors.text_muted)
                                    .size(12.0),
                            );
                            ui.add_space(8.0);
                            ui.label(
                                egui::RichText::new(path.display().to_string())
                                    .color(colors.text_main)
                                    .size(14.0)
                                    .strong(),
                            );
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
                            if ui
                                .add(
                                    secondary_button("或点击选择文件", colors)
                                        .min_size([130.0, 32.0].into()),
                                )
                                .clicked()
                            {
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

        ui.add_space(16.0);
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
                        EncryptInputMode::File => {
                            if let Some(path) = &self.selected_file {
                                self.encrypted_output_path =
                                    Some(io::default_file_output_path(path));
                            }
                        }
                        EncryptInputMode::Text => {
                            self.encrypted_output_path = Some(io::default_text_output_path());
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
        ui.add_space(16.0);

        let drop_area_height = if self.selected_file.is_some() {
            88.0
        } else {
            160.0
        };
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
            .inner_margin(egui::Margin::same(16))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.set_min_height(drop_area_height);
                ui.with_layout(
                    egui::Layout::top_down(egui::Align::Center)
                        .with_main_align(egui::Align::Center),
                    |ui| {
                        if let Some(path) = &self.selected_file {
                            ui.add_space(4.0);
                            ui.label(
                                egui::RichText::new("已选择文件")
                                    .color(colors.text_muted)
                                    .size(12.0),
                            );
                            ui.add_space(8.0);
                            ui.label(
                                egui::RichText::new(path.display().to_string())
                                    .color(colors.text_main)
                                    .size(14.0)
                                    .strong(),
                            );
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
                            if ui
                                .add(
                                    secondary_button("或点击选择文件", colors)
                                        .min_size([130.0, 32.0].into()),
                                )
                                .clicked()
                            {
                                if let Some(path) = FileDialog::new().pick_file() {
                                    self.selected_file = Some(path.clone());
                                    self.encrypted_output_path =
                                        Some(io::default_file_output_path(&path));
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
                self.selected_file = Some(path.clone());
                self.encrypted_output_path = Some(io::default_file_output_path(&path));
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
        ui.add_space(16.0);

        egui::Frame::new()
            .fill(colors.surface)
            .stroke(egui::Stroke::new(1.5, colors.border))
            .corner_radius(10)
            .inner_margin(egui::Margin::same(12))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                scrollable_text_edit(ui, &mut self.text_input, 180.0, "在这里输入文本...");
            });

        if self.encrypted_output_path.is_none() {
            self.encrypted_output_path = Some(io::default_text_output_path());
        }
    }

    fn render_passphrase_input(&mut self, ui: &mut egui::Ui) {
        let colors = theme_colors(ui.ctx());

        // 使用 TextEdit 自带 frame，通过 background_color 和 margin 控制外观。
        // 显式设置 vertical_align::Center，确保 hint text 在内部 rect 中垂直居中。
        ui.add(
            egui::TextEdit::singleline(&mut self.passphrase)
                .password(true)
                .hint_text("至少 8 个字符")
                .vertical_align(egui::Align::Center)
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
                    egui::RichText::new("加密结果会保存为一个 .encrust 文件")
                        .color(colors.text_muted)
                        .size(12.0),
                );
                ui.add_space(12.0);

                ui.with_layout(
                    egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(true),
                    |ui| {
                        let output_label = self
                            .encrypted_output_path
                            .as_ref()
                            .map(|path| path.display().to_string())
                            .unwrap_or_else(|| "未选择保存路径".to_owned());

                        path_display(ui, "保存到", &output_label, colors);

                        if ui
                            .add(
                                secondary_button("另存为...", colors).min_size([90.0, 32.0].into()),
                            )
                            .clicked()
                        {
                            let dialog = FileDialog::new().set_file_name("encrypted.encrust");
                            if let Some(path) = dialog.save_file() {
                                self.encrypted_output_path = Some(path);
                                self.toast = None;
                            }
                        }
                    },
                );
            });
    }

    fn render_encrypt_action(&mut self, ui: &mut egui::Ui) {
        let colors = theme_colors(ui.ctx());
        let can_encrypt = self.can_encrypt();

        let btn = primary_button("加密并保存", colors).min_size([150.0, 42.0].into());

        // 在水平布局中添加按钮，使按钮内部文字垂直居中
        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
            if ui.add_enabled(can_encrypt, btn).clicked() {
                self.encrypt_active_input();
            }
        });
    }

    fn render_decrypt_action(&mut self, ui: &mut egui::Ui) {
        let colors = theme_colors(ui.ctx());
        let can_decrypt = self.encrypted_input_path.is_some()
            && crypto::validate_passphrase(&self.passphrase).is_ok();

        let btn = primary_button("解密", colors).min_size([110.0, 42.0].into());

        if ui.add_enabled(can_decrypt, btn).clicked() {
            self.decrypt_selected_file();
        }
    }

    fn render_decrypt_result(&mut self, ui: &mut egui::Ui) {
        if !self.decrypted_text.is_empty() {
            ui.add_space(16.0);
            let colors = theme_colors(ui.ctx());

            egui::Frame::new()
                .fill(colors.surface)
                .stroke(egui::Stroke::new(1.5, colors.border))
                .corner_radius(10)
                .inner_margin(egui::Margin::same(16))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(egui::RichText::new("解密后的文本").strong());
                    ui.add_space(12.0);
                    scrollable_text_edit(ui, &mut self.decrypted_text, 180.0, "");
                });

            ui.add_space(12.0);
            let copy_btn = primary_button("复制文本", colors).min_size([110.0, 40.0].into());
            if ui.add(copy_btn).clicked() {
                ui.ctx().copy_text(self.decrypted_text.clone());
                self.clear_decrypt_inputs();
                self.show_toast(Notice::Success("已复制解密后的文本".to_owned()));
            }
        }

        if self.decrypted_file_bytes.is_some() {
            ui.add_space(16.0);
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

                    ui.with_layout(
                        egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(true),
                        |ui| {
                            let output_label = self
                                .decrypted_output_path
                                .as_ref()
                                .map(|path| path.display().to_string())
                                .unwrap_or_else(|| "未选择保存路径".to_owned());
                            path_display(ui, "保存到", &output_label, colors);

                            if ui
                                .add(
                                    secondary_button("另存为...", colors)
                                        .min_size([90.0, 32.0].into()),
                                )
                                .clicked()
                            {
                                let file_name = self
                                    .decrypted_file_name
                                    .clone()
                                    .unwrap_or_else(|| "decrypted-output".to_owned());
                                if let Some(path) =
                                    FileDialog::new().set_file_name(&file_name).save_file()
                                {
                                    self.decrypted_output_path = Some(path);
                                    self.toast = None;
                                }
                            }
                        },
                    );

                    ui.add_space(12.0);
                    let can_save = self.decrypted_output_path.is_some();
                    let save_btn =
                        primary_button("保存解密文件", colors).min_size([150.0, 40.0].into());
                    if ui.add_enabled(can_save, save_btn).clicked() {
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
        let (message, fill, stroke, text_color, icon) = match &toast.notice {
            Notice::Success(message) => (
                message,
                colors.success_bg,
                colors.success,
                colors.success,
                "✓",
            ),
            Notice::Error(message) => (message, colors.error_bg, colors.error, colors.error, "✕"),
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
                            egui::RichText::new(icon)
                                .size(16.0)
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
                crypto::encrypt_bytes(&plaintext, &self.passphrase, kind, file_name.as_deref())
                    .map_err(|err| err.to_string())
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
                crypto::decrypt_bytes(&encrypted, &self.passphrase)
                    .map(|payload| (path, payload))
                    .map_err(|err| err.to_string())
            });

        match result {
            Ok((path, payload)) => self.apply_decrypted_payload(path, payload),
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

    fn apply_decrypted_payload(&mut self, encrypted_path: PathBuf, payload: DecryptedPayload) {
        self.decrypted_text.clear();
        self.decrypted_file_bytes = None;
        self.decrypted_file_name = None;
        self.decrypted_output_path = None;

        match payload.kind {
            ContentKind::Text => match String::from_utf8(payload.plaintext) {
                Ok(text) => {
                    self.decrypted_text = text;
                    self.passphrase.clear();
                    self.show_toast(Notice::Success("文本解密成功".to_owned()));
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
                self.show_toast(Notice::Success("文件解密成功，请选择保存位置".to_owned()));
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
    ctx.set_theme(egui::ThemePreference::System);
    let mut style = (*ctx.style()).clone();

    // 间距系统
    style.spacing.item_spacing = egui::vec2(12.0, 10.0);
    style.spacing.button_padding = egui::vec2(16.0, 8.0);

    let colors = theme_colors(ctx);

    // 背景
    style.visuals.panel_fill = colors.app_bg;
    style.visuals.window_fill = colors.app_bg;
    style.visuals.extreme_bg_color = colors.surface_alt;

    // 控件默认样式
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

    // 统一圆角
    let radius = egui::CornerRadius::same(8);
    style.visuals.widgets.noninteractive.corner_radius = radius;
    style.visuals.widgets.inactive.corner_radius = radius;
    style.visuals.widgets.hovered.corner_radius = radius;
    style.visuals.widgets.active.corner_radius = radius;

    // 选中和高亮
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
    let visuals = ctx.style().visuals.clone();

    if visuals.dark_mode {
        ThemeColors {
            app_bg: egui::Color32::from_rgb(15, 17, 23),
            surface: egui::Color32::from_rgb(24, 27, 35),
            surface_alt: egui::Color32::from_rgb(32, 36, 46),
            border: egui::Color32::from_rgb(42, 46, 58),
            border_hover: egui::Color32::from_rgb(60, 65, 82),
            primary: DARK_PRIMARY,
            primary_soft: egui::Color32::from_rgb(30, 33, 55),
            text_main: egui::Color32::from_rgb(226, 228, 233),
            text_muted: egui::Color32::from_rgb(139, 146, 168),
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

// ---------------------------------------------------------------------------
// UI 组件辅助函数
// ---------------------------------------------------------------------------

fn primary_button(text: &str, colors: ThemeColors) -> egui::Button<'static> {
    egui::Button::new(
        egui::RichText::new(text)
            .color(colors.text_on_primary)
            .strong(),
    )
    .fill(colors.primary)
    .corner_radius(8)
    .stroke(egui::Stroke::NONE)
}

fn secondary_button(text: &str, colors: ThemeColors) -> egui::Button<'static> {
    egui::Button::new(egui::RichText::new(text).color(colors.text_main).size(13.0))
        .fill(colors.surface_alt)
        .corner_radius(6)
        .stroke(egui::Stroke::new(1.0, colors.border))
}

fn notice_frame(fill: egui::Color32, stroke: egui::Color32) -> egui::Frame {
    egui::Frame::new()
        .fill(fill)
        .stroke(egui::Stroke::new(1.0, stroke))
        .corner_radius(8)
        .inner_margin(egui::Margin::symmetric(16, 12))
}

fn scrollable_text_edit(ui: &mut egui::Ui, text: &mut String, height: f32, hint: &str) {
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
    egui::Frame::new()
        .fill(colors.surface_alt)
        .stroke(egui::Stroke::new(1.0, colors.border))
        .corner_radius(6)
        .inner_margin(egui::Margin::symmetric(12, 9))
        .show(ui, |ui| {
            ui.with_layout(
                egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(true),
                |ui| {
                    ui.label(
                        egui::RichText::new(format!("{label}："))
                            .color(colors.text_muted)
                            .size(13.0),
                    );
                    ui.label(
                        egui::RichText::new(value)
                            .color(colors.text_main)
                            .size(13.0),
                    );
                },
            );
        });
}

fn sidebar_card(ui: &mut egui::Ui, title: &str, content: impl FnOnce(&mut egui::Ui)) {
    let colors = theme_colors(ui.ctx());
    egui::Frame::new()
        .fill(colors.surface)
        .stroke(egui::Stroke::new(1.5, colors.border))
        .corner_radius(10)
        .inner_margin(egui::Margin::same(14))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
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
