use anyhow::Result;
use eframe::egui;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use crate::{
    config::{Config, MouseButton, Waveform, load_or_create},
    i18n::{Language, Strings},
    device::DeviceManager,
};

/// The GUI drops its own HID connection after this long without a test, so an
/// idle settings window does not keep a second handle and reader thread open.
const HID_IDLE_DISCONNECT: Duration = Duration::from_secs(30);

enum HidRequest {
    Probe,
    Play(Waveform),
}

enum HidReply {
    Probed(Result<u32, String>),
    Played(Result<(), String>),
}

/// Runs HID++ I/O off the UI thread: connecting can take seconds when the
/// mouse is asleep, which used to freeze the window on startup and on "Test".
fn spawn_hid_worker(
    device: String,
    ctx: egui::Context,
) -> (mpsc::Sender<HidRequest>, mpsc::Receiver<HidReply>) {
    let (request_tx, request_rx) = mpsc::channel::<HidRequest>();
    let (reply_tx, reply_rx) = mpsc::channel();
    let _ = thread::Builder::new()
        .name("mx4-haptic-gui-hid".to_owned())
        .spawn(move || {
            let mut manager = DeviceManager::with_expected_name(false, device);
            loop {
                let request = match request_rx.recv_timeout(HID_IDLE_DISCONNECT) {
                    Ok(request) => request,
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        manager.disconnect();
                        continue;
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => return,
                };
                manager.connect_now();
                let reply = match request {
                    HidRequest::Probe => HidReply::Probed(
                        manager.supported_waveforms().map_err(|e| format!("{e:#}")),
                    ),
                    HidRequest::Play(waveform) => {
                        HidReply::Played(manager.play(waveform).map_err(|e| format!("{e:#}")))
                    }
                };
                if reply_tx.send(reply).is_err() {
                    return;
                }
                ctx.request_repaint();
            }
        });
    (request_tx, reply_rx)
}

/// Opens the settings window; returns when it is closed.
pub fn run() -> Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(crate::tray::WINDOW_TITLE)
            .with_inner_size([540.0, 800.0])
            // Narrower than this and the button / advanced rows get cramped.
            .with_min_inner_size([480.0, 560.0])
            .with_icon(std::sync::Arc::new(egui::IconData {
                rgba: crate::tray::icon_rgba(64),
                width: 64,
                height: 64,
            })),
        // egui only repaints on input, so vsync buys nothing here, while many
        // OpenGL drivers busy-wait for it inside SwapBuffers: hovering the
        // window burned a whole CPU core.
        vsync: false,
        ..Default::default()
    };
    eframe::run_native(
        crate::tray::WINDOW_TITLE,
        options,
        Box::new(move |cc| Ok(Box::new(ConfigApp::new(cc)))),
    )
    .map_err(|e| anyhow::anyhow!("eframe error: {}", e))
}

// ---------------------------------------------------------------------------
// Look & feel

mod theme {
    use eframe::egui::{
        self, Color32, FontFamily, FontId, Margin, Rounding, Stroke, TextStyle, Vec2,
    };

    pub const ACCENT: Color32 = Color32::from_rgb(59, 130, 246);
    pub const ACCENT_DIM: Color32 = Color32::from_rgb(37, 99, 235);
    pub const BG: Color32 = Color32::from_rgb(18, 19, 23);
    pub const CARD: Color32 = Color32::from_rgb(28, 30, 36);
    pub const CARD_BORDER: Color32 = Color32::from_rgb(42, 45, 53);
    pub const FIELD: Color32 = Color32::from_rgb(38, 41, 49);
    pub const INACTIVE: Color32 = Color32::from_rgb(33, 35, 42);
    pub const TEXT: Color32 = Color32::from_rgb(228, 230, 235);
    pub const MUTED: Color32 = Color32::from_rgb(140, 146, 160);
    pub const OK: Color32 = Color32::from_rgb(52, 199, 120);
    pub const WARN: Color32 = Color32::from_rgb(245, 166, 35);
    pub const ERR: Color32 = Color32::from_rgb(239, 83, 80);

    pub fn apply(ctx: &egui::Context) {
        let mut style = (*ctx.style()).clone();
        style.text_styles = [
            (TextStyle::Heading, FontId::new(20.0, FontFamily::Proportional)),
            (TextStyle::Body, FontId::new(14.0, FontFamily::Proportional)),
            (TextStyle::Button, FontId::new(14.0, FontFamily::Proportional)),
            (TextStyle::Small, FontId::new(12.0, FontFamily::Proportional)),
            (TextStyle::Monospace, FontId::new(13.0, FontFamily::Monospace)),
        ]
        .into();
        style.spacing.item_spacing = Vec2::new(8.0, 8.0);
        style.spacing.button_padding = Vec2::new(12.0, 6.0);
        style.spacing.interact_size.y = 28.0;

        let v = &mut style.visuals;
        *v = egui::Visuals::dark();
        v.panel_fill = BG;
        v.window_fill = CARD;
        v.extreme_bg_color = FIELD;
        v.faint_bg_color = CARD;
        v.override_text_color = Some(TEXT);
        v.selection.bg_fill = ACCENT_DIM;
        v.selection.stroke = Stroke::new(1.0, Color32::WHITE);
        v.hyperlink_color = ACCENT;
        v.window_rounding = Rounding::same(10.0);
        v.menu_rounding = Rounding::same(8.0);
        v.window_stroke = Stroke::new(1.0, CARD_BORDER);
        v.popup_shadow = egui::epaint::Shadow::NONE;
        v.window_shadow = egui::epaint::Shadow::NONE;

        let w = &mut v.widgets;
        for state in [&mut w.noninteractive, &mut w.inactive, &mut w.hovered, &mut w.active, &mut w.open] {
            state.rounding = Rounding::same(7.0);
        }
        w.noninteractive.bg_stroke = Stroke::new(1.0, CARD_BORDER);
        w.inactive.bg_fill = FIELD;
        w.inactive.weak_bg_fill = FIELD;
        w.inactive.bg_stroke = Stroke::NONE;
        w.hovered.bg_fill = Color32::from_rgb(50, 54, 64);
        w.hovered.weak_bg_fill = Color32::from_rgb(50, 54, 64);
        w.hovered.bg_stroke = Stroke::new(1.0, ACCENT);
        w.active.bg_fill = ACCENT_DIM;
        w.active.weak_bg_fill = ACCENT_DIM;
        w.open.weak_bg_fill = Color32::from_rgb(50, 54, 64);

        ctx.set_style(style);
    }

    /// A rounded panel grouping related settings.
    pub fn card<R>(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
        egui::Frame::none()
            .fill(CARD)
            .stroke(Stroke::new(1.0, CARD_BORDER))
            .rounding(Rounding::same(10.0))
            .inner_margin(Margin::same(14.0))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(egui::RichText::new(title.to_uppercase()).small().strong().color(MUTED));
                ui.add_space(2.0);
                add(ui)
            })
            .inner
    }

    /// iOS-style on/off switch.
    pub fn toggle(ui: &mut egui::Ui, on: &mut bool) -> egui::Response {
        let size = Vec2::new(36.0, 20.0);
        let (rect, mut response) = ui.allocate_exact_size(size, egui::Sense::click());
        if response.clicked() {
            *on = !*on;
            response.mark_changed();
        }
        if ui.is_rect_visible(rect) {
            let t = ui.ctx().animate_bool_responsive(response.id, *on);
            let fill = if *on { ACCENT_DIM } else { FIELD };
            let radius = rect.height() / 2.0;
            ui.painter().rect(rect, radius, fill, Stroke::new(1.0, CARD_BORDER));
            let x = egui::lerp((rect.left() + radius)..=(rect.right() - radius), t);
            ui.painter()
                .circle_filled(egui::pos2(x, rect.center().y), radius - 3.0, Color32::WHITE);
        }
        response
    }

    /// Label on the left, switch on the right, optional hint below.
    pub fn toggle_row(ui: &mut egui::Ui, on: &mut bool, label: &str, hint: &str) -> bool {
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                ui.label(label);
                if !hint.is_empty() {
                    ui.label(egui::RichText::new(hint).small().color(MUTED));
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                changed = toggle(ui, on).changed();
            });
        });
        changed
    }

    /// Small coloured dot followed by text.
    /// A filled circle; the default fonts have no "●" glyph.
    pub fn dot(ui: &mut egui::Ui, color: Color32) {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(8.0), egui::Sense::hover());
        ui.painter().circle_filled(rect.center(), 4.0, color);
    }

    /// Coloured dot + text in a rounded badge. Painted by hand so it looks
    /// the same inside left-to-right and right-to-left layouts.
    pub fn status_pill(ui: &mut egui::Ui, color: Color32, text: &str) -> egui::Response {
        let galley = ui.painter().layout_no_wrap(
            text.to_owned(),
            ui.style().text_styles[&TextStyle::Small].clone(),
            color,
        );
        let padding = Vec2::new(10.0, 5.0);
        let dot_space = 14.0;
        let size = Vec2::new(dot_space + galley.size().x, galley.size().y) + padding * 2.0;
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::hover());
        let painter = ui.painter();
        painter.rect_filled(rect, 12.0, color.gamma_multiply(0.15));
        let dot_center = egui::pos2(rect.left() + padding.x + 4.0, rect.center().y);
        painter.circle_filled(dot_center, 4.0, color);
        let text_pos = egui::pos2(rect.left() + padding.x + dot_space, rect.center().y - galley.size().y / 2.0);
        painter.galley(text_pos, galley, color);
        response
    }

    /// Full-width, left-aligned selectable row (accent fill when selected).
    pub fn choice_row(ui: &mut egui::Ui, selected: bool, text: &str) -> egui::Response {
        let size = Vec2::new(ui.available_width(), 34.0);
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
        let fill = if selected {
            ACCENT_DIM
        } else if response.hovered() {
            FIELD
        } else {
            Color32::TRANSPARENT
        };
        let color = if selected { Color32::WHITE } else { TEXT };
        let painter = ui.painter();
        painter.rect_filled(rect, 7.0, fill);
        painter.text(
            egui::pos2(rect.left() + 12.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            text,
            ui.style().text_styles[&TextStyle::Body].clone(),
            color,
        );
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    }

    /// Visual state of a footer button.
    #[derive(Clone, Copy, PartialEq)]
    pub enum Tone {
        Primary,
        Secondary,
        /// Nothing to do right now: calm grey, not clickable.
        Inactive,
        Success,
        Failure,
    }

    /// Footer button with explicit colours. egui's own disabled look fades
    /// widgets towards the background, which turned the accent into a
    /// washed-out blue; inactive buttons are drawn as plain grey instead.
    pub fn footer_button(ui: &mut egui::Ui, text: &str, tone: Tone, min_width: f32) -> egui::Response {
        let (fill, color) = match tone {
            Tone::Primary => (ACCENT_DIM, Color32::WHITE),
            Tone::Secondary => (FIELD, TEXT),
            Tone::Inactive => (INACTIVE, MUTED),
            Tone::Success => (OK.gamma_multiply(0.16), OK),
            Tone::Failure => (ERR.gamma_multiply(0.16), ERR),
        };
        let sense = match tone {
            Tone::Primary | Tone::Secondary | Tone::Failure => egui::Sense::click(),
            Tone::Inactive | Tone::Success => egui::Sense::hover(),
        };
        let mut text = egui::RichText::new(text.to_owned()).color(color);
        if tone == Tone::Primary {
            text = text.strong();
        }
        let button = egui::Button::new(text)
            .fill(fill)
            .stroke(Stroke::NONE)
            .sense(sense)
            .min_size(Vec2::new(min_width, 34.0));
        ui.add(button)
    }
}

/// "SUBTLE COLLISION" -> "Subtle collision".
fn pretty(waveform: Waveform) -> String {
    let lower = waveform.to_string().to_lowercase();
    let mut chars = lower.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => lower,
    }
}

// ---------------------------------------------------------------------------
// App state

#[derive(PartialEq, Clone, Copy)]
enum Page {
    Settings,
    Tester,
    Language,
}

enum Connection {
    Connecting,
    Connected,
    Disconnected(String),
}

/// Something that went wrong outside the footer buttons; shown in a
/// dismissable banner. Rendered at draw time, so it follows the language.
#[derive(Clone, Copy)]
enum Problem {
    Load,
    Save,
    Listener,
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    Autostart,
}

/// Feedback carried by a footer button's label instead of a status line.
enum ButtonState {
    Idle,
    Busy,
    /// Success label until this instant.
    Done(Instant),
    Failed(String),
}

const DONE_LABEL_FOR: Duration = Duration::from_millis(2_500);

struct TesterEntry {
    at: std::time::Instant,
    click: crate::input::Click,
}

struct ConfigApp {
    config: Config,
    /// Last state written to disk, to show unsaved changes.
    saved: Config,
    supported_mask: u32,
    connection: Connection,
    problem: Option<(Problem, String)>,
    save_state: ButtonState,
    restart_state: ButtonState,
    page: Page,
    tester_receiver: Option<mpsc::Receiver<crate::input::Click>>,
    tester_log: std::collections::VecDeque<TesterEntry>,
    hid_requests: mpsc::Sender<HidRequest>,
    hid_replies: mpsc::Receiver<HidReply>,
    restart_result: Option<mpsc::Receiver<Result<(), String>>>,
}

const TESTER_LOG_LEN: usize = 12;

impl ConfigApp {
    /// Strings in the current UI language. `'static`, so holding them does
    /// not borrow `self`.
    fn tr(&self) -> &'static Strings {
        self.config.ui_language().strings()
    }

    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::apply(&cc.egui_ctx);

        // winit just registered raw input for its own window; take mouse
        // input back if this process already runs the click listener.
        crate::input::reclaim();

        let mut problem = None;
        #[cfg_attr(not(target_os = "windows"), allow(unused_mut))]
        let mut config = load_or_create().unwrap_or_else(|e| {
            problem = Some((Problem::Load, format!("{e:#}")));
            Config {
                enabled: true,
                device: "MX Master 4".to_string(),
                only_mx_master: true,
                language: None,
                left: None,
                right: None,
                middle: None,
                forward: None,
                back: None,
                cooldown_ms: 30,
                queue_size: 4,
                autostart: false,
            }
        });

        // The registry is the source of truth (install.ps1 writes it too).
        // Re-register so the Run entry follows the current exe path and
        // carries --minimized (older builds registered it without the flag).
        #[cfg(target_os = "windows")]
        {
            config.autostart = crate::autostart::is_enabled();
            if config.autostart {
                let _ = crate::autostart::set_autostart(true);
            }
        }

        let (hid_requests, hid_replies) =
            spawn_hid_worker(config.device.clone(), cc.egui_ctx.clone());
        let _ = hid_requests.send(HidRequest::Probe);

        Self {
            saved: config.clone(),
            config,
            // Until the probe answers, offer every waveform.
            supported_mask: 0xFFFF_FFFF,
            connection: Connection::Connecting,
            problem,
            save_state: ButtonState::Idle,
            restart_state: ButtonState::Idle,
            page: Page::Settings,
            tester_receiver: None,
            tester_log: std::collections::VecDeque::new(),
            hid_requests,
            hid_replies,
            restart_result: None,
        }
    }

    fn poll_background(&mut self) {
        while let Ok(reply) = self.hid_replies.try_recv() {
            match reply {
                HidReply::Probed(Ok(mask)) => {
                    self.supported_mask = mask;
                    self.connection = Connection::Connected;
                }
                HidReply::Probed(Err(e)) => self.connection = Connection::Disconnected(e),
                // The vibration itself is the success feedback; a failure
                // shows up in the connection badge (reason on hover).
                HidReply::Played(Ok(())) => self.connection = Connection::Connected,
                HidReply::Played(Err(e)) => self.connection = Connection::Disconnected(e),
            }
        }

        if let Some(result) = self.restart_result.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.restart_result = None;
            self.restart_state = match result {
                Ok(()) => ButtonState::Done(Instant::now() + DONE_LABEL_FOR),
                Err(e) => ButtonState::Failed(e),
            };
            // Reconnect: the new service may have woken the mouse.
            self.connection = Connection::Connecting;
            let _ = self.hid_requests.send(HidRequest::Probe);
        }

        if let Some(rx) = &self.tester_receiver {
            while let Ok(click) = rx.try_recv() {
                self.tester_log.push_front(TesterEntry {
                    at: std::time::Instant::now(),
                    click,
                });
                self.tester_log.truncate(TESTER_LOG_LEN);
            }
        }
    }

    fn start_tester(&mut self, ctx: &egui::Context) {
        if self.tester_receiver.is_some() {
            return;
        }
        let (gui_tx, gui_rx) = mpsc::channel();
        self.tester_receiver = Some(gui_rx);
        let (tx, rx) = mpsc::sync_channel(16);
        if let Err(e) = crate::input::subscribe(tx, false) {
            self.problem = Some((Problem::Listener, format!("{e:#}")));
        }
        // Repaint only when a click arrives instead of redrawing
        // continuously while the tester is open.
        let ctx = ctx.clone();
        thread::spawn(move || {
            while let Ok(click) = rx.recv() {
                if gui_tx.send(click).is_err() {
                    return;
                }
                ctx.request_repaint();
            }
        });
    }

    fn header(&mut self, ui: &mut egui::Ui) {
        let s = self.tr();
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.heading(egui::RichText::new("MX Master 4 Haptic").strong());
                ui.label(egui::RichText::new(s.subtitle).small().color(theme::MUTED));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let mut retry = false;
                match &self.connection {
                    Connection::Connecting => {
                        theme::status_pill(ui, theme::WARN, s.connecting);
                    }
                    Connection::Connected => {
                        theme::status_pill(ui, theme::OK, s.connected);
                    }
                    Connection::Disconnected(reason) => {
                        retry = ui
                            .small_button("⟳")
                            .on_hover_text(s.reconnect)
                            .clicked();
                        theme::status_pill(ui, theme::ERR, s.disconnected).on_hover_text(reason);
                    }
                }
                if retry {
                    self.connection = Connection::Connecting;
                    let _ = self.hid_requests.send(HidRequest::Probe);
                }
            });
        });
        ui.add_space(6.0);

        // Segmented tab control.
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let tab_width = (ui.available_width() - 8.0) / 3.0;
            let tabs = [
                (Page::Settings, s.tab_settings),
                (Page::Tester, s.tab_tester),
                (Page::Language, s.tab_language),
            ];
            for (page, label) in tabs {
                let selected = self.page == page;
                let text = egui::RichText::new(label).color(if selected {
                    egui::Color32::WHITE
                } else {
                    theme::MUTED
                });
                let button = egui::Button::new(text)
                    .fill(if selected { theme::ACCENT_DIM } else { theme::CARD })
                    .min_size(egui::vec2(tab_width, 30.0));
                if ui.add(button).clicked() {
                    self.page = page;
                    if page == Page::Tester {
                        self.start_tester(ui.ctx());
                    }
                }
            }
        });
    }

    fn settings_page(&mut self, ui: &mut egui::Ui) {
        let s = self.tr();
        theme::card(ui, s.card_general, |ui| {
            theme::toggle_row(
                ui,
                &mut self.config.enabled,
                s.enabled,
                s.enabled_hint,
            );
            #[cfg(target_os = "windows")]
            {
                ui.separator();
                theme::toggle_row(
                    ui,
                    &mut self.config.only_mx_master,
                    s.only_mx,
                    s.only_mx_hint,
                );
            }
            #[cfg(target_os = "windows")]
            {
                ui.separator();
                if theme::toggle_row(
                    ui,
                    &mut self.config.autostart,
                    s.autostart,
                    s.autostart_hint,
                ) {
                    if let Err(e) = crate::autostart::set_autostart(self.config.autostart) {
                        self.problem = Some((Problem::Autostart, format!("{e:#}")));
                    }
                }
            }
        });

        let mut test_waveform = None;
        theme::card(ui, s.card_buttons, |ui| {
            let supported_mask = self.supported_mask;
            // Fixed label column, combo boxes take whatever width is left.
            const LABEL_WIDTH: f32 = 130.0;
            const PLAY_WIDTH: f32 = 36.0;
            const ROW_HEIGHT: f32 = 28.0;
            let gap = ui.spacing().item_spacing.x;
            let combo_width =
                (ui.available_width() - LABEL_WIDTH - PLAY_WIDTH - 2.0 * gap).max(110.0);
            let rows: [(&str, &mut Option<Waveform>); 5] = [
                (s.button_left, &mut self.config.left),
                (s.button_right, &mut self.config.right),
                (s.button_middle, &mut self.config.middle),
                (s.button_forward, &mut self.config.forward),
                (s.button_back, &mut self.config.back),
            ];
            for (index, (label, selected)) in rows.into_iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(LABEL_WIDTH, ROW_HEIGHT),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            // Reserve the whole column so all combos line up.
                            ui.set_min_width(LABEL_WIDTH);
                            ui.add(egui::Label::new(label).truncate());
                        },
                    );
                    egui::ComboBox::from_id_salt(("mapping", index))
                        .width(combo_width)
                        .selected_text(selected.map_or(s.waveform_none.to_owned(), pretty))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(selected, None, s.waveform_none);
                            for &waveform in Waveform::ALL {
                                if waveform.is_advertised_by(supported_mask) {
                                    ui.selectable_value(selected, Some(waveform), pretty(waveform));
                                }
                            }
                        });
                    let play = egui::Button::new("▶").min_size(egui::vec2(PLAY_WIDTH, ROW_HEIGHT));
                    let button = ui.add_enabled(selected.is_some(), play);
                    if button.on_hover_text(s.play_hint).clicked() {
                        test_waveform = *selected;
                    }
                });
            }
        });
        if let Some(w) = test_waveform {
            let _ = self.hid_requests.send(HidRequest::Play(w));
        }

        theme::card(ui, s.card_advanced, |ui| {
            // Label and hint above a full-width slider: fits any language.
            let slider_row = |ui: &mut egui::Ui, label: &str, hint: &str| {
                ui.spacing_mut().item_spacing.y = 2.0;
                ui.label(label);
                ui.label(egui::RichText::new(hint).small().color(theme::MUTED));
                ui.spacing_mut().item_spacing.y = 8.0;
                // Leave room for the value box next to the slider.
                ui.spacing_mut().slider_width = (ui.available_width() - 80.0).max(120.0);
            };
            slider_row(ui, s.cooldown, s.cooldown_hint);
            ui.add(egui::Slider::new(&mut self.config.cooldown_ms, 0..=500).suffix(" ms"));
            ui.add_space(4.0);
            slider_row(ui, s.queue, s.queue_hint);
            ui.add(egui::Slider::new(&mut self.config.queue_size, 1..=10));
        });

        #[cfg(target_os = "macos")]
        theme::card(ui, s.card_permissions, |ui| {
            ui.label(
                egui::RichText::new(s.permissions_hint)
                    .small()
                    .color(theme::MUTED),
            );
            ui.horizontal(|ui| {
                if ui.button(s.accessibility).clicked() {
                    let _ = std::process::Command::new("open")
                        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
                        .spawn();
                }
                if ui.button(s.input_monitoring).clicked() {
                    let _ = std::process::Command::new("open")
                        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent")
                        .spawn();
                }
            });
        });
    }

    fn tester_page(&mut self, ui: &mut egui::Ui) {
        let s = self.tr();
        theme::card(ui, s.tester_title, |ui| {
            ui.label(egui::RichText::new(s.tester_hint).color(theme::MUTED));
        });
        theme::card(ui, s.recent_clicks, |ui| {
            if self.tester_log.is_empty() {
                ui.label(egui::RichText::new(s.waiting_for_click).italics().color(theme::MUTED));
                return;
            }
            for entry in &self.tester_log {
                use crate::input::ClickSource;
                let (dot, source) = match entry.click.source {
                    ClickSource::TargetMouse => (theme::OK, s.source_mx),
                    ClickSource::OtherDevice => (theme::MUTED, s.source_other),
                    ClickSource::Unknown => (theme::OK, ""),
                };
                let button = match entry.click.button {
                    MouseButton::Left => s.button_left,
                    MouseButton::Right => s.button_right,
                    MouseButton::Middle => s.button_middle,
                    MouseButton::Forward => s.button_forward,
                    MouseButton::Back => s.button_back,
                };
                ui.horizontal(|ui| {
                    theme::dot(ui, dot);
                    ui.label(egui::RichText::new(button).strong());
                    if !source.is_empty() {
                        ui.label(egui::RichText::new(source).small().color(theme::MUTED));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let secs = entry.at.elapsed().as_secs();
                        ui.label(
                            egui::RichText::new(format!("{secs} {}", s.seconds_ago))
                                .small()
                                .color(theme::MUTED),
                        );
                    });
                });
            }
            ui.add_space(4.0);
            if ui.button(s.clear).clicked() {
                self.tester_log.clear();
            }
        });
    }

    fn language_page(&mut self, ui: &mut egui::Ui) {
        let s = self.tr();
        theme::card(ui, s.language_title, |ui| {
            ui.label(egui::RichText::new(s.language_hint).small().color(theme::MUTED));
            ui.add_space(4.0);
            let auto_label = format!("{} — {}", s.language_auto, Language::system().native_name());
            let mut choice = self.config.language;
            let options = std::iter::once((None, auto_label)).chain(
                Language::ALL
                    .into_iter()
                    .map(|language| (Some(language), language.native_name().to_owned())),
            );
            for (value, label) in options {
                let selected = choice == value;
                if theme::choice_row(ui, selected, &label).clicked() {
                    choice = value;
                }
            }
            if choice != self.config.language {
                self.set_language(choice);
            }
        });
    }

    /// The language is saved right away, independently of other unsaved
    /// edits, so the tray menu follows it too.
    fn set_language(&mut self, language: Option<Language>) {
        self.config.language = language;
        let mut on_disk = self.saved.clone();
        on_disk.language = language;
        match on_disk.save() {
            Ok(()) => self.saved = on_disk,
            Err(e) => self.problem = Some((Problem::Save, format!("{e:#}"))),
        }
    }

    /// Banner for problems that do not belong to a footer button.
    fn problem_banner(&mut self, ui: &mut egui::Ui) {
        let Some((kind, detail)) = &self.problem else {
            return;
        };
        let s = self.tr();
        let title = match kind {
            Problem::Load => s.load_failed,
            Problem::Save => s.save_failed,
            Problem::Listener => s.listener_failed,
            Problem::Autostart => s.autostart_failed,
        };
        let mut dismissed = false;
        egui::Frame::none()
            .fill(theme::ERR.gamma_multiply(0.12))
            .rounding(egui::Rounding::same(10.0))
            .inner_margin(egui::Margin::symmetric(14.0, 10.0))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        ui.label(egui::RichText::new(title).color(theme::ERR).strong());
                        ui.label(egui::RichText::new(detail.as_str()).small().color(theme::MUTED));
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // "×" exists in the default font, "✕" does not.
                        let close = egui::Button::new(
                            egui::RichText::new("×").size(20.0).color(theme::MUTED),
                        )
                        .frame(false);
                        dismissed = ui.add(close).clicked();
                    });
                });
            });
        ui.add_space(4.0);
        if dismissed {
            self.problem = None;
        }
    }

    /// Cancel + Save on the left, Restart on the right; the buttons carry
    /// all feedback (active = something to do, label = outcome).
    fn footer(&mut self, ui: &mut egui::Ui) {
        use theme::Tone;
        let s = self.tr();
        let now = Instant::now();
        let dirty = self.config != self.saved;

        // A new edit replaces the previous save outcome.
        if dirty && matches!(self.save_state, ButtonState::Done(_)) {
            self.save_state = ButtonState::Idle;
        }
        for state in [&mut self.save_state, &mut self.restart_state] {
            if matches!(state, ButtonState::Done(until) if *until <= now) {
                *state = ButtonState::Idle;
            }
        }

        let (save_label, save_tone, save_hint) = match &self.save_state {
            ButtonState::Done(_) => (s.saved_done, Tone::Success, None),
            ButtonState::Failed(e) => (s.failed, Tone::Failure, Some(e.as_str())),
            _ if dirty => (s.save, Tone::Primary, None),
            _ => (s.save, Tone::Inactive, None),
        };
        let (restart_label, restart_tone, restart_hint) = match &self.restart_state {
            ButtonState::Busy => (s.restarting, Tone::Inactive, s.restart_hint),
            ButtonState::Done(_) => (s.restart_done, Tone::Success, s.restart_hint),
            ButtonState::Failed(e) => (s.failed, Tone::Failure, e.as_str()),
            ButtonState::Idle => (s.restart, Tone::Secondary, s.restart_hint),
        };

        let mut save_clicked = false;
        let mut cancel_clicked = false;
        let mut restart_clicked = false;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let cancel_tone = if dirty { Tone::Secondary } else { Tone::Inactive };
            cancel_clicked = theme::footer_button(ui, s.cancel, cancel_tone, 96.0).clicked();
            let save = theme::footer_button(ui, save_label, save_tone, 110.0);
            save_clicked = match save_hint {
                Some(hint) => save.on_hover_text(hint).clicked(),
                None => save.clicked(),
            };
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                restart_clicked = theme::footer_button(ui, restart_label, restart_tone, 150.0)
                    .on_hover_text(restart_hint)
                    .clicked();
            });
        });

        if cancel_clicked {
            self.config = self.saved.clone();
            self.save_state = ButtonState::Idle;
        }
        if save_clicked {
            self.save_state = match self.config.save() {
                Ok(()) => {
                    self.saved = self.config.clone();
                    ButtonState::Done(now + DONE_LABEL_FOR)
                }
                Err(e) => ButtonState::Failed(format!("{e:#}")),
            };
        }
        if restart_clicked {
            self.restart_service(ui.ctx());
        }

        // Wake up once to turn a "Done!" label back into the normal one.
        let next_reset = [&self.save_state, &self.restart_state]
            .into_iter()
            .filter_map(|state| match state {
                ButtonState::Done(until) => Some(until.saturating_duration_since(now)),
                _ => None,
            })
            .min();
        if let Some(after) = next_reset {
            ui.ctx().request_repaint_after(after);
        }
    }

    /// Restarts the background service without blocking the window.
    fn restart_service(&mut self, ctx: &egui::Context) {
        let (tx, rx) = mpsc::channel();
        self.restart_result = Some(rx);
        self.restart_state = ButtonState::Busy;
        let ctx = ctx.clone();
        thread::spawn(move || {
            let _ = tx.send(restart_background().map_err(|e| format!("{e:#}")));
            ctx.request_repaint();
        });
    }
}

#[cfg(target_os = "windows")]
fn restart_background() -> Result<()> {
    crate::tray::restart_background()
}

/// macOS: the background service is the LaunchAgent installed by install.sh.
#[cfg(not(target_os = "windows"))]
fn restart_background() -> Result<()> {
    let uid = std::process::Command::new("id").arg("-u").output()?;
    let target = format!("gui/{}/com.local.mx4-haptic", String::from_utf8_lossy(&uid.stdout).trim());
    let status = std::process::Command::new("launchctl")
        .args(["kickstart", "-k", &target])
        .status()?;
    anyhow::ensure!(status.success(), "launchctl kickstart failed (is the LaunchAgent installed?)");
    Ok(())
}

impl eframe::App for ConfigApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_background();

        let margin = egui::Margin::symmetric(16.0, 12.0);
        egui::TopBottomPanel::top("header")
            .frame(egui::Frame::none().fill(theme::BG).inner_margin(margin))
            .show_separator_line(false)
            .show(ctx, |ui| self.header(ui));

        egui::TopBottomPanel::bottom("footer")
            .frame(
                egui::Frame::none()
                    .fill(theme::CARD)
                    .stroke(egui::Stroke::new(1.0, theme::CARD_BORDER))
                    .inner_margin(margin),
            )
            .show_separator_line(false)
            .show(ctx, |ui| self.footer(ui));

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(theme::BG).inner_margin(egui::Margin {
                left: 16.0,
                right: 16.0,
                top: 4.0,
                bottom: 12.0,
            }))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                    self.problem_banner(ui);
                    match self.page {
                        Page::Settings => self.settings_page(ui),
                        Page::Tester => self.tester_page(ui),
                        Page::Language => self.language_page(ui),
                    }
                });
            });

        // Keep the "N s temu" labels fresh while the tester is visible,
        // but only once per second rather than every frame.
        if self.page == Page::Tester && !self.tester_log.is_empty() {
            ctx.request_repaint_after(Duration::from_secs(1));
        }
    }
}
