use std::path::PathBuf;
use std::time::{Duration, Instant};

use eframe::egui;
use rfd::FileDialog;

use crate::crypto::{self, ContentKind, DecryptedPayload};
use crate::io;

const SUCCESS: egui::Color32 = egui::Color32::from_rgb(22, 101, 52);
const SUCCESS_BG: egui::Color32 = egui::Color32::from_rgb(240, 253, 244);
const ERROR: egui::Color32 = egui::Color32::from_rgb(185, 28, 28);
const ERROR_BG: egui::Color32 = egui::Color32::from_rgb(254, 242, 242);
const DARK_SUCCESS: egui::Color32 = egui::Color32::from_rgb(134, 239, 172);
const DARK_SUCCESS_BG: egui::Color32 = egui::Color32::from_rgb(20, 83, 45);
const DARK_ERROR: egui::Color32 = egui::Color32::from_rgb(252, 165, 165);
const DARK_ERROR_BG: egui::Color32 = egui::Color32::from_rgb(127, 29, 29);

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
        }
    }
}

impl eframe::App for EncrustApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        apply_app_style(ctx);
        self.capture_dropped_files(ctx);
        let window_width = ctx.screen_rect().width();
        let side_width = if window_width < 820.0 {
            220.0
        } else if window_width < 1040.0 {
            260.0
        } else {
            300.0
        };

        egui::TopBottomPanel::top("menu_bar")
            .resizable(false)
            .exact_height(42.0)
            .frame({
                let mut frame = egui::Frame::side_top_panel(&ctx.style());
                frame.inner_margin = egui::Margin::ZERO;
                frame.stroke = egui::Stroke::NONE;
                frame
            })
            .show(ctx, |ui| {
                let colors = theme_colors(ctx);
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.add_space(8.0);
                    ui.spacing_mut().item_spacing.x = 0.0;

                    for (mode, label) in [(OperationMode::Encrypt, "加密"), (OperationMode::Decrypt, "解密")] {
                        let active = self.operation_mode == mode;
                        let resp = ui.add_sized(
                            [72.0, 36.0],
                            egui::Label::new(egui::RichText::new(label).color(if active { colors.text_main } else { colors.text_muted }).strong())
                                .selectable(false)
                                .sense(egui::Sense::click()),
                        );
                        if resp.clicked() {
                            self.set_operation_mode(mode);
                        }
                        if active {
                            let rect = resp.rect;
                            let bar_y = rect.bottom();
                            let bar_height = 2.5;
                            let bar_width = 28.0;
                            let bar_x = rect.center().x - bar_width / 2.0;
                            ui.painter().rect_filled(
                                egui::Rect::from_min_max(egui::pos2(bar_x, bar_y - bar_height), egui::pos2(bar_x + bar_width, bar_y)),
                                0.0,
                                colors.text_main,
                            );
                        }
                    }
                });
            });

        egui::SidePanel::left("settings").resizable(false).exact_width(side_width).show(ctx, |ui| {
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                ui.add_space(16.0);
                ui.vertical(|ui| {
                    ui.set_width((side_width - 32.0).max(180.0));
                    self.render_sidebar(ui);
                });
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(18.0);
            let horizontal_padding = 18.0;
            let content_width = (ui.available_width() - horizontal_padding * 2.0).max(320.0).min(760.0);
            let content_height = (ui.available_height() - 18.0).max(360.0);

            ui.horizontal(|ui| {
                ui.add_space(horizontal_padding);
                ui.vertical(|ui| {
                    ui.set_width(content_width);
                    match self.operation_mode {
                        OperationMode::Encrypt => self.render_encrypt_view(ui, content_height),
                        OperationMode::Decrypt => self.render_decrypt_view(ui, content_height),
                    }
                });
            });
        });

        self.render_toast(ctx);
    }
}

impl EncrustApp {
    /// egui 把拖拽文件放在输入事件里。这里每帧检查一次，
    /// 如果发现用户拖入了文件，就根据当前顶层模式分配给加密或解密流程。
    fn capture_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped_files = ctx.input(|input| input.raw.dropped_files.clone());

        if let Some(path) = dropped_files.into_iter().find_map(|file| file.path) {
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
        ui.heading("选项");
        ui.separator();

        if self.operation_mode == OperationMode::Encrypt {
            ui.label("输入类型");
            self.render_encrypt_input_tabs(ui);
            ui.add_space(14.0);
        } else {
            ui.label("输入");
            ui.label("右侧选择或拖入 .encrust 文件");
            ui.add_space(14.0);
        }

        self.render_passphrase_input(ui);
    }

    fn set_operation_mode(&mut self, mode: OperationMode) {
        if self.operation_mode != mode {
            self.operation_mode = mode;
            self.toast = None;
        }
    }

    fn render_encrypt_view(&mut self, ui: &mut egui::Ui, available_height: f32) {
        let text_height = (available_height - 300.0).clamp(180.0, 340.0);

        match self.encrypt_input_mode {
            EncryptInputMode::File => self.render_file_encrypt_input(ui),
            EncryptInputMode::Text => self.render_text_encrypt_input(ui, text_height),
        }

        ui.add_space(12.0);
        self.render_encrypted_output_picker(ui);
        ui.add_space(14.0);
        self.render_encrypt_action(ui);
        ui.add_space(60.0);
    }

    fn render_decrypt_view(&mut self, ui: &mut egui::Ui, available_height: f32) {
        ui.heading("解密");
        ui.label("将 .encrust 文件拖拽到窗口中，或通过系统文件选择器手动选择");
        ui.add_space(12.0);

        ui.group(|ui| {
            ui.set_width(ui.available_width());
            ui.label(egui::RichText::new("加密文件").strong());
            ui.add_space(12.0);

            if ui.button("选择加密文件").clicked() {
                if let Some(path) = FileDialog::new().add_filter("Encrust 加密文件", &["encrust"]).pick_file() {
                    self.set_encrypted_input_path(path);
                }
            }

            match &self.encrypted_input_path {
                Some(path) => {
                    path_display(ui, "已选择", &path.display().to_string());
                }
                None => {}
            }
        });

        ui.add_space(12.0);
        self.render_decrypt_action(ui);
        self.render_decrypt_result(ui, available_height);
        ui.add_space(24.0);
    }

    fn render_encrypt_input_tabs(&mut self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            // 这里不用 `selectable_value` 的原因是：切换模式时除了修改模式，
            // 还要同步更新默认输出路径。显式处理 clicked() 更适合教学和扩展。
            if ui.radio(self.encrypt_input_mode == EncryptInputMode::File, "文件").clicked() {
                self.encrypt_input_mode = EncryptInputMode::File;
                if let Some(path) = &self.selected_file {
                    self.encrypted_output_path = Some(io::default_file_output_path(path));
                }
            }

            if ui.radio(self.encrypt_input_mode == EncryptInputMode::Text, "文本").clicked() {
                self.encrypt_input_mode = EncryptInputMode::Text;
                self.encrypted_output_path = Some(io::default_text_output_path());
            }
        });
    }

    fn render_file_encrypt_input(&mut self, ui: &mut egui::Ui) {
        ui.heading("文件加密");
        ui.label("将文件拖拽到窗口中，或通过系统文件选择器手动选择");
        ui.add_space(12.0);

        ui.group(|ui| {
            ui.set_width(ui.available_width());
            ui.label(egui::RichText::new("输入文件").strong());
            ui.add_space(12.0);

            if ui.button("选择文件").clicked() {
                if let Some(path) = FileDialog::new().pick_file() {
                    self.selected_file = Some(path.clone());
                    self.encrypted_output_path = Some(io::default_file_output_path(&path));
                    self.toast = None;
                }
            }

            match &self.selected_file {
                Some(path) => {
                    path_display(ui, "已选择", &path.display().to_string());
                }
                None => {}
            }
        });
    }

    fn render_text_encrypt_input(&mut self, ui: &mut egui::Ui, text_height: f32) {
        ui.heading("文本加密");
        ui.label("输入需要加密为 .encrust 文件的文本");
        ui.add_space(12.0);

        ui.group(|ui| {
            ui.set_width(ui.available_width());
            ui.add_space(12.0);

            scrollable_text_edit(ui, &mut self.text_input, text_height, "在这里输入文本...");

            if self.encrypted_output_path.is_none() {
                self.encrypted_output_path = Some(io::default_text_output_path());
            }
        });
    }

    fn render_passphrase_input(&mut self, ui: &mut egui::Ui) {
        ui.label("密钥");
        ui.add_space(10.0);
        ui.add_sized([ui.available_width(), 34.0], egui::TextEdit::singleline(&mut self.passphrase).password(true).hint_text("至少 8 个字符"));

        if !self.passphrase.is_empty() {
            if let Err(err) = crypto::validate_passphrase(&self.passphrase) {
                ui.add_space(8.0);
                ui.colored_label(theme_colors(ui.ctx()).error, err.to_string());
            }
        }
    }

    fn render_encrypted_output_picker(&mut self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            ui.set_width(ui.available_width());
            ui.label(egui::RichText::new("输出路径").strong());
            ui.label("加密结果会保存为一个 .encrust 文件");
            ui.add_space(12.0);

            ui.horizontal_wrapped(|ui| {
                let output_label = self.encrypted_output_path.as_ref().map(|path| path.display().to_string()).unwrap_or_else(|| "".to_owned());

                path_display(ui, "保存到", &output_label);

                if ui.button("另存为...").clicked() {
                    let dialog = FileDialog::new().set_file_name("encrypted.encrust");
                    if let Some(path) = dialog.save_file() {
                        self.encrypted_output_path = Some(path);
                        self.toast = None;
                    }
                }
            });
        });
    }

    fn render_encrypt_action(&mut self, ui: &mut egui::Ui) {
        let can_encrypt = self.can_encrypt();

        if ui.add_enabled(can_encrypt, egui::Button::new("加密并保存").min_size([132.0, 34.0].into())).clicked() {
            self.encrypt_active_input();
        }
    }

    fn render_decrypt_action(&mut self, ui: &mut egui::Ui) {
        let can_decrypt = self.encrypted_input_path.is_some() && crypto::validate_passphrase(&self.passphrase).is_ok();

        if ui.add_enabled(can_decrypt, egui::Button::new("解密").min_size([96.0, 34.0].into())).clicked() {
            self.decrypt_selected_file();
        }
    }

    fn render_decrypt_result(&mut self, ui: &mut egui::Ui, _available_height: f32) {
        if !self.decrypted_text.is_empty() {
            ui.add_space(12.0);
            ui.group(|ui| {
                ui.set_width(ui.available_width());
                ui.label(egui::RichText::new("解密后的文本").strong());
                ui.add_space(12.0);
                let text_height = ui.available_height().clamp(180.0, 400.0);
                scrollable_text_edit(ui, &mut self.decrypted_text, text_height, "");
            });
            ui.add_space(10.0);
            if ui.button("复制文本").clicked() {
                ui.ctx().copy_text(self.decrypted_text.clone());
                self.clear_decrypt_inputs();
                self.show_toast(Notice::Success("已复制解密后的文本".to_owned()));
            }
        }

        if self.decrypted_file_bytes.is_some() {
            ui.add_space(12.0);
            ui.group(|ui| {
                ui.set_width(ui.available_width());
                ui.label(egui::RichText::new("解密后的文件").strong());
                ui.label("解密内容已准备好，选择路径后保存");
                ui.add_space(12.0);

                if let Some(name) = &self.decrypted_file_name {
                    path_display(ui, "原文件名", name);
                }

                ui.horizontal_wrapped(|ui| {
                    let output_label = self.decrypted_output_path.as_ref().map(|path| path.display().to_string()).unwrap_or_else(|| "".to_owned());
                    path_display(ui, "保存到", &output_label);

                    if ui.button("另存为...").clicked() {
                        let file_name = self.decrypted_file_name.clone().unwrap_or_else(|| "decrypted-output".to_owned());
                        if let Some(path) = FileDialog::new().set_file_name(&file_name).save_file() {
                            self.decrypted_output_path = Some(path);
                            self.toast = None;
                        }
                    }
                });

                ui.add_space(10.0);
                if ui.button("保存解密文件").clicked() {
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
        let (message, fill, stroke, text_color) = match &toast.notice {
            Notice::Success(message) => (message, colors.success_bg, colors.success, colors.success),
            Notice::Error(message) => (message, colors.error_bg, colors.error, colors.error),
        };

        egui::Area::new("toast".into()).anchor(egui::Align2::CENTER_TOP, [0.0, 52.0]).interactable(false).show(ctx, |ui| {
            notice_frame(fill, stroke).show(ui, |ui| {
                ui.set_max_width(360.0);
                ui.label(egui::RichText::new(message).color(text_color).strong());
            });
        });

        ctx.request_repaint_after(Duration::from_millis(250));
    }

    fn show_toast(&mut self, notice: Notice) {
        self.toast = Some(Toast { notice, created_at: Instant::now() });
    }

    fn can_encrypt(&self) -> bool {
        let has_valid_input = match self.encrypt_input_mode {
            EncryptInputMode::File => self.selected_file.is_some(),
            EncryptInputMode::Text => !self.text_input.trim().is_empty(),
        };

        has_valid_input && self.encrypted_output_path.is_some() && crypto::validate_passphrase(&self.passphrase).is_ok()
    }

    /// 根据当前激活的加密输入模式读取明文、加密并写入输出路径。
    ///
    /// 这个函数故意返回 `()`，并把错误写入 toast。UI 事件处理函数通常
    /// 不直接向上抛错，而是把结果转换成用户能看到的状态。
    fn encrypt_active_input(&mut self) {
        let result = self
            .load_active_plaintext()
            .and_then(|(plaintext, kind, file_name)| {
                crypto::encrypt_bytes(&plaintext, &self.passphrase, kind, file_name.as_deref()).map_err(|err| err.to_string())
            })
            .and_then(|encrypted| {
                let output_path = self.encrypted_output_path.clone().ok_or_else(|| "请选择保存路径".to_owned())?;
                io::write_file(&output_path, &encrypted).map_err(|err| format!("保存失败：{err}"))?;
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
            .and_then(|path| io::read_file(path).map(|bytes| (path.clone(), bytes)).map_err(|err| format!("读取加密文件失败：{err}")))
            .and_then(|(path, encrypted)| {
                crypto::decrypt_bytes(&encrypted, &self.passphrase).map(|payload| (path, payload)).map_err(|err| err.to_string())
            });

        match result {
            Ok((path, payload)) => self.apply_decrypted_payload(path, payload),
            Err(err) => self.show_toast(Notice::Error(err)),
        }
    }

    fn save_decrypted_file(&mut self) {
        let result = self.decrypted_file_bytes.as_ref().ok_or_else(|| "没有可保存的解密文件".to_owned()).and_then(|bytes| {
            let output_path = self.decrypted_output_path.clone().ok_or_else(|| "请选择解密文件保存路径".to_owned())?;
            io::write_file(&output_path, bytes).map_err(|err| format!("保存解密文件失败：{err}"))?;
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
                let path = self.selected_file.as_ref().ok_or_else(|| "请选择要加密的文件".to_owned())?;
                let bytes = io::read_file(path).map_err(|err| format!("读取文件失败：{err}"))?;
                let file_name = path.file_name().and_then(|name| name.to_str()).map(ToOwned::to_owned);
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
                    self.show_toast(Notice::Error("解密成功，但内容不是有效的 UTF-8 文本".to_owned()));
                }
            },
            ContentKind::File => {
                self.decrypted_output_path = Some(io::default_decrypted_output_path(&encrypted_path, payload.file_name.as_deref()));
                self.decrypted_file_name = payload.file_name;
                self.decrypted_file_bytes = Some(payload.plaintext);
                self.passphrase.clear();
                self.show_toast(Notice::Success("文件解密成功，请选择保存位置".to_owned()));
            }
        }
    }

    /// 加密保存完成后清理界面状态。
    ///
    /// 这里会清掉密钥、文本和已选文件，避免用户离开电脑后界面还暴露输入内容。
    /// 输出路径也一起清掉，因为它可能包含隐私目录或文件名。
    fn clear_encrypt_inputs(&mut self) {
        self.selected_file = None;
        self.text_input.clear();
        self.passphrase.clear();
        self.encrypted_output_path = None;
    }

    /// 解密流程结束后清理界面状态。
    ///
    /// 解密文本和解密文件 bytes 都属于敏感明文；保存或复制完成后应尽快从 UI 状态中移除。
    fn clear_decrypt_inputs(&mut self) {
        self.encrypted_input_path = None;
        self.decrypted_text.clear();
        self.decrypted_file_bytes = None;
        self.decrypted_file_name = None;
        self.decrypted_output_path = None;
        self.passphrase.clear();
    }
}

/// 统一设置 egui 的全局视觉参数。
///
/// egui 没有 CSS 这样的样式表，所以通常会在每一帧开始时调整 `Style`。
/// 这些值控制控件间距、按钮内边距、输入框圆角等基础视觉语言。
fn apply_app_style(ctx: &egui::Context) {
    ctx.set_theme(egui::ThemePreference::System);
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(10.0, 8.0);
    style.spacing.button_padding = egui::vec2(16.0, 8.0);
    let colors = theme_colors(ctx);
    style.visuals.panel_fill = colors.app_bg;
    style.visuals.window_fill = colors.app_bg;
    style.visuals.extreme_bg_color = colors.surface_alt;
    style.visuals.widgets.noninteractive.bg_fill = colors.surface;
    style.visuals.widgets.inactive.bg_fill = colors.surface_alt;
    style.visuals.widgets.hovered.bg_fill = colors.surface_alt;
    style.visuals.widgets.active.bg_fill = colors.primary_soft;
    style.visuals.widgets.inactive.fg_stroke.color = colors.text_main;
    style.visuals.widgets.noninteractive.fg_stroke.color = colors.text_main;
    style.visuals.widgets.noninteractive.corner_radius = 4.into();
    style.visuals.widgets.inactive.corner_radius = 4.into();
    style.visuals.widgets.hovered.corner_radius = 4.into();
    style.visuals.widgets.active.corner_radius = 4.into();
    ctx.set_style(style);
}

#[derive(Clone, Copy)]
struct ThemeColors {
    app_bg: egui::Color32,
    surface: egui::Color32,
    surface_alt: egui::Color32,
    border: egui::Color32,
    primary_soft: egui::Color32,
    text_main: egui::Color32,
    text_muted: egui::Color32,
    success: egui::Color32,
    success_bg: egui::Color32,
    error: egui::Color32,
    error_bg: egui::Color32,
}

fn theme_colors(ctx: &egui::Context) -> ThemeColors {
    let visuals = ctx.style().visuals.clone();

    if visuals.dark_mode {
        ThemeColors {
            app_bg: egui::Color32::from_rgb(24, 24, 27),
            surface: egui::Color32::from_rgb(32, 33, 36),
            surface_alt: egui::Color32::from_rgb(39, 39, 42),
            border: egui::Color32::from_rgb(63, 63, 70),
            primary_soft: egui::Color32::from_rgb(38, 46, 61),
            text_main: egui::Color32::from_rgb(244, 244, 245),
            text_muted: egui::Color32::from_rgb(161, 161, 170),
            success: DARK_SUCCESS,
            success_bg: DARK_SUCCESS_BG,
            error: DARK_ERROR,
            error_bg: DARK_ERROR_BG,
        }
    } else {
        ThemeColors {
            app_bg: egui::Color32::from_rgb(246, 247, 249),
            surface: egui::Color32::from_rgb(255, 255, 255),
            surface_alt: egui::Color32::from_rgb(242, 244, 247),
            border: egui::Color32::from_rgb(214, 219, 227),
            primary_soft: egui::Color32::from_rgb(239, 246, 255),
            text_main: egui::Color32::from_rgb(17, 24, 39),
            text_muted: egui::Color32::from_rgb(100, 116, 139),
            success: SUCCESS,
            success_bg: SUCCESS_BG,
            error: ERROR,
            error_bg: ERROR_BG,
        }
    }
}

fn notice_frame(fill: egui::Color32, stroke: egui::Color32) -> egui::Frame {
    egui::Frame::new().fill(fill).stroke(egui::Stroke::new(1.0, stroke)).corner_radius(4).inner_margin(egui::Margin::same(14))
}

fn scrollable_text_edit(ui: &mut egui::Ui, text: &mut String, height: f32, hint: &str) {
    // 多行 TextEdit 会根据内容计算内部排版高度。把它包进固定高度的
    // ScrollArea 后，长文本只会在这个区域内滚动，不会把整个表单撑高。
    egui::ScrollArea::vertical().max_height(height).min_scrolled_height(height).auto_shrink(false).show(ui, |ui| {
        ui.add_sized([ui.available_width(), height], egui::TextEdit::multiline(text).desired_width(f32::INFINITY).hint_text(hint));
    });
}

fn path_display(ui: &mut egui::Ui, label: &str, value: &str) {
    let colors = theme_colors(ui.ctx());

    egui::Frame::new()
        .fill(colors.surface_alt)
        .stroke(egui::Stroke::new(1.0, colors.border))
        .corner_radius(4)
        .inner_margin(egui::Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new(format!("{label}：")).color(colors.text_muted));
                ui.label(egui::RichText::new(value).color(colors.text_main));
            });
        });
}

// fn empty_state(ui: &mut egui::Ui, text: &str) {
//     let colors = theme_colors(ui.ctx());

//     egui::Frame::new()
//         .fill(colors.surface_alt)
//         .stroke(egui::Stroke::new(1.0, colors.border))
//         .corner_radius(4)
//         .inner_margin(egui::Margin::symmetric(12, 10))
//         .show(ui, |ui| {
//             ui.label(egui::RichText::new(text).color(colors.text_muted));
//         });
// }
