//! Interfaccia grafica: pannelli vintage con manopole disegnate a mano (egui).

use crate::{ClipP, McsParams, ModeP, Shared};
use nih_plug::prelude::*;
use nih_plug_egui::egui::{
    self, Align2, Color32, ComboBox, CornerRadius, FontId, Margin, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, UiBuilder, Vec2,
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
    pub set_sel: [usize; 6],
    /// Fattore di ingrandimento dell'interfaccia (0,6 – 1,4).
    pub zoom: f32,
}

impl Default for GuiState {
    fn default() -> Self {
        GuiState { look: Look::Mix, set_sel: [0; 6], zoom: 0.75 }
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


/// Un colore di pannello diverso per ogni unità del rack.
const PLATES: [Theme; 9] = [
    // INPUT: nero
    Theme { panel: Color32::from_rgb(28, 28, 30), edge: Color32::from_rgb(8, 8, 10), text: Color32::from_rgb(236, 228, 208), dim: Color32::from_rgb(160, 154, 140), metal: false },
    // TONE: panna
    Theme { panel: Color32::from_rgb(222, 217, 202), edge: Color32::from_rgb(110, 106, 96), text: Color32::from_rgb(30, 30, 32), dim: Color32::from_rgb(90, 88, 80), metal: true },
    // LIMITER: argento
    Theme { panel: Color32::from_rgb(178, 180, 182), edge: Color32::from_rgb(96, 98, 100), text: Color32::from_rgb(24, 24, 28), dim: Color32::from_rgb(70, 72, 74), metal: true },
    // FILTER: blu
    Theme { panel: Color32::from_rgb(34, 70, 126), edge: Color32::from_rgb(12, 28, 58), text: Color32::from_rgb(238, 242, 250), dim: Color32::from_rgb(176, 194, 222), metal: false },
    // EQ FINALE: grafite
    Theme { panel: Color32::from_rgb(70, 74, 74), edge: Color32::from_rgb(24, 26, 26), text: Color32::from_rgb(232, 226, 208), dim: Color32::from_rgb(168, 166, 154), metal: false },
    // CLIPPER: ottone dorato
    Theme { panel: Color32::from_rgb(190, 152, 64), edge: Color32::from_rgb(72, 52, 16), text: Color32::from_rgb(58, 38, 10), dim: Color32::from_rgb(104, 76, 30), metal: true },
    // OUTPUT: nero
    Theme { panel: Color32::from_rgb(22, 24, 28), edge: Color32::from_rgb(6, 6, 8), text: Color32::from_rgb(236, 228, 208), dim: Color32::from_rgb(160, 154, 140), metal: false },
    // ANALYSIS: verde scuro
    Theme { panel: Color32::from_rgb(20, 30, 26), edge: Color32::from_rgb(6, 12, 10), text: Color32::from_rgb(190, 236, 200), dim: Color32::from_rgb(120, 170, 130), metal: false },
    // COMPRESSORE: verde salvia
    Theme { panel: Color32::from_rgb(70, 104, 90), edge: Color32::from_rgb(22, 38, 30), text: Color32::from_rgb(240, 244, 232), dim: Color32::from_rgb(190, 210, 194), metal: false },
];

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
const K_COMP: KnobStyle = KnobStyle { skirt: Color32::from_rgb(24, 30, 26), cap: Color32::from_rgb(226, 222, 200), pointer: Color32::from_rgb(30, 40, 34), ticks: true };
const INNER: Theme = Theme { panel: Color32::from_rgb(38, 10, 10), edge: Color32::BLACK, text: Color32::from_rgb(244, 236, 218), dim: Color32::from_rgb(214, 186, 160), metal: false };
const K_CLIPG: KnobStyle = KnobStyle { skirt: Color32::from_rgb(10, 10, 11), cap: Color32::from_rgb(34, 34, 36), pointer: Color32::from_rgb(238, 234, 224), ticks: true };
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
    // ombra, gonna zigrinata, cappuccio con bordo e riflesso
    painter.circle_filled(c + Vec2::new(2.0, 3.0), r, Color32::from_black_alpha(80));
    painter.circle_filled(c, r, st.skirt);
    painter.circle_stroke(c, r, Stroke::new(1.0, Color32::from_black_alpha(160)));
    for i in 0..30 {
        let a = i as f32 * std::f32::consts::TAU / 30.0;
        let d = Vec2::new(a.cos(), a.sin());
        painter.line_segment([c + d * (r * 0.90), c + d * (r * 0.99)], Stroke::new(1.0, Color32::from_white_alpha(26)));
    }
    let cr = r * 0.80;
    painter.circle_filled(c, cr, mix(st.cap, Color32::BLACK, 0.35));
    painter.circle_filled(c, cr * 0.97, st.cap);
    painter.circle_filled(c + Vec2::new(-cr * 0.18, -cr * 0.22), cr * 0.62, mix(st.cap, Color32::WHITE, 0.14));
    painter.circle_stroke(c, cr, Stroke::new(1.0, mix(st.cap, Color32::BLACK, 0.5)));
    for k in 1..4 {
        painter.circle_stroke(c, cr * (0.28 + 0.2 * k as f32), Stroke::new(0.7, Color32::from_white_alpha(16)));
    }
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

/// Pulsante di un selettore a segmenti (ST / DUO / MS, L / R...): arancione quando è attivo.
fn seg_button(ui: &mut Ui, label: &str, selected: bool, w: f32, th: Theme) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w.max(20.0), 22.0), Sense::click());
    let bg = if selected {
        Color32::from_rgb(232, 150, 70)
    } else if th.metal {
        Color32::from_rgb(150, 150, 146)
    } else {
        Color32::from_rgb(56, 56, 58)
    };
    ui.painter().rect(rect, CornerRadius::same(3), bg, Stroke::new(1.0, th.edge), StrokeKind::Inside);
    let tc = if selected { Color32::from_rgb(30, 18, 8) } else { th.text };
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, label, FontId::proportional(11.5), tc);
    resp.clicked()
}

/// Una vite a taglio.
fn screw(p: &egui::Painter, c: Pos2) {
    p.circle_filled(c + Vec2::new(0.8, 1.2), 6.0, Color32::from_black_alpha(90));
    p.circle_filled(c, 6.0, Color32::from_rgb(112, 114, 116));
    p.circle_filled(c + Vec2::new(-1.0, -1.2), 4.6, Color32::from_rgb(176, 178, 180));
    p.circle_stroke(c, 6.0, Stroke::new(1.0, Color32::from_black_alpha(150)));
    p.line_segment([c + Vec2::new(-3.8, 2.0), c + Vec2::new(3.8, -2.0)], Stroke::new(1.6, Color32::from_rgb(40, 40, 42)));
}

/// Rettangolo con sfumatura verticale.
fn gradient_rect(p: &egui::Painter, r: Rect, top: Color32, bottom: Color32) {
    let mut m = egui::epaint::Mesh::default();
    m.colored_vertex(r.left_top(), top);
    m.colored_vertex(r.right_top(), top);
    m.colored_vertex(r.right_bottom(), bottom);
    m.colored_vertex(r.left_bottom(), bottom);
    m.add_triangle(0, 1, 2);
    m.add_triangle(0, 2, 3);
    p.add(egui::Shape::mesh(m));
}

/// Indicatore a lancetta con quadrante ambra. `value` tra `min` e `max`; la parte da `red_from` in poi è rossa.
fn vu_meter(ui: &mut Ui, w: f32, h: f32, caption: &str, value: f32, min: f32, max: f32, ticks: &[(f32, &str)], red_from: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, h), Sense::hover());
    let p = ui.painter_at(rect);
    p.rect(rect, CornerRadius::same(5), Color32::from_rgb(16, 16, 16), Stroke::new(2.0, Color32::from_rgb(78, 72, 64)), StrokeKind::Inside);
    let face = rect.shrink(6.0);
    gradient_rect(&p, face, Color32::from_rgb(250, 228, 160), Color32::from_rgb(214, 168, 88));
    let fp = ui.painter_at(face);
    let ink = Color32::from_rgb(44, 30, 18);
    let pivot = Pos2::new(face.center().x, face.bottom() + face.height() * 0.30);
    let rad = face.height() * 1.15;
    let a0 = -50.0f32.to_radians();
    let a1 = 50.0f32.to_radians();
    let at = |t: f32, r: f32| {
        let a = a0 + (a1 - a0) * t.clamp(0.0, 1.0);
        pivot + Vec2::new(a.sin(), -a.cos()) * r
    };
    let tn = |v: f32| (v - min) / (max - min);
    let red_t = tn(red_from);
    for (t0, t1, col) in [(0.0f32, red_t, ink), (red_t, 1.0f32, Color32::from_rgb(190, 40, 30))] {
        let mut pts = Vec::new();
        for k in 0..=36 {
            pts.push(at(t0 + (t1 - t0) * k as f32 / 36.0, rad * 0.90));
        }
        fp.add(egui::Shape::line(pts, Stroke::new(2.2, col)));
    }
    for (v, lab) in ticks {
        let t = tn(*v);
        fp.line_segment([at(t, rad * 0.83), at(t, rad * 0.90)], Stroke::new(1.4, ink));
        fp.text(at(t, rad * 0.76), Align2::CENTER_CENTER, *lab, FontId::proportional(9.0), ink);
    }
    let t = tn(value);
    fp.line_segment([at(t, rad * 0.28) + Vec2::new(1.5, 1.5), at(t, rad * 0.93) + Vec2::new(1.5, 1.5)], Stroke::new(1.6, Color32::from_black_alpha(60)));
    fp.line_segment([at(t, rad * 0.28), at(t, rad * 0.93)], Stroke::new(1.8, Color32::from_rgb(20, 14, 10)));
    fp.text(Pos2::new(face.center().x, face.bottom() - 9.0), Align2::CENTER_CENTER, caption, FontId::proportional(9.5), ink);
    fp.rect_filled(Rect::from_min_size(face.min, Vec2::new(face.width(), face.height() * 0.38)), 0.0, Color32::from_white_alpha(14));
}

/// Una unità del rack: pannello metallico con orecchie e viti, intestazione a sinistra e comandi a destra.
/// `content` riceve l'indice del set (A o B) in modifica.
fn rack_unit(
    ui: &mut Ui,
    setter: &ParamSetter,
    state: &mut GuiState,
    idx: usize,
    height: f32,
    title: &str,
    subtitle: &str,
    on: Option<&BoolParam>,
    mode: Option<&EnumParam<ModeP>>,
    sel_slot: Option<usize>,
    content: impl FnOnce(&mut Ui, usize, Theme),
) {
    let th = PLATES[idx];
    let mut set = 0;
    if let (Some(m), Some(slot)) = (mode, sel_slot) {
        if m.value() == ModeP::St {
            state.set_sel[slot] = 0;
        }
        set = state.set_sel[slot];
    }
    let w = ui.available_width().clamp(640.0, 960.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, height), Sense::hover());
    let painter = ui.painter_at(rect);
    let ear = 28.0;
    // orecchie: metallo scuro con sfumatura
    gradient_rect(&painter, rect, Color32::from_rgb(52, 52, 54), Color32::from_rgb(20, 20, 22));
    let plate = Rect::from_min_max(rect.min + Vec2::new(ear, 1.0), rect.max - Vec2::new(ear, 1.0));
    // pannello: sfumatura, righe di spazzolatura, bordo e luci
    gradient_rect(&painter, plate, mix(th.panel, Color32::WHITE, 0.12), mix(th.panel, Color32::BLACK, 0.22));
    if idx == 5 {
        gold_mottle(&painter.with_clip_rect(plate), plate);
    }
    let mut y = plate.top() + 3.0;
    let mut k = 0;
    while y < plate.bottom() - 2.0 {
        let c = if k % 2 == 0 { Color32::from_white_alpha(8) } else { Color32::from_black_alpha(6) };
        painter.line_segment([Pos2::new(plate.left() + 2.0, y), Pos2::new(plate.right() - 2.0, y)], Stroke::new(1.0, c));
        y += 2.0;
        k += 1;
    }
    painter.rect_stroke(plate, 2.0, Stroke::new(1.5, th.edge), StrokeKind::Inside);
    painter.line_segment([plate.left_top() + Vec2::new(2.0, 2.0), plate.right_top() + Vec2::new(-2.0, 2.0)], Stroke::new(1.0, mix(th.panel, Color32::WHITE, 0.45)));
    painter.line_segment([plate.left_bottom() + Vec2::new(2.0, -2.0), plate.right_bottom() + Vec2::new(-2.0, -2.0)], Stroke::new(1.0, Color32::from_black_alpha(70)));
    for x in [rect.left() + ear * 0.5, rect.right() - ear * 0.5] {
        for yy in [rect.top() + 16.0, rect.bottom() - 16.0] {
            screw(&painter, Pos2::new(x, yy));
        }
    }
    // marchio inciso, molto tenue, in basso a destra
    painter.text(plate.right_bottom() + Vec2::new(-18.0, -8.0), Align2::RIGHT_BOTTOM, "M C S", FontId::proportional(26.0), th.text.gamma_multiply(0.14));

    // intestazione: titolo inciso e sottotitolo
    let head = Rect::from_min_size(plate.min + Vec2::new(14.0, 7.0), Vec2::new(148.0, plate.height() - 14.0));
    let sep_x = head.right() + 10.0;
    let engrave = if th.metal { Color32::from_white_alpha(170) } else { Color32::from_black_alpha(190) };
    let tp = head.left_top();
    for (d, c) in [(Vec2::new(0.0, 1.3), engrave), (Vec2::ZERO, th.text), (Vec2::new(0.7, 0.0), th.text)] {
        painter.text(tp + d, Align2::LEFT_TOP, title, FontId::proportional(18.0), c);
    }
    painter.text(tp + Vec2::new(1.0, 23.0), Align2::LEFT_TOP, subtitle, FontId::proportional(9.5), th.dim);
    painter.line_segment([Pos2::new(sep_x, plate.top() + 10.0), Pos2::new(sep_x, plate.bottom() - 10.0)], Stroke::new(1.0, mix(th.panel, Color32::BLACK, 0.4)));
    painter.line_segment([Pos2::new(sep_x + 1.0, plate.top() + 10.0), Pos2::new(sep_x + 1.0, plate.bottom() - 10.0)], Stroke::new(1.0, mix(th.panel, Color32::WHITE, 0.3)));

    if let Some(on) = on {
        let r = Rect::from_min_size(Pos2::new(head.right() - 50.0, head.top()), Vec2::new(50.0, 24.0));
        ui.scope_builder(UiBuilder::new().max_rect(r), |ui| {
            toggle(ui, setter, on, "ON", th, Color32::from_rgb(90, 220, 90));
        });
    }
    if let (Some(m), Some(slot)) = (mode, sel_slot) {
        let r = Rect::from_min_max(head.min + Vec2::new(0.0, 38.0), head.max);
        ui.scope_builder(UiBuilder::new().max_rect(r), |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(4.0, 3.0);
            let cur = m.value();
            ui.horizontal(|ui| {
                let bw = (ui.available_width() - 2.0 * ui.spacing().item_spacing.x) / 3.0;
                for (name, val) in [("ST", ModeP::St), ("DUO", ModeP::Duo), ("MS", ModeP::Ms)] {
                    if seg_button(ui, name, cur == val, bw, th) && cur != val {
                        setter.begin_set_parameter(m);
                        setter.set_parameter(m, val);
                        setter.end_set_parameter(m);
                        state.set_sel[slot] = 0;
                    }
                }
            });
            if cur != ModeP::St {
                let (a, b) = if cur == ModeP::Duo { ("L", "R") } else { ("MID", "SIDE") };
                ui.horizontal(|ui| {
                    let bw = (ui.available_width() - ui.spacing().item_spacing.x) / 2.0;
                    if seg_button(ui, a, state.set_sel[slot] == 0, bw, th) {
                        state.set_sel[slot] = 0;
                    }
                    if seg_button(ui, b, state.set_sel[slot] == 1, bw, th) {
                        state.set_sel[slot] = 1;
                    }
                });
            }
        });
    }

    let body = Rect::from_min_max(Pos2::new(sep_x + 14.0, plate.top() + 6.0), Pos2::new(plate.right() - 12.0, plate.bottom() - 6.0));
    ui.scope_builder(UiBuilder::new().max_rect(body).layout(egui::Layout::left_to_right(egui::Align::Min)), |ui| {
        ui.spacing_mut().item_spacing = Vec2::new(10.0, 4.0);
        content(ui, set, th);
    });
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
            let (rect, _) = ui.allocate_exact_size(Vec2::new(100.0, 56.0), Sense::hover());
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

}

/// Goniometro: i campioni recenti disegnati su L/R ruotati di 45 gradi.
fn goniometer(ui: &mut Ui, shared: &Shared, side: f32) {
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


// ---------------------------------------------------------------- curva EQ

const BAND_COL: [Color32; 4] = [
    Color32::from_rgb(255, 150, 90),
    Color32::from_rgb(240, 210, 90),
    Color32::from_rgb(120, 220, 140),
    Color32::from_rgb(110, 180, 255),
];
const F_MIN: f64 = 20.0;
const F_MAX: f64 = 20000.0;
const DB_RANGE: f32 = 18.0;

fn x_of(f: f64, rect: Rect) -> f32 {
    let t = ((f.max(F_MIN).ln() - F_MIN.ln()) / (F_MAX.ln() - F_MIN.ln())) as f32;
    rect.left() + t.clamp(0.0, 1.0) * rect.width()
}
fn f_of(x: f32, rect: Rect) -> f64 {
    let t = ((x - rect.left()) / rect.width()).clamp(0.0, 1.0) as f64;
    (F_MIN.ln() + t * (F_MAX.ln() - F_MIN.ln())).exp()
}
fn y_of(db: f32, rect: Rect) -> f32 {
    let half = rect.height() * 0.5 - 8.0;
    rect.center().y - (db / DB_RANGE).clamp(-1.0, 1.0) * half
}
fn db_of(y: f32, rect: Rect) -> f32 {
    let half = rect.height() * 0.5 - 8.0;
    (rect.center().y - y) / half * DB_RANGE
}

/// Grafico della risposta in frequenza: bande del Tone, presenza e filtri finali. I punti delle bande si trascinano.
fn eq_curve(ui: &mut Ui, setter: &ParamSetter, state: &GuiState, p: &McsParams, sr_in: f32, w: f32, h: f32) {
    use mcs_dsp::biquad::{peaking, Cascade};
    use mcs_dsp::tone::{band_coeffs, Shape};

    let sr = if sr_in > 1000.0 { sr_in as f64 } else { 48000.0 };
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, h), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, Color32::from_rgb(16, 18, 20));
    painter.rect_stroke(rect, 4.0, Stroke::new(1.5, Color32::from_rgb(70, 66, 58)), StrokeKind::Inside);

    // griglia
    let grid = Color32::from_rgb(44, 48, 50);
    let txt = Color32::from_rgb(120, 126, 120);
    for (f, label) in [(50.0, ""), (100.0, "100"), (200.0, ""), (500.0, ""), (1000.0, "1k"), (2000.0, ""), (5000.0, ""), (10000.0, "10k")] {
        let x = x_of(f, rect);
        painter.line_segment([Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())], Stroke::new(1.0, grid));
        if !label.is_empty() {
            painter.text(Pos2::new(x + 2.0, rect.bottom() - 2.0), Align2::LEFT_BOTTOM, label, FontId::monospace(9.0), txt);
        }
    }
    for db in [-12.0f32, -6.0, 0.0, 6.0, 12.0] {
        let y = y_of(db, rect);
        let st = if db == 0.0 { Stroke::new(1.5, Color32::from_rgb(90, 96, 90)) } else { Stroke::new(1.0, grid) };
        painter.line_segment([Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)], st);
        if db != 0.0 {
            painter.text(Pos2::new(rect.left() + 3.0, y - 1.0), Align2::LEFT_BOTTOM, format!("{:+.0}", db), FontId::monospace(9.0), txt);
        }
    }

    // set in uso
    let st_tone = if p.tone_mode.value() == ModeP::St { 0 } else { state.set_sel[0] };
    let st_flt = if p.flt_mode.value() == ModeP::St { 0 } else { state.set_sel[2] };
    let st_eqf = if p.eqf_mode.value() == ModeP::St { 0 } else { state.set_sel[3] };

    // coefficienti calcolati una volta per fotogramma
    let mut band_c = Vec::with_capacity(4);
    for b in 0..4 {
        let bp = &p.sets[st_tone].bands[b];
        let active = p.tone_on.value() && bp.on.value() && bp.gain.value().abs() > 0.005;
        band_c.push(if active {
            Some(band_coeffs(sr, b, bp.freq.value() as f64, bp.gain.value() as f64, Shape::from_index(bp.shape.value() as usize)))
        } else {
            None
        });
    }
    let sf = &p.sets[st_flt];
    let pres = if p.flt_on.value() && sf.pres_db.value().abs() > 0.005 {
        Some(peaking(sr, sf.pres_f.value() as f64, 0.9, sf.pres_db.value() as f64))
    } else {
        None
    };
    let se = &p.sets[st_eqf];
    let mut hp = Cascade::default();
    let mut lp = Cascade::default();
    let hp_on = p.eqf_on.value() && se.hp_f.value() > 20.5;
    let lp_on = p.eqf_on.value() && se.lp_f.value() < 19000.0;
    if hp_on {
        hp.set_butterworth(sr, se.hp_f.value() as f64, se.hp_s.value() as usize, true);
    }
    if lp_on {
        lp.set_butterworth(sr, se.lp_f.value() as f64, se.lp_s.value() as usize, false);
    }

    let n = (rect.width() / 2.0).max(60.0) as usize;
    let mut total = Vec::with_capacity(n + 1);
    let mut per_band: Vec<Vec<Pos2>> = vec![Vec::with_capacity(n + 1); 4];
    for i in 0..=n {
        let x = rect.left() + rect.width() * i as f32 / n as f32;
        let f = f_of(x, rect);
        let mut mag = 1.0f64;
        for b in 0..4 {
            if let Some(c) = &band_c[b] {
                let m = c.magnitude(sr, f);
                per_band[b].push(Pos2::new(x, y_of(20.0 * m.log10() as f32, rect)));
                mag *= m;
            }
        }
        if let Some(c) = &pres {
            mag *= c.magnitude(sr, f);
        }
        if hp_on {
            mag *= hp.magnitude(sr, f);
        }
        if lp_on {
            mag *= lp.magnitude(sr, f);
        }
        let db = 20.0 * mag.max(1e-6).log10() as f32;
        total.push(Pos2::new(x, y_of(db, rect)));
    }

    // riempimento tra la curva e lo 0 dB
    let y0 = y_of(0.0, rect);
    let fill = Color32::from_rgba_unmultiplied(230, 200, 120, 38);
    for pt in total.iter().step_by(2) {
        painter.line_segment([Pos2::new(pt.x, y0), *pt], Stroke::new(2.0, fill));
    }
    for (b, pts) in per_band.into_iter().enumerate() {
        if pts.len() > 1 {
            let c = BAND_COL[b];
            painter.add(egui::Shape::line(pts, Stroke::new(1.0, Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), 120))));
        }
    }
    painter.add(egui::Shape::line(total, Stroke::new(2.2, Color32::from_rgb(245, 225, 160))));

    // punti trascinabili delle quattro bande
    for b in 0..4 {
        let bp = &p.sets[st_tone].bands[b];
        let pos = Pos2::new(x_of(bp.freq.value() as f64, rect), y_of(bp.gain.value(), rect));
        let id = ui.make_persistent_id(("eq-node", b));
        let resp = ui.interact(Rect::from_center_size(pos, Vec2::splat(20.0)), id, Sense::click_and_drag());
        let on = p.tone_on.value() && bp.on.value();
        let col = if on { BAND_COL[b] } else { Color32::from_rgb(110, 110, 105) };
        if resp.drag_started() {
            setter.begin_set_parameter(&bp.gain);
            setter.begin_set_parameter(&bp.freq);
        }
        if resp.dragged() {
            if let Some(pp) = resp.interact_pointer_pos() {
                let f = f_of(pp.x, rect) as f32;
                let g = db_of(pp.y, rect).clamp(-15.0, 15.0);
                setter.set_parameter_normalized(&bp.freq, bp.freq.preview_normalized(f).clamp(0.0, 1.0));
                setter.set_parameter_normalized(&bp.gain, bp.gain.preview_normalized(g).clamp(0.0, 1.0));
            }
        }
        if resp.drag_stopped() {
            setter.end_set_parameter(&bp.gain);
            setter.end_set_parameter(&bp.freq);
        }
        if resp.double_clicked() {
            setter.begin_set_parameter(&bp.gain);
            setter.set_parameter_normalized(&bp.gain, bp.gain.default_normalized_value());
            setter.end_set_parameter(&bp.gain);
        }
        let hot = resp.hovered() || resp.dragged();
        painter.circle_filled(pos, if hot { 8.0 } else { 6.5 }, col);
        painter.circle_stroke(pos, if hot { 8.0 } else { 6.5 }, Stroke::new(1.5, Color32::from_rgb(20, 20, 20)));
        painter.text(pos, Align2::CENTER_CENTER, format!("{}", b + 1), FontId::monospace(9.0), Color32::from_rgb(20, 20, 20));
        if hot {
            let t = format!("{:.0} Hz  {:+.1} dB", bp.freq.value(), bp.gain.value());
            painter.text(pos + Vec2::new(0.0, -14.0), Align2::CENTER_BOTTOM, t, FontId::monospace(10.0), Color32::from_rgb(235, 235, 220));
        }
    }
    ui.add_space(6.0);
}

// ---------------------------------------------------------------- finestra

/// Dimensione di progetto dell'interfaccia: la finestra si adatta da sola, così si vede sempre tutto.

pub fn draw(ctx: &egui::Context, setter: &ParamSetter, state: &mut GuiState, params: &McsParams, shared: &Shared, egui_state: &EguiState) {
    ctx.request_repaint_after(Duration::from_millis(33));
    // zoom manuale (pulsanti - e +): niente calcoli automatici, resta nitido
    if (state.zoom - ctx.zoom_factor()).abs() > 0.004 {
        ctx.set_zoom_factor(state.zoom);
    }
    ResizableWindow::new("mcs-window").min_size(Vec2::new(520.0, 560.0)).show(ctx, egui_state, |ui| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("M C S").strong().size(26.0).color(Color32::from_rgb(236, 214, 150)));
            ui.label(egui::RichText::new("MULTI CHANNEL STRIP").size(13.0).color(Color32::from_rgb(190, 180, 160)));
            ui.label(egui::RichText::new("v10 ORO").size(11.0).color(Color32::from_rgb(120, 200, 120)));
            ui.add_space(12.0);
            if ui.button("  −  ").clicked() {
                state.zoom = (state.zoom - 0.05).max(0.4);
            }
            ui.label(format!("{:.0}%", state.zoom * 100.0));
            if ui.button("  +  ").clicked() {
                state.zoom = (state.zoom + 0.05).min(1.6);
            }
            ui.add_space(12.0);
            ui.label("Oversampling");
            dropdown(ui, setter, &params.os, "os", 70.0);
            if ui.button("Reset LUFS").clicked() {
                shared.reset_lufs.store(true, Relaxed);
            }
        });
        ui.horizontal_top(|ui| {
            let total = ui.available_width().min(1700.0);
            let curve_w = (total - 530.0).max(300.0);
            eq_curve(ui, setter, state, params, shared.sr.load(Relaxed), curve_w, 172.0);
            ui.vertical(|ui| {
                analysis_panel(ui, shared, PLATES[7]);
            });
            ui.vertical(|ui| {
                goniometer(ui, shared, 172.0);
            });
        });
        ui.add_space(4.0);
        let avail = (ctx.screen_rect().bottom() - ui.cursor().top() - 6.0).max(120.0);
        let wide = ui.available_width() >= 1540.0;
        egui::ScrollArea::vertical().max_height(avail).auto_shrink([false, false]).show(ui, |ui| {
            if wide {
                // finestra larga: due colonne affiancate, nell'ordine del segnale (sinistra poi destra)
                let left_w = 880.0;
                let right_w = (ui.available_width() - left_w - 16.0).clamp(640.0, 960.0);
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(left_w);
                        rack(ui, setter, state, params, shared, 0);
                    });
                    ui.add_space(4.0);
                    ui.vertical(|ui| {
                        ui.set_width(right_w);
                        rack(ui, setter, state, params, shared, 1);
                    });
                });
            } else {
                rack(ui, setter, state, params, shared, 2);
            }
        });
    });
}

/// Macchie sottili sull'ottone, per un aspetto meno piatto.
fn gold_mottle(p: &egui::Painter, r: Rect) {
    for i in 0..18 {
        let f = i as f32;
        let x = r.left() + r.width() * ((f * 7.31).sin() * 0.5 + 0.5);
        let y = r.top() + r.height() * ((f * 3.17 + 1.0).sin() * 0.5 + 0.5);
        let rad = 22.0 + 44.0 * ((f * 1.91).cos() * 0.5 + 0.5);
        let col = if i % 2 == 0 { Color32::from_rgba_unmultiplied(255, 236, 150, 14) } else { Color32::from_rgba_unmultiplied(90, 60, 10, 16) };
        p.circle_filled(Pos2::new(x, y), rad, col);
    }
}

/// Riquadro incassato scuro, con titolo opzionale.
fn inset_panel(ui: &mut Ui, w: f32, h: f32, title: &str, f: impl FnOnce(&mut Ui)) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, h), Sense::hover());
    let p = ui.painter_at(rect);
    gradient_rect(&p, rect, Color32::from_rgb(64, 16, 14), Color32::from_rgb(24, 6, 6));
    p.rect_stroke(rect, 4.0, Stroke::new(1.5, Color32::from_rgb(8, 3, 3)), StrokeKind::Inside);
    p.line_segment([rect.left_top() + Vec2::new(3.0, 1.5), rect.right_top() + Vec2::new(-3.0, 1.5)], Stroke::new(1.0, Color32::from_white_alpha(26)));
    let mut top = 4.0;
    if !title.is_empty() {
        p.text(rect.left_top() + Vec2::new(8.0, 9.0), Align2::LEFT_CENTER, title, FontId::proportional(10.5), INNER.text);
        top = 18.0;
    }
    let inner = Rect::from_min_max(rect.min + Vec2::new(5.0, top), rect.max - Vec2::new(5.0, 2.0));
    ui.scope_builder(UiBuilder::new().max_rect(inner), |ui| {
        ui.spacing_mut().item_spacing = Vec2::new(4.0, 0.0);
        f(ui)
    });
}

/// Riga con LED e etichetta, cliccabile. Restituisce true se cliccata.
fn led_choice(ui: &mut Ui, w: f32, label: &str, on: bool, col: Color32) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 16.0), Sense::click());
    let p = ui.painter_at(rect);
    let c = Pos2::new(rect.left() + 7.0, rect.center().y);
    if on {
        p.circle_filled(c, 5.5, col.gamma_multiply(0.25));
        p.circle_filled(c, 3.2, col);
    } else {
        p.circle_filled(c, 3.0, mix(col, Color32::BLACK, 0.8));
    }
    let t = if on || resp.hovered() { INNER.text } else { INNER.dim };
    p.text(Pos2::new(rect.left() + 18.0, rect.center().y), Align2::LEFT_CENTER, label, FontId::proportional(10.5), t);
    resp.clicked()
}

/// Scala a LED stereo (picchi), come i misuratori a punti dei convertitori.
fn led_ladder(ui: &mut Ui, w: f32, shared: &Shared) {
    const T: [f32; 31] = [
        -48.0, -44.0, -40.0, -36.0, -33.0, -30.0, -28.0, -26.0, -24.0, -22.0, -20.0, -19.0, -18.0, -17.0, -16.0, -15.0, -14.0, -13.0, -12.0, -11.0, -10.0,
        -9.0, -8.0, -7.0, -6.0, -5.0, -4.0, -3.0, -2.0, -1.0, 0.0,
    ];
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 44.0), Sense::hover());
    let p = ui.painter_at(rect);
    p.rect(rect, CornerRadius::same(4), Color32::from_rgb(12, 10, 10), Stroke::new(1.5, Color32::from_rgb(70, 50, 16)), StrokeKind::Inside);
    let step = (w - 30.0) / T.len() as f32;
    let lv = [shared.peak_l.load(Relaxed), shared.peak_r.load(Relaxed)];
    for (row, name) in ["L", "R"].iter().enumerate() {
        let y = rect.top() + if row == 0 { 10.0 } else { 34.0 };
        p.text(Pos2::new(rect.left() + 7.0, y), Align2::LEFT_CENTER, *name, FontId::proportional(9.0), INNER.dim);
        let db = 20.0 * lv[row].max(1e-6).log10();
        for (i, t) in T.iter().enumerate() {
            let pos = Pos2::new(rect.left() + 24.0 + step * (i as f32 + 0.5), y);
            let base = if *t <= -9.0 {
                Color32::from_rgb(60, 220, 70)
            } else if *t <= -5.0 {
                Color32::from_rgb(236, 206, 60)
            } else {
                Color32::from_rgb(240, 60, 50)
            };
            if db >= *t {
                p.circle_filled(pos, 4.6, base.gamma_multiply(0.22));
                p.circle_filled(pos, 2.7, base);
            } else {
                p.circle_filled(pos, 2.4, mix(base, Color32::BLACK, 0.82));
            }
        }
    }
    for (i, t) in T.iter().enumerate() {
        if i % 3 == 0 || i == T.len() - 1 {
            let x = rect.left() + 24.0 + step * (i as f32 + 0.5);
            p.text(Pos2::new(x, rect.top() + 22.0), Align2::CENTER_CENTER, format!("{:.0}", t), FontId::proportional(7.5), INNER.dim);
        }
    }
}

/// Il rack: unità orizzontali impilate, nell'ordine del segnale.
fn rack(ui: &mut Ui, setter: &ParamSetter, state: &mut GuiState, p: &McsParams, shared: &Shared, part: u8) {
    let green = Color32::from_rgb(90, 220, 90);
    let amber = Color32::from_rgb(240, 190, 60);
    ui.spacing_mut().item_spacing.y = 5.0;

    if part != 1 {
    // INPUT
    rack_unit(ui, setter, state, 0, 104.0, "INPUT", "PREAMP / TRIM", None, None, None, |ui, _s, th| {
        knob(ui, setter, &p.in_trim, "TRIM", 44.0, K_INPUT, th);
        knob(ui, setter, &p.in_tape, "NASTRO", 44.0, K_INPUT, th);
        ui.vertical(|ui| {
            toggle(ui, setter, &p.in_pol_l, "POL L", th, amber);
            toggle(ui, setter, &p.in_pol_r, "POL R", th, amber);
        });
        ui.vertical(|ui| {
            toggle(ui, setter, &p.in_swap, "L/R SWAP", th, amber);
            toggle(ui, setter, &p.in_phase, "FASE", th, amber);
        });
    });

    // TONE: quattro bande in colonne strette
    rack_unit(ui, setter, state, 1, 168.0, "TONE", "EQUALIZER", Some(&p.tone_on), Some(&p.tone_mode), Some(0), |ui, s, th| {
        let names = ["LF", "LMF", "HMF", "HF"];
        for b in 0..4 {
            let bp = &p.sets[s].bands[b];
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(names[b]).strong().size(14.0).color(BAND_COL[b]));
                    toggle(ui, setter, &bp.on, "ON", th, green);
                    toggle(ui, setter, &bp.solo, "SOLO", th, amber);
                });
                ui.horizontal(|ui| {
                    knob(ui, setter, &bp.gain, "GAIN", 40.0, K_TONE, th);
                    knob(ui, setter, &bp.freq, "FREQ", 34.0, K_TONE, th);
                });
                dropdown(ui, setter, &bp.shape, &format!("shape{}_{}", b, s), 112.0);
            });
        }
    });

    }

    // COMPRESSORE
    if part != 1 {
        rack_unit(ui, setter, state, 8, 176.0, "COMPRESSORE", "MASTER DYNAMICS", Some(&p.cmp_on), Some(&p.cmp_mode), Some(5), |ui, s, th| {
            let q = &p.sets[s];
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    knob(ui, setter, &q.cmp_thr, "SOGLIA", 38.0, K_COMP, th);
                    knob(ui, setter, &q.cmp_ratio, "RATIO", 38.0, K_COMP, th);
                    knob(ui, setter, &q.cmp_att, "ATTACCO", 38.0, K_COMP, th);
                    ui.vertical(|ui| {
                        ui.add_space(14.0);
                        toggle(ui, setter, &q.cmp_att_auto, "AUTO", th, green);
                    });
                    knob(ui, setter, &q.cmp_rel, "RILASCIO", 38.0, K_COMP, th);
                    ui.vertical(|ui| {
                        ui.add_space(14.0);
                        toggle(ui, setter, &q.cmp_rel_auto, "AUTO", th, green);
                    });
                    knob(ui, setter, &q.cmp_mk, "MAKEUP", 38.0, K_COMP, th);
                    knob(ui, setter, &q.cmp_mix, "MIX", 38.0, K_COMP, th);
                });
                ui.horizontal(|ui| {
                    knob(ui, setter, &q.cmp_tilt, "TONO", 34.0, K_COMP, th);
                    knob(ui, setter, &q.cmp_tilt_f, "FREQ", 34.0, K_COMP, th);
                    ui.add_space(14.0);
                    knob(ui, setter, &q.cmp_sc_db, "SC FILTRO", 34.0, K_COMP, th);
                    knob(ui, setter, &q.cmp_sc_f, "SC FREQ", 34.0, K_COMP, th);
                    ui.add_space(14.0);
                    ui.vertical(|ui| {
                        ui.add_space(8.0);
                        toggle(ui, setter, &q.cmp_ff, "AVANTI", th, amber);
                        toggle(ui, setter, &q.cmp_xf, "TRAFO", th, amber);
                    });
                });
            });
            let gr = shared.comp_gr.load(Relaxed);
            vu_meter(ui, 150.0, 120.0, "GAIN REDUCTION", gr, 0.0, 12.0, &[(0.0, "0"), (3.0, "3"), (6.0, "6"), (9.0, "9"), (12.0, "12")], 9.0);
        });
    }

    if part != 1 {
    // LIMITER
    rack_unit(ui, setter, state, 2, 120.0, "LIMITER", "PEAK LIMITER", Some(&p.lim_on), Some(&p.lim_mode), Some(1), |ui, s, th| {
        let q = &p.sets[s];
        knob(ui, setter, &q.lim_in, "INPUT", 44.0, K_LIMITER, th);
        knob(ui, setter, &q.lim_thr, "SOGLIA", 44.0, K_LIMITER, th);
        knob(ui, setter, &q.lim_rel, "RILASCIO", 44.0, K_LIMITER, th);
        ui.vertical(|ui| {
            toggle(ui, setter, &q.lim_auto, "AUTO", th, green);
            toggle(ui, setter, &q.sc_bell, "PRESENZA", th, amber);
        });
        ui.vertical(|ui| {
            ui.label(egui::RichText::new("SIDECHAIN").size(10.0).color(th.dim));
            toggle(ui, setter, &q.sc_hp, "HP", th, amber);
            toggle(ui, setter, &q.sc_lp, "LP", th, amber);
        });
        let gr = -shared.gr.load(Relaxed);
        vu_meter(ui, 150.0, 100.0, "GAIN REDUCTION", gr, 0.0, 12.0, &[(0.0, "0"), (3.0, "3"), (6.0, "6"), (9.0, "9"), (12.0, "12")], 9.0);
    });

    }

    if part != 0 {
    // FILTER
    rack_unit(ui, setter, state, 3, 104.0, "FILTER", "PRESENCE", Some(&p.flt_on), Some(&p.flt_mode), Some(2), |ui, s, th| {
        let q = &p.sets[s];
        knob(ui, setter, &q.pres_db, "PRESENZA", 44.0, K_FILTER, th);
        knob(ui, setter, &q.pres_f, "FREQ", 44.0, K_FILTER, th);
    });

    // EQ FINALE
    rack_unit(ui, setter, state, 4, 126.0, "EQ FINALE", "HIGH / LOW CUT", Some(&p.eqf_on), Some(&p.eqf_mode), Some(3), |ui, s, th| {
        let q = &p.sets[s];
        ui.vertical(|ui| {
            knob(ui, setter, &q.hp_f, "PASSA-ALTI", 44.0, K_EQF, th);
            dropdown(ui, setter, &q.hp_s, &format!("hps{}", s), 100.0);
        });
        ui.add_space(14.0);
        ui.vertical(|ui| {
            knob(ui, setter, &q.lp_f, "PASSA-BASSI", 44.0, K_EQF, th);
            dropdown(ui, setter, &q.lp_s, &format!("lps{}", s), 100.0);
        });
    });

    // CLIPPER
    rack_unit(ui, setter, state, 5, 168.0, "CLIPPER", "OVERSAMPLED SOFT CLIPPER", Some(&p.clip_on), Some(&p.clip_mode), Some(4), |ui, s, _th| {
        let q = &p.sets[s];
        let bw = ui.available_width();
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(6.0, 6.0);
            ui.horizontal(|ui| {
                for (param, name) in [(&q.clip_drive, "DRIVE"), (&q.clip_ceil, "CEILING"), (&q.clip_mix, "MIX")] {
                    inset_panel(ui, 70.0, 98.0, "", |ui| {
                        knob(ui, setter, param, name, 38.0, K_CLIPG, INNER);
                    });
                }
                inset_panel(ui, 112.0, 98.0, "MODELLO", |ui| {
                    let green = Color32::from_rgb(60, 220, 70);
                    for (v, name) in [(ClipP::Hard, "DURO"), (ClipP::Tanh, "MORBIDO"), (ClipP::Asym, "ASIMM."), (ClipP::Atan, "ARCTAN")] {
                        if led_choice(ui, 98.0, name, q.clip_model.value() == v, green) {
                            setter.begin_set_parameter(&q.clip_model);
                            setter.set_parameter(&q.clip_model, v);
                            setter.end_set_parameter(&q.clip_model);
                        }
                    }
                    if led_choice(ui, 98.0, "POST CLIP", q.clip_post.value(), amber) {
                        setter.begin_set_parameter(&q.clip_post);
                        setter.set_parameter(&q.clip_post, !q.clip_post.value());
                        setter.end_set_parameter(&q.clip_post);
                    }
                });
            });
            led_ladder(ui, bw, shared);
        });
    });

    // OUTPUT
    rack_unit(ui, setter, state, 6, 120.0, "OUTPUT", "OUTPUT STAGE", None, None, None, |ui, _s, th| {
        knob(ui, setter, &p.out_l, "L", 44.0, K_OUT, th);
        knob(ui, setter, &p.out_r, "R", 44.0, K_OUT, th);
        toggle(ui, setter, &p.out_link, "LINK L/R", th, green);
        ui.add_space(30.0);
        let lufs = shared.momentary.load(Relaxed);
        vu_meter(ui, 190.0, 100.0, "LUFS", lufs, -40.0, 0.0, &[(-40.0, "-40"), (-30.0, "-30"), (-20.0, "-20"), (-14.0, "-14"), (-8.0, "-8"), (0.0, "0")], -9.0);
    });
    }
}
