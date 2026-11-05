//! Interfaccia grafica: pannelli vintage con manopole disegnate a mano (egui).

use crate::{McsParams, ModeP, Shared};
use nih_plug::prelude::*;
use nih_plug_egui::egui::{
    self, Align2, Color32, ComboBox, CornerRadius, FontId, Margin, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2,
};
use nih_plug_egui::resizable_window::ResizableWindow;
use nih_plug_egui::EguiState;
use std::f32::consts::PI;
use std::sync::atomic::Ordering::Relaxed;
use std::time::Duration;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Look {
    Mix,
    Metal,
    Dark,
}

pub struct GuiState {
    pub look: Look,
    /// Set (A=0, B=1) in modifica per ciascun modulo: Tone, Limiter, Filter, EQ finale, Clipper.
    pub set_sel: [usize; 5],
}

impl Default for GuiState {
    fn default() -> Self {
        GuiState { look: Look::Mix, set_sel: [0; 5] }
    }
}

#[derive(Clone, Copy)]
struct Theme {
    panel: Color32,
    edge: Color32,
    text: Color32,
    dim: Color32,
    metal: bool,
}

const METAL: Theme = Theme {
    panel: Color32::from_rgb(178, 178, 172),
    edge: Color32::from_rgb(96, 96, 92),
    text: Color32::from_rgb(28, 28, 30),
    dim: Color32::from_rgb(70, 70, 68),
    metal: true,
};
const DARK: Theme = Theme {
    panel: Color32::from_rgb(38, 38, 40),
    edge: Color32::from_rgb(14, 14, 16),
    text: Color32::from_rgb(228, 218, 192),
    dim: Color32::from_rgb(150, 144, 128),
    metal: false,
};

fn theme_for(look: Look, index: usize) -> Theme {
    match look {
        Look::Metal => METAL,
        Look::Dark => DARK,
        Look::Mix => {
            if index % 2 == 0 {
                METAL
            } else {
                DARK
            }
        }
    }
}

#[derive(Clone, Copy)]
struct KnobStyle {
    skirt: Color32,
    cap: Color32,
    pointer: Color32,
    ticks: bool,
}

const K_INPUT: KnobStyle = KnobStyle { skirt: Color32::from_rgb(20, 20, 20), cap: Color32::from_rgb(44, 44, 46), pointer: Color32::from_rgb(240, 240, 235), ticks: true };
const K_TONE: KnobStyle = KnobStyle { skirt: Color32::from_rgb(40, 38, 36), cap: Color32::from_rgb(222, 210, 180), pointer: Color32::from_rgb(40, 30, 20), ticks: true };
const K_LIMITER: KnobStyle = KnobStyle { skirt: Color32::from_rgb(30, 30, 30), cap: Color32::from_rgb(176, 48, 40), pointer: Color32::from_rgb(250, 240, 230), ticks: true };
const K_FILTER: KnobStyle = KnobStyle { skirt: Color32::from_rgb(30, 30, 34), cap: Color32::from_rgb(52, 92, 150), pointer: Color32::from_rgb(240, 240, 240), ticks: true };
const K_EQF: KnobStyle = KnobStyle { skirt: Color32::from_rgb(60, 60, 58), cap: Color32::from_rgb(150, 152, 150), pointer: Color32::from_rgb(20, 20, 20), ticks: false };
const K_CLIP: KnobStyle = KnobStyle { skirt: Color32::from_rgb(28, 28, 28), cap: Color32::from_rgb(214, 128, 36), pointer: Color32::from_rgb(25, 20, 15), ticks: true };
const K_OUT: KnobStyle = KnobStyle { skirt: Color32::from_rgb(24, 24, 24), cap: Color32::from_rgb(206, 208, 210), pointer: Color32::from_rgb(10, 10, 10), ticks: true };

fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

pub fn setup(ctx: &egui::Context) {
    ctx.set_visuals(egui::Visuals::dark());
}

// ---------------------------------------------------------------- widget

fn knob(ui: &mut Ui, setter: &ParamSetter, p: &FloatParam, label: &str, size: f32, st: KnobStyle, th: Theme) {
    let w = size + 20.0;
    let h = size + 40.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, h), Sense::click_and_drag());

    if resp.drag_started() {
        setter.begin_set_parameter(p);
    }
    if resp.dragged() {
        let fine = ui.input(|i| i.modifiers.shift);
        let k = if fine { 0.0008 } else { 0.006 };
        let v = (p.unmodulated_normalized_value() - resp.drag_delta().y * k).clamp(0.0, 1.0);
        setter.set_parameter_normalized(p, v);
    }
    if resp.drag_stopped() {
        setter.end_set_parameter(p);
    }
    if resp.double_clicked() {
        setter.begin_set_parameter(p);
        setter.set_parameter_normalized(p, p.default_normalized_value());
        setter.end_set_parameter(p);
    }

    let norm = p.unmodulated_normalized_value();
    let painter = ui.painter_at(rect);
    painter.text(Pos2::new(rect.center().x, rect.top() + 8.0), Align2::CENTER_CENTER, label, FontId::proportional(11.0), th.dim);

    let c = Pos2::new(rect.center().x, rect.top() + 18.0 + size * 0.5);
    let r = size * 0.5;
    let start = 0.75 * PI; // 135 gradi
    let sweep = 1.5 * PI; // 270 gradi

    if st.ticks {
        for i in 0..=10 {
            let a = start + sweep * (i as f32 / 10.0);
            let d = Vec2::new(a.cos(), a.sin());
            painter.line_segment([c + d * (r + 3.0), c + d * (r + if i % 5 == 0 { 8.0 } else { 6.0 })], Stroke::new(1.2, th.dim));
        }
    }
    // ombra, gonna, cappuccio con bordo e riflesso
    painter.circle_filled(c + Vec2::new(2.0, 3.0), r, Color32::from_black_alpha(70));
    painter.circle_filled(c, r, st.skirt);
    painter.circle_stroke(c, r, Stroke::new(1.0, Color32::from_black_alpha(160)));
    let cr = r * 0.80;
    painter.circle_filled(c, cr, mix(st.cap, Color32::BLACK, 0.35));
    painter.circle_filled(c, cr * 0.97, st.cap);
    painter.circle_filled(c + Vec2::new(-cr * 0.18, -cr * 0.22), cr * 0.62, mix(st.cap, Color32::WHITE, 0.14));
    painter.circle_stroke(c, cr, Stroke::new(1.0, mix(st.cap, Color32::BLACK, 0.5)));
    let a = start + sweep * norm;
    let d = Vec2::new(a.cos(), a.sin());
    painter.line_segment([c + d * (cr * 0.18), c + d * (cr * 0.92)], Stroke::new((size * 0.06).max(2.5), st.pointer));

    let txt = p.normalized_value_to_string(norm, true);
    painter.text(Pos2::new(rect.center().x, rect.bottom() - 7.0), Align2::CENTER_CENTER, txt, FontId::monospace(11.0), th.text);
}

fn toggle(ui: &mut Ui, setter: &ParamSetter, p: &BoolParam, label: &str, th: Theme, led: Color32) {
    let font = FontId::proportional(11.5);
    let galley = ui.painter().layout_no_wrap(label.to_string(), font.clone(), th.text);
    let size = Vec2::new(galley.size().x + 34.0, 22.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let on = p.value();
    if resp.clicked() {
        setter.begin_set_parameter(p);
        setter.set_parameter(p, !on);
        setter.end_set_parameter(p);
    }
    let bg = if th.metal { Color32::from_rgb(150, 150, 146) } else { Color32::from_rgb(56, 56, 58) };
    ui.painter().rect(rect, CornerRadius::same(4), bg, Stroke::new(1.0, th.edge), StrokeKind::Inside);
    let lc = Pos2::new(rect.left() + 11.0, rect.center().y);
    ui.painter().circle_filled(lc, 5.0, if on { led } else { Color32::from_rgb(52, 30, 28) });
    ui.painter().circle_stroke(lc, 5.0, Stroke::new(1.0, Color32::from_black_alpha(160)));
    ui.painter().text(Pos2::new(rect.left() + 22.0, rect.center().y), Align2::LEFT_CENTER, label, font, th.text);
}

fn dropdown<T: Enum + PartialEq + Copy + 'static>(ui: &mut Ui, setter: &ParamSetter, p: &EnumParam<T>, salt: &str, width: f32) {
    let names = T::variants();
    let cur = p.value().to_index();
    ComboBox::from_id_salt(salt).width(width).selected_text(names[cur]).show_ui(ui, |ui| {
        for (i, n) in names.iter().enumerate() {
            if ui.selectable_label(i == cur, *n).clicked() {
                setter.begin_set_parameter(p);
                setter.set_parameter(p, T::from_index(i));
                setter.end_set_parameter(p);
            }
        }
    });
}

/// Pannello di un modulo. `content` riceve l'indice del set (A o B) in modifica.
fn module(
    ui: &mut Ui,
    setter: &ParamSetter,
    state: &mut GuiState,
    idx: usize,
    title: &str,
    on: &BoolParam,
    mode: Option<&EnumParam<ModeP>>,
    sel_slot: Option<usize>,
    content: impl FnOnce(&mut Ui, usize, Theme),
) {
    let th = theme_for(state.look, idx);
    let mut set = 0;
    if let (Some(m), Some(slot)) = (mode, sel_slot) {
        if m.value() == ModeP::St {
            state.set_sel[slot] = 0;
        }
        set = state.set_sel[slot];
    }
    let frame = egui::Frame::new()
        .fill(th.panel)
        .stroke(Stroke::new(2.0, th.edge))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::same(10));
    let out = frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(title).strong().size(15.0).color(th.text));
            toggle(ui, setter, on, "ON", th, Color32::from_rgb(90, 220, 90));
            if let (Some(m), Some(slot)) = (mode, sel_slot) {
                let cur = m.value();
                let label = match cur {
                    ModeP::St => "ST",
                    ModeP::Duo => "DUO",
                    ModeP::Ms => "MS",
                };
                if ui.button(egui::RichText::new(label).strong()).clicked() {
                    let next = match cur {
                        ModeP::St => ModeP::Duo,
                        ModeP::Duo => ModeP::Ms,
                        ModeP::Ms => ModeP::St,
                    };
                    setter.begin_set_parameter(m);
                    setter.set_parameter(m, next);
                    setter.end_set_parameter(m);
                    state.set_sel[slot] = 0;
                }
                if cur != ModeP::St {
                    let (a, b) = if cur == ModeP::Duo { ("L", "R") } else { ("MID", "SIDE") };
                    if ui.selectable_label(state.set_sel[slot] == 0, a).clicked() {
                        state.set_sel[slot] = 0;
                    }
                    if ui.selectable_label(state.set_sel[slot] == 1, b).clicked() {
                        state.set_sel[slot] = 1;
                    }
                }
            }
        });
        ui.add_space(4.0);
        content(ui, set, th);
    });
    let r = out.response.rect;
    for (dx, dy) in [(8.0, 8.0), (-8.0, 8.0), (8.0, -8.0), (-8.0, -8.0)] {
        let p = Pos2::new(if dx > 0.0 { r.left() } else { r.right() } + dx, if dy > 0.0 { r.top() } else { r.bottom() } + dy);
        ui.painter().circle_filled(p, 3.0, mix(th.panel, Color32::BLACK, 0.45));
        ui.painter().line_segment([p + Vec2::new(-2.0, 1.0), p + Vec2::new(2.0, -1.0)], Stroke::new(1.0, Color32::from_black_alpha(160)));
    }
    ui.add_space(8.0);
}

// ---------------------------------------------------------------- analisi

fn db_norm(db: f32) -> f32 {
    ((db + 60.0) / 60.0).clamp(0.0, 1.0)
}

fn analysis_panel(ui: &mut Ui, shared: &Shared, th: Theme) {
    let (m, s, i) = (shared.momentary.load(Relaxed), shared.short_term.load(Relaxed), shared.integrated.load(Relaxed));
    let fmt = |v: f32| if v < -100.0 { "-inf".to_string() } else { format!("{:.1}", v) };
    ui.horizontal(|ui| {
        for (name, v) in [("MOMENTARY", m), ("SHORT", s), ("INTEGRATED", i)] {
            let (rect, _) = ui.allocate_exact_size(Vec2::new(104.0, 56.0), Sense::hover());
            let p = ui.painter();
            p.rect(rect, CornerRadius::same(4), Color32::from_rgb(14, 20, 16), Stroke::new(1.0, th.edge), StrokeKind::Inside);
            p.text(Pos2::new(rect.center().x, rect.top() + 11.0), Align2::CENTER_CENTER, name, FontId::proportional(10.0), Color32::from_rgb(120, 170, 130));
            p.text(Pos2::new(rect.center().x, rect.center().y + 8.0), Align2::CENTER_CENTER, format!("{} LUFS", fmt(v)), FontId::monospace(15.0), Color32::from_rgb(150, 255, 160));
        }
    });
    ui.add_space(6.0);

    // livelli picco e RMS, riduzione di guadagno
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width().min(380.0), 70.0), Sense::hover());
    let p = ui.painter();
    let rows = [("L", shared.peak_l.load(Relaxed), shared.rms_l.load(Relaxed)), ("R", shared.peak_r.load(Relaxed), shared.rms_r.load(Relaxed))];
    for (k, (name, peak, rms)) in rows.iter().enumerate() {
        let y = rect.top() + 6.0 + k as f32 * 22.0;
        p.text(Pos2::new(rect.left() + 8.0, y + 8.0), Align2::CENTER_CENTER, *name, FontId::proportional(11.0), th.text);
        let bar = Rect::from_min_size(Pos2::new(rect.left() + 20.0, y), Vec2::new(rect.width() - 28.0, 16.0));
        p.rect_filled(bar, CornerRadius::same(3), Color32::from_rgb(18, 18, 20));
        let pk_db = 20.0 * peak.max(1e-6).log10();
        let rms_w = bar.width() * db_norm(*rms);
        let pk_w = bar.width() * db_norm(pk_db);
        p.rect_filled(Rect::from_min_size(bar.min, Vec2::new(rms_w, bar.height())), CornerRadius::same(3), Color32::from_rgb(60, 170, 80));
        let col = if pk_db > -0.5 { Color32::from_rgb(240, 60, 50) } else { Color32::from_rgb(235, 200, 70) };
        p.line_segment([Pos2::new(bar.left() + pk_w, bar.top()), Pos2::new(bar.left() + pk_w, bar.bottom())], Stroke::new(3.0, col));
        p.text(Pos2::new(bar.right() - 4.0, bar.center().y), Align2::RIGHT_CENTER, format!("{:.1} dBFS", pk_db), FontId::monospace(10.0), Color32::from_rgb(200, 200, 200));
    }
    let gr = shared.gr.load(Relaxed);
    p.text(Pos2::new(rect.left() + 8.0, rect.bottom() - 6.0), Align2::LEFT_CENTER, format!("Riduzione limiter: {:.1} dB", gr), FontId::proportional(11.0), th.text);

    // correlazione di fase
    ui.add_space(4.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width().min(380.0), 34.0), Sense::hover());
    let p = ui.painter();
    let bar = Rect::from_min_size(Pos2::new(rect.left() + 8.0, rect.top() + 4.0), Vec2::new(rect.width() - 16.0, 14.0));
    p.rect_filled(bar, CornerRadius::same(3), Color32::from_rgb(18, 18, 20));
    let corr = shared.corr.load(Relaxed).clamp(-1.0, 1.0);
    let x0 = bar.center().x;
    let x1 = x0 + corr * bar.width() * 0.5;
    let col = if corr < 0.0 { Color32::from_rgb(230, 70, 60) } else { Color32::from_rgb(80, 190, 110) };
    p.rect_filled(Rect::from_x_y_ranges(x0.min(x1)..=x0.max(x1), bar.y_range()), CornerRadius::same(2), col);
    p.line_segment([Pos2::new(x0, bar.top() - 2.0), Pos2::new(x0, bar.bottom() + 2.0)], Stroke::new(1.0, Color32::WHITE));
    p.text(Pos2::new(rect.left() + 8.0, rect.bottom() - 6.0), Align2::LEFT_CENTER, "-1", FontId::proportional(10.0), th.dim);
    p.text(Pos2::new(rect.center().x, rect.bottom() - 6.0), Align2::CENTER_CENTER, format!("correlazione {:+.2}", corr), FontId::proportional(11.0), th.text);
    p.text(Pos2::new(rect.right() - 8.0, rect.bottom() - 6.0), Align2::RIGHT_CENTER, "+1", FontId::proportional(10.0), th.dim);

    // goniometro
    ui.add_space(6.0);
    let side = ui.available_width().min(220.0).max(120.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(side), Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, CornerRadius::same(6), Color32::from_rgb(10, 16, 12));
    let c = rect.center();
    let half = side * 0.46;
    let grid = Stroke::new(1.0, Color32::from_rgb(40, 70, 50));
    p.line_segment([c - Vec2::new(half, 0.0), c + Vec2::new(half, 0.0)], grid);
    p.line_segment([c - Vec2::new(0.0, half), c + Vec2::new(0.0, half)], grid);
    p.line_segment([c - Vec2::new(half, half) * 0.7071, c + Vec2::new(half, half) * 0.7071], grid);
    p.line_segment([c + Vec2::new(-half, half) * 0.7071, c + Vec2::new(half, -half) * 0.7071], grid);
    p.text(rect.center_top() + Vec2::new(0.0, 8.0), Align2::CENTER_CENTER, "M", FontId::proportional(10.0), Color32::from_rgb(120, 170, 130));
    p.text(rect.left_top() + Vec2::new(10.0, 10.0), Align2::CENTER_CENTER, "L", FontId::proportional(10.0), Color32::from_rgb(120, 170, 130));
    p.text(rect.right_top() + Vec2::new(-10.0, 10.0), Align2::CENTER_CENTER, "R", FontId::proportional(10.0), Color32::from_rgb(120, 170, 130));
    if let Some(pts) = shared.scope.try_lock() {
        for (l, r) in pts.iter() {
            let x = (l - r) * 0.7071;
            let y = (l + r) * 0.7071;
            let pos = Pos2::new(c.x + x * half * 1.4, c.y - y * half * 1.4);
            if rect.contains(pos) {
                p.circle_filled(pos, 1.1, Color32::from_rgba_unmultiplied(120, 255, 150, 150));
            }
        }
    }
}

// ---------------------------------------------------------------- finestra

pub fn draw(ctx: &egui::Context, setter: &ParamSetter, state: &mut GuiState, params: &McsParams, shared: &Shared, egui_state: &EguiState) {
    ctx.request_repaint_after(Duration::from_millis(33));
    ResizableWindow::new("mcs-window").min_size(Vec2::new(420.0, 400.0)).show(ctx, egui_state, |ui| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("MULTI CHANNEL STRIP").strong().size(20.0).color(Color32::from_rgb(230, 220, 190)));
            ui.add_space(12.0);
            for (name, l) in [("Misto", Look::Mix), ("Metallo", Look::Metal), ("Scuro", Look::Dark)] {
                if ui.selectable_label(state.look == l, name).clicked() {
                    state.look = l;
                }
            }
            ui.add_space(12.0);
            ui.label("Oversampling");
            dropdown(ui, setter, &params.os, "os", 70.0);
            if ui.button("Reset LUFS").clicked() {
                shared.reset_lufs.store(true, Relaxed);
            }
        });
        ui.add_space(6.0);
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let wide = ui.available_width() >= 820.0;
            if wide {
                ui.columns(2, |cols| {
                    left_column(&mut cols[0], setter, state, params);
                    right_column(&mut cols[1], setter, state, params, shared);
                });
            } else {
                left_column(ui, setter, state, params);
                right_column(ui, setter, state, params, shared);
            }
        });
    });
}

fn left_column(ui: &mut Ui, setter: &ParamSetter, state: &mut GuiState, p: &McsParams) {
    // INPUT
    let th = theme_for(state.look, 0);
    let frame = egui::Frame::new().fill(th.panel).stroke(Stroke::new(2.0, th.edge)).corner_radius(CornerRadius::same(6)).inner_margin(Margin::same(10));
    frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(egui::RichText::new("INPUT").strong().size(15.0).color(th.text));
        ui.horizontal_wrapped(|ui| {
            knob(ui, setter, &p.in_trim, "TRIM", 56.0, K_INPUT, th);
            knob(ui, setter, &p.in_tape, "NASTRO", 56.0, K_INPUT, th);
            ui.vertical(|ui| {
                toggle(ui, setter, &p.in_pol_l, "POL L", th, Color32::from_rgb(240, 190, 60));
                toggle(ui, setter, &p.in_pol_r, "POL R", th, Color32::from_rgb(240, 190, 60));
                toggle(ui, setter, &p.in_swap, "L/R SWAP", th, Color32::from_rgb(240, 190, 60));
                toggle(ui, setter, &p.in_phase, "FASE", th, Color32::from_rgb(240, 190, 60));
            });
        });
    });
    ui.add_space(8.0);

    // TONE
    module(ui, setter, state, 1, "TONE", &p.tone_on, Some(&p.tone_mode), Some(0), |ui, s, th| {
        let names = ["LF", "LMF", "HMF", "HF"];
        ui.horizontal_wrapped(|ui| {
            for b in 0..4 {
                let bp = &p.sets[s].bands[b];
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(names[b]).strong().color(th.text));
                        toggle(ui, setter, &bp.on, "ON", th, Color32::from_rgb(90, 220, 90));
                    });
                    toggle(ui, setter, &bp.solo, "SOLO", th, Color32::from_rgb(240, 190, 60));
                    knob(ui, setter, &bp.gain, "GAIN", 68.0, K_TONE, th);
                    knob(ui, setter, &bp.freq, "FREQ", 52.0, K_TONE, th);
                    ui.label(egui::RichText::new("FORMA").size(10.5).color(th.dim));
                    dropdown(ui, setter, &bp.shape, &format!("shape{}_{}", b, s), 110.0);
                });
                ui.add_space(6.0);
            }
        });
    });

    // LIMITER
    module(ui, setter, state, 2, "LIMITER", &p.lim_on, Some(&p.lim_mode), Some(1), |ui, s, th| {
        let q = &p.sets[s];
        ui.horizontal_wrapped(|ui| {
            knob(ui, setter, &q.lim_in, "INPUT", 64.0, K_LIMITER, th);
            knob(ui, setter, &q.lim_thr, "SOGLIA", 64.0, K_LIMITER, th);
            knob(ui, setter, &q.lim_rel, "RILASCIO", 64.0, K_LIMITER, th);
            ui.vertical(|ui| {
                toggle(ui, setter, &q.lim_auto, "AUTO", th, Color32::from_rgb(90, 220, 90));
                ui.label(egui::RichText::new("SIDECHAIN").size(10.5).color(th.dim));
                toggle(ui, setter, &q.sc_hp, "HP", th, Color32::from_rgb(240, 190, 60));
                toggle(ui, setter, &q.sc_bell, "PRESENZA", th, Color32::from_rgb(240, 190, 60));
                toggle(ui, setter, &q.sc_lp, "LP", th, Color32::from_rgb(240, 190, 60));
            });
        });
    });
}

fn right_column(ui: &mut Ui, setter: &ParamSetter, state: &mut GuiState, p: &McsParams, shared: &Shared) {
    module(ui, setter, state, 3, "FILTER", &p.flt_on, Some(&p.flt_mode), Some(2), |ui, s, th| {
        let q = &p.sets[s];
        ui.horizontal_wrapped(|ui| {
            knob(ui, setter, &q.pres_db, "PRESENZA", 64.0, K_FILTER, th);
            knob(ui, setter, &q.pres_f, "FREQ", 64.0, K_FILTER, th);
        });
    });

    module(ui, setter, state, 4, "EQ FINALE", &p.eqf_on, Some(&p.eqf_mode), Some(3), |ui, s, th| {
        let q = &p.sets[s];
        ui.horizontal_wrapped(|ui| {
            ui.vertical(|ui| {
                knob(ui, setter, &q.hp_f, "PASSA-ALTI", 60.0, K_EQF, th);
                dropdown(ui, setter, &q.hp_s, &format!("hps{}", s), 100.0);
            });
            ui.add_space(10.0);
            ui.vertical(|ui| {
                knob(ui, setter, &q.lp_f, "PASSA-BASSI", 60.0, K_EQF, th);
                dropdown(ui, setter, &q.lp_s, &format!("lps{}", s), 100.0);
            });
        });
    });

    module(ui, setter, state, 5, "CLIPPER", &p.clip_on, Some(&p.clip_mode), Some(4), |ui, s, th| {
        let q = &p.sets[s];
        ui.horizontal_wrapped(|ui| {
            knob(ui, setter, &q.clip_drive, "DRIVE", 64.0, K_CLIP, th);
            knob(ui, setter, &q.clip_ceil, "CEILING", 64.0, K_CLIP, th);
            knob(ui, setter, &q.clip_mix, "MIX", 64.0, K_CLIP, th);
            ui.vertical(|ui| {
                ui.label(egui::RichText::new("MODELLO").size(10.5).color(th.dim));
                dropdown(ui, setter, &q.clip_model, &format!("clipm{}", s), 110.0);
                toggle(ui, setter, &q.clip_post, "POST CLIP", th, Color32::from_rgb(240, 190, 60));
            });
        });
    });

    // OUTPUT
    let th = theme_for(state.look, 6);
    let frame = egui::Frame::new().fill(th.panel).stroke(Stroke::new(2.0, th.edge)).corner_radius(CornerRadius::same(6)).inner_margin(Margin::same(10));
    frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(egui::RichText::new("OUTPUT").strong().size(15.0).color(th.text));
        ui.horizontal_wrapped(|ui| {
            knob(ui, setter, &p.out_l, "L", 64.0, K_OUT, th);
            knob(ui, setter, &p.out_r, "R", 64.0, K_OUT, th);
            toggle(ui, setter, &p.out_link, "LINK L/R", th, Color32::from_rgb(90, 220, 90));
        });
    });
    ui.add_space(8.0);

    // ANALYSIS
    let th = theme_for(state.look, 7);
    let frame = egui::Frame::new().fill(th.panel).stroke(Stroke::new(2.0, th.edge)).corner_radius(CornerRadius::same(6)).inner_margin(Margin::same(10));
    frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(egui::RichText::new("ANALYSIS").strong().size(15.0).color(th.text));
        analysis_panel(ui, shared, th);
    });
}
