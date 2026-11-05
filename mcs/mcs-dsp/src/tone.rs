//! Modulo Tone: EQ a 4 bande con "forma" selezionabile per ogni banda.

use crate::biquad::{self, Biquad, Coeffs};
use crate::util::*;

/// Le cinque forme selezionabili per ogni banda.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    /// Shelf standard.
    Shelf,
    /// Shelf morbido, pendenza dolce.
    ShelfSoft,
    /// Campana media.
    Bell,
    /// Campana larga.
    BellWide,
    /// Campana stretta.
    BellNarrow,
}

impl Shape {
    pub const ALL: [Shape; 5] = [Shape::Shelf, Shape::ShelfSoft, Shape::Bell, Shape::BellWide, Shape::BellNarrow];
    pub fn from_index(i: usize) -> Shape {
        Shape::ALL[i.min(4)]
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ToneBand {
    pub on: bool,
    pub gain_db: f64,
    pub freq: f64,
    pub shape: Shape,
    /// Ascolta solo questa banda (passa-banda, passa-bassi per la prima, passa-alti per l'ultima).
    pub solo: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct ToneParams {
    pub bands: [ToneBand; 4],
}

impl Default for ToneParams {
    fn default() -> Self {
        let b = |freq, shape| ToneBand { on: true, gain_db: 0.0, freq, shape, solo: false };
        ToneParams {
            bands: [b(80.0, Shape::Shelf), b(400.0, Shape::Bell), b(2500.0, Shape::Bell), b(10000.0, Shape::Shelf)],
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Low,
    Mid,
    High,
}
const KINDS: [Kind; 4] = [Kind::Low, Kind::Mid, Kind::Mid, Kind::High];

fn band_q(shape: Shape) -> f64 {
    match shape {
        Shape::Shelf => 0.7071,
        Shape::ShelfSoft => 0.45,
        Shape::Bell => 1.0,
        Shape::BellWide => 0.5,
        Shape::BellNarrow => 2.5,
    }
}

/// Coefficienti di una banda. Le forme shelf sulle bande centrali diventano campane dolci.
pub fn band_coeffs(sr: f64, kind_index: usize, freq: f64, gain_db: f64, shape: Shape) -> Coeffs {
    let kind = KINDS[kind_index.min(3)];
    let q = band_q(shape);
    let is_shelf = matches!(shape, Shape::Shelf | Shape::ShelfSoft);
    match (is_shelf, kind) {
        (true, Kind::Low) => biquad::low_shelf(sr, freq, q, gain_db),
        (true, Kind::High) => biquad::high_shelf(sr, freq, q, gain_db),
        _ => biquad::peaking(sr, freq, q, gain_db),
    }
}

#[derive(Clone, Copy)]
struct BandSm {
    g: Smooth,
    f: Smooth,
}

pub struct Tone {
    sr: f64,
    mode: Mode,
    on: bool,
    fade: Fade,
    sm: [[BandSm; 4]; 2],
    f: [[Biquad; 4]; 2],
    act: [[bool; 4]; 2],
    solo: [Option<usize>; 2],
    solo_f: [Biquad; 2],
}

impl Tone {
    pub fn new(sr: f64) -> Self {
        let rate = sr / SUB as f64;
        let bs = BandSm { g: Smooth::new(0.0, 15.0, rate), f: Smooth::new(1000.0, 15.0, rate) };
        Tone {
            sr,
            mode: Mode::St,
            on: true,
            fade: Fade::new(sr),
            sm: [[bs; 4]; 2],
            f: [[Biquad::new(); 4]; 2],
            act: [[false; 4]; 2],
            solo: [None; 2],
            solo_f: [Biquad::new(); 2],
        }
    }

    pub fn reset(&mut self) {
        for c in self.f.iter_mut() {
            for b in c.iter_mut() {
                b.reset();
            }
        }
    }

    /// Da chiamare una volta per sotto-blocco.
    pub fn update(&mut self, p: &Module<ToneParams>) {
        if p.mode != self.mode {
            self.mode = p.mode;
            self.reset();
            for s in 0..2 {
                for b in 0..4 {
                    self.sm[s][b].g.snap(0.0);
                    self.sm[s][b].f.snap(p.sets[s].bands[b].freq);
                }
            }
        }
        self.on = p.on;
        let nsets = if p.mode == Mode::St { 1 } else { 2 };
        for s in 0..nsets {
            for b in 0..4 {
                let bp = &p.sets[s].bands[b];
                let sm = &mut self.sm[s][b];
                sm.g.set(if bp.on { bp.gain_db } else { 0.0 });
                sm.f.set(bp.freq);
                let g = sm.g.step();
                let fr = sm.f.step();
                if g.abs() < 0.005 {
                    self.act[s][b] = false;
                } else {
                    self.act[s][b] = true;
                    let c = band_coeffs(self.sr, b, fr, g, bp.shape);
                    self.f[s][b].set(c);
                }
            }
        }
        for s in 0..nsets {
            self.solo[s] = (0..4).find(|&b| p.sets[s].bands[b].solo);
            if let Some(b) = self.solo[s] {
                let bp = &p.sets[s].bands[b];
                let fr = self.sm[s][b].f.cur;
                let q = band_q(bp.shape);
                let c = match KINDS[b] {
                    Kind::Low => biquad::lowpass(self.sr, fr, q.max(0.5)),
                    Kind::High => biquad::highpass(self.sr, fr, q.max(0.5)),
                    Kind::Mid => biquad::bandpass(self.sr, fr, q.max(1.5)),
                };
                self.solo_f[s].set(c);
            }
        }
        if nsets == 1 {
            // solo i coefficienti, mai lo stato dei filtri: i due canali restano indipendenti
            for b in 0..4 {
                let c = self.f[0][b].c;
                self.f[1][b].set(c);
            }
            self.act[1] = self.act[0];
            self.solo[1] = self.solo[0];
            let c = self.solo_f[0].c;
            self.solo_f[1].set(c);
        }
    }

    #[inline]
    pub fn process(&mut self, l: f64, r: f64) -> (f64, f64) {
        let w = self.fade.tick(self.on);
        if w <= 0.0 {
            return (l, r);
        }
        let (a, b) = encode(self.mode, l, r);
        let mut ya = a;
        let mut yb = b;
        if self.solo[0].is_some() {
            ya = self.solo_f[0].process(a);
        } else {
            for i in 0..4 {
                if self.act[0][i] {
                    ya = self.f[0][i].process(ya);
                }
            }
        }
        if self.solo[1].is_some() {
            yb = self.solo_f[1].process(b);
        } else {
            for i in 0..4 {
                if self.act[1][i] {
                    yb = self.f[1][i].process(yb);
                }
            }
        }
        let (wl, wr) = decode(self.mode, ya, yb);
        if w >= 1.0 {
            (wl, wr)
        } else {
            (l + w * (wl - l), r + w * (wr - r))
        }
    }
}
