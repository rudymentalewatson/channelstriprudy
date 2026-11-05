//! Clipper con antialiasing ADAA del primo ordine e oversampling opzionale.

use crate::oversample::{measure_latency, Oversampler};
use crate::util::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipModel {
    Hard,
    Tanh,
    /// Tanh asimmetrico: aggiunge armoniche pari.
    Asym,
    Atan,
}
impl ClipModel {
    pub fn from_index(i: usize) -> Self {
        [ClipModel::Hard, ClipModel::Tanh, ClipModel::Asym, ClipModel::Atan][i.min(3)]
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ClipperParams {
    pub drive_db: f64,
    pub ceiling_db: f64,
    pub model: ClipModel,
    /// 0..1, parallelo tra segnale pulito e clippato.
    pub mix: f64,
    /// Taglio duro finale al livello di ceiling.
    pub post_clip: bool,
}
impl Default for ClipperParams {
    fn default() -> Self {
        ClipperParams { drive_db: 0.0, ceiling_db: 0.0, model: ClipModel::Tanh, mix: 1.0, post_clip: false }
    }
}

const BIAS: f64 = 0.3;

#[inline]
fn lncosh(x: f64) -> f64 {
    let a = x.abs();
    a + (1.0 + (-2.0 * a).exp()).ln() - std::f64::consts::LN_2
}

#[inline]
fn f(m: ClipModel, x: f64) -> f64 {
    match m {
        ClipModel::Hard => x.clamp(-1.0, 1.0),
        ClipModel::Tanh => x.tanh(),
        ClipModel::Asym => (x + BIAS).tanh() - BIAS.tanh(),
        ClipModel::Atan => x.atan() * (2.0 / std::f64::consts::PI),
    }
}

/// Antiderivata di `f`.
#[inline]
fn big_f(m: ClipModel, x: f64) -> f64 {
    match m {
        ClipModel::Hard => {
            if x.abs() <= 1.0 {
                0.5 * x * x
            } else {
                x.abs() - 0.5
            }
        }
        ClipModel::Tanh => lncosh(x),
        ClipModel::Asym => lncosh(x + BIAS) - x * BIAS.tanh(),
        ClipModel::Atan => (2.0 / std::f64::consts::PI) * (x * x.atan() - 0.5 * (1.0 + x * x).ln()),
    }
}

#[derive(Clone, Copy, Default)]
struct Adaa {
    x1: f64,
    f1: f64,
}
impl Adaa {
    #[inline]
    fn tick(&mut self, m: ClipModel, x: f64) -> f64 {
        let fx = big_f(m, x);
        let d = x - self.x1;
        let y = if d.abs() < 1e-5 { f(m, 0.5 * (x + self.x1)) } else { (fx - self.f1) / d };
        self.x1 = x;
        self.f1 = fx;
        y
    }
}

const DRY_LEN: usize = 128;

struct Chan {
    os: Oversampler,
    adaa: Adaa,
    dry: [f64; DRY_LEN],
    dpos: usize,
    dc_x: f64,
    dc_y: f64,
    last_model: ClipModel,
}
impl Chan {
    fn new() -> Self {
        Chan {
            os: Oversampler::new(),
            adaa: Adaa::default(),
            dry: [0.0; DRY_LEN],
            dpos: 0,
            dc_x: 0.0,
            dc_y: 0.0,
            last_model: ClipModel::Tanh,
        }
    }
}

pub struct Clipper {
    mode: Mode,
    on: bool,
    fade: Fade,
    ch: [Chan; 2],
    drive: [Smooth; 2],
    ceil: [Smooth; 2],
    drive_lin: [f64; 2],
    ceil_lin: [f64; 2],
    sp: [ClipperParams; 2],
    latency: usize,
    dc_r: f64,
}

impl Clipper {
    pub fn new(sr: f64) -> Self {
        let rate = sr / SUB as f64;
        Clipper {
            mode: Mode::St,
            on: true,
            fade: Fade::new(sr),
            ch: [Chan::new(), Chan::new()],
            drive: [Smooth::new(0.0, 15.0, rate); 2],
            ceil: [Smooth::new(0.0, 15.0, rate); 2],
            drive_lin: [1.0; 2],
            ceil_lin: [1.0; 2],
            sp: [ClipperParams::default(); 2],
            latency: 0,
            dc_r: 1.0 - 2.0 * std::f64::consts::PI * 8.0 / sr,
        }
    }

    pub fn latency(&self) -> usize {
        self.latency
    }

    /// 0 = nessuno, 1 = 2x, 2 = 4x, 3 = 8x, 4 = 16x.
    pub fn set_oversampling(&mut self, log2: usize) {
        if self.ch[0].os.factor_log2() != log2.min(4) {
            for c in self.ch.iter_mut() {
                c.os.set_factor_log2(log2);
                c.adaa = Adaa::default();
            }
            self.latency = measure_latency(log2.min(4));
        }
    }

    pub fn update(&mut self, p: &Module<ClipperParams>) {
        if p.mode != self.mode {
            self.mode = p.mode;
            for c in self.ch.iter_mut() {
                c.os.reset();
                c.adaa = Adaa::default();
            }
        }
        self.on = p.on;
        for i in 0..2 {
            let sp = *p.set_for(i);
            self.drive[i].set(sp.drive_db);
            self.ceil[i].set(sp.ceiling_db);
            self.drive_lin[i] = db_to_lin(self.drive[i].step());
            self.ceil_lin[i] = db_to_lin(self.ceil[i].step());
            self.sp[i] = sp;
        }
    }

    #[inline]
    fn one(&mut self, i: usize, x: f64) -> (f64, f64) {
        let sp = self.sp[i];
        let (dg, cl) = (self.drive_lin[i], self.ceil_lin[i]);
        let c = &mut self.ch[i];
        if sp.model != c.last_model {
            c.last_model = sp.model;
            c.adaa = Adaa::default();
        }
        let m = sp.model;
        let adaa = &mut c.adaa;
        let mut y = c.os.process(x * dg, &mut |v| adaa.tick(m, v)) * cl;
        if m == ClipModel::Asym {
            let o = y - c.dc_x + self.dc_r * c.dc_y;
            c.dc_x = y;
            c.dc_y = o;
            y = o;
        }
        if sp.post_clip {
            y = y.clamp(-cl, cl);
        }
        // segnale originale ritardato come il percorso elaborato
        c.dry[c.dpos] = x;
        let rp = (c.dpos + DRY_LEN - self.latency) % DRY_LEN;
        let dry = c.dry[rp];
        c.dpos = (c.dpos + 1) % DRY_LEN;
        (dry + sp.mix * (y - dry), dry)
    }

    #[inline]
    pub fn process(&mut self, l: f64, r: f64) -> (f64, f64) {
        let w = self.fade.tick(self.on);
        // quando spento e a dissolvenza finita il percorso dry deve comunque restare ritardato
        let (a, b) = encode(self.mode, l, r);
        if w <= 0.0 {
            let (_, da) = self.one_bypass(0, a);
            let (_, db) = self.one_bypass(1, b);
            return decode(self.mode, da, db);
        }
        let (ya, da) = self.one(0, a);
        let (yb, db) = self.one(1, b);
        let (wl, wr) = decode(self.mode, ya, yb);
        if w >= 1.0 {
            (wl, wr)
        } else {
            let (dl, dr) = decode(self.mode, da, db);
            (dl + w * (wl - dl), dr + w * (wr - dr))
        }
    }

    #[inline]
    fn one_bypass(&mut self, i: usize, x: f64) -> (f64, f64) {
        let c = &mut self.ch[i];
        c.dry[c.dpos] = x;
        let rp = (c.dpos + DRY_LEN - self.latency) % DRY_LEN;
        let dry = c.dry[rp];
        c.dpos = (c.dpos + 1) % DRY_LEN;
        (dry, dry)
    }
}
