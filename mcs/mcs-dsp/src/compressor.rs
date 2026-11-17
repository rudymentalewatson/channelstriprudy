//! Compressore da mastering: ginocchio morbido, attacco e rilascio "auto veloce", rivelatore
//! in retroazione o in avanti, filtro sul sidechain, filtro tonale dopo la compressione,
//! mix parallelo e stadio "trasformatore" (saturazione leggera). Nessun lookahead, quindi nessuna latenza.

use crate::biquad::{self, Biquad};
use crate::util::*;

#[derive(Clone, Copy, Debug)]
pub struct CompParams {
    pub thresh_db: f64,
    pub ratio: f64,
    pub attack_ms: f64,
    /// Accorcia l'attacco sui picchi veloci e forti.
    pub auto_attack: bool,
    pub release_ms: f64,
    /// Rilascio veloce sulla parte di riduzione oltre 4 dB, poi torna al valore impostato.
    pub auto_release: bool,
    /// Rivelatore sul segnale in ingresso (più duro) invece che dopo il guadagno (più morbido).
    pub feed_fwd: bool,
    pub makeup_db: f64,
    /// 0 = solo segnale originale, 1 = solo compresso.
    pub mix: f64,
    /// Inclinazione tonale dopo la compressione: positivo = più alti, meno bassi.
    pub tilt_db: f64,
    pub tilt_freq: f64,
    /// Inclinazione sul sidechain: positivo = il compressore ignora i bassi, negativo = ignora gli alti.
    pub sc_db: f64,
    pub sc_freq: f64,
    /// Stadio trasformatore: saturazione dispari leggera e un po' di corpo sui bassi.
    pub xfmr: bool,
}
impl Default for CompParams {
    fn default() -> Self {
        CompParams {
            thresh_db: 0.0,
            ratio: 2.0,
            attack_ms: 30.0,
            auto_attack: false,
            release_ms: 200.0,
            auto_release: false,
            feed_fwd: false,
            makeup_db: 0.0,
            mix: 1.0,
            tilt_db: 0.0,
            tilt_freq: 1000.0,
            sc_db: 0.0,
            sc_freq: 150.0,
            xfmr: false,
        }
    }
}

const KNEE_DB: f64 = 6.0;
const FAST_REL_MS: f64 = 20.0;
const AUTO_REL_FROM_DB: f64 = 4.0;
const AUTO_ATT_FROM_DB: f64 = 3.0;

/// Riduzione di guadagno in dB (>= 0) per un livello `over` dB sopra la soglia.
#[inline]
pub fn gain_reduction_db(over: f64, ratio: f64) -> f64 {
    let s = 1.0 - 1.0 / ratio.max(1.0);
    let h = KNEE_DB * 0.5;
    if over <= -h {
        0.0
    } else if over >= h {
        s * over
    } else {
        let x = over + h;
        s * x * x / (2.0 * KNEE_DB)
    }
}

struct Chan {
    gr: f64,
    last_lin: f64,
    sc: [Biquad; 2],
    tilt: [Biquad; 2],
    xf: Biquad,
    // valori già impostati nei filtri, per ricalcolare i coefficienti solo quando cambiano
    sc_key: (f64, f64),
    tilt_key: (f64, f64),
    sc_act: bool,
    tilt_act: bool,
}
impl Chan {
    fn new() -> Self {
        Chan {
            gr: 0.0,
            last_lin: 1.0,
            sc: [Biquad::new(); 2],
            tilt: [Biquad::new(); 2],
            xf: Biquad::new(),
            sc_key: (f64::NAN, f64::NAN),
            tilt_key: (f64::NAN, f64::NAN),
            sc_act: false,
            tilt_act: false,
        }
    }
    fn reset(&mut self) {
        self.gr = 0.0;
        self.last_lin = 1.0;
        for b in self.sc.iter_mut().chain(self.tilt.iter_mut()) {
            b.reset();
        }
        self.xf.reset();
    }
}

pub struct Compressor {
    sr: f64,
    mode: Mode,
    on: bool,
    fade: Fade,
    ch: [Chan; 2],
    thr: [Smooth; 2],
    mk: [Smooth; 2],
    mix: [Smooth; 2],
    ratio: [f64; 2],
    att_c: [f64; 2],
    att_fast_c: [f64; 2],
    rel_c: [f64; 2],
    rel_fast_c: f64,
    auto_att: [bool; 2],
    auto_rel: [bool; 2],
    ff: [bool; 2],
    xfmr: [bool; 2],
    mk_lin: [f64; 2],
    mix_v: [f64; 2],
    pub gr_db: f64,
}

impl Compressor {
    pub fn new(sr: f64) -> Self {
        let rate = sr / SUB as f64;
        let mut ch = [Chan::new(), Chan::new()];
        for c in ch.iter_mut() {
            c.xf.set(biquad::low_shelf(sr, 70.0, 0.7, 0.8));
        }
        Compressor {
            sr,
            mode: Mode::St,
            on: true,
            fade: Fade::new(sr),
            ch,
            thr: [Smooth::new(0.0, 15.0, rate); 2],
            mk: [Smooth::new(0.0, 15.0, rate); 2],
            mix: [Smooth::new(1.0, 15.0, rate); 2],
            ratio: [2.0; 2],
            att_c: [0.0; 2],
            att_fast_c: [0.0; 2],
            rel_c: [0.0; 2],
            rel_fast_c: smooth_coeff(FAST_REL_MS, sr),
            auto_att: [false; 2],
            auto_rel: [false; 2],
            ff: [false; 2],
            xfmr: [false; 2],
            mk_lin: [1.0; 2],
            mix_v: [1.0; 2],
            gr_db: 0.0,
        }
    }

    pub fn update(&mut self, p: &Module<CompParams>) {
        if p.mode != self.mode {
            self.mode = p.mode;
            self.ch[0].reset();
            self.ch[1].reset();
        }
        self.on = p.on;
        for i in 0..2 {
            let sp = p.set_for(i);
            self.thr[i].set(sp.thresh_db);
            self.mk[i].set(sp.makeup_db);
            self.mix[i].set(sp.mix);
            self.thr[i].step();
            self.mk_lin[i] = db_to_lin(self.mk[i].step());
            self.mix_v[i] = self.mix[i].step();
            self.ratio[i] = sp.ratio.max(1.0);
            let att = sp.attack_ms.max(0.05);
            self.att_c[i] = smooth_coeff(att, self.sr);
            self.att_fast_c[i] = smooth_coeff((att * 0.12).max(0.05), self.sr);
            self.rel_c[i] = smooth_coeff(sp.release_ms.max(5.0), self.sr);
            self.auto_att[i] = sp.auto_attack;
            self.auto_rel[i] = sp.auto_release;
            self.ff[i] = sp.feed_fwd;
            self.xfmr[i] = sp.xfmr;

            let c = &mut self.ch[i];
            let sk = (sp.sc_db, sp.sc_freq);
            if sk != c.sc_key {
                c.sc_key = sk;
                c.sc_act = sp.sc_db.abs() >= 0.1;
                let g = sp.sc_db * 0.5;
                c.sc[0].set(biquad::low_shelf(self.sr, sp.sc_freq, 0.6, -g));
                c.sc[1].set(biquad::high_shelf(self.sr, sp.sc_freq, 0.6, g));
            }
            let tk = (sp.tilt_db, sp.tilt_freq);
            if tk != c.tilt_key {
                c.tilt_key = tk;
                c.tilt_act = sp.tilt_db.abs() >= 0.02;
                let g = sp.tilt_db * 0.5;
                c.tilt[0].set(biquad::low_shelf(self.sr, sp.tilt_freq, 0.5, -g));
                c.tilt[1].set(biquad::high_shelf(self.sr, sp.tilt_freq, 0.5, g));
            }
        }
    }

    /// Livello visto dal rivelatore (dopo il filtro sidechain, e dopo il guadagno se in retroazione).
    #[inline]
    fn detect(&mut self, i: usize, x: f64) -> f64 {
        let c = &mut self.ch[i];
        let mut y = x;
        if c.sc_act {
            y = c.sc[0].process(y);
            y = c.sc[1].process(y);
        }
        y = y.abs();
        if !self.ff[i] {
            y *= c.last_lin;
        }
        y
    }

    #[inline]
    fn step(&mut self, i: usize, level: f64) -> f64 {
        let target = gain_reduction_db(lin_to_db(level) - self.thr[i].cur, self.ratio[i]);
        let c = &mut self.ch[i];
        let coef = if target > c.gr {
            if self.auto_att[i] && target - c.gr > AUTO_ATT_FROM_DB {
                self.att_fast_c[i]
            } else {
                self.att_c[i]
            }
        } else if self.auto_rel[i] && c.gr > AUTO_REL_FROM_DB {
            self.rel_fast_c.max(self.rel_c[i])
        } else {
            self.rel_c[i]
        };
        c.gr += (target - c.gr) * coef;
        c.last_lin = db_to_lin(-c.gr);
        c.gr
    }

    #[inline]
    fn finish(&mut self, i: usize, dry: f64, wet_in: f64) -> f64 {
        let c = &mut self.ch[i];
        let mut wet = wet_in * c.last_lin * self.mk_lin[i];
        if c.tilt_act {
            wet = c.tilt[0].process(wet);
            wet = c.tilt[1].process(wet);
        }
        let mut y = dry + self.mix_v[i] * (wet - dry);
        if self.xfmr[i] {
            y = c.xf.process(y);
            let v = y.clamp(-1.5, 1.5);
            y = v - 0.08 * v * v * v;
        }
        y
    }

    #[inline]
    pub fn process(&mut self, l: f64, r: f64) -> (f64, f64) {
        let w = self.fade.tick(self.on);
        if w <= 0.0 {
            self.gr_db = 0.0;
            return (l, r);
        }
        let (a, b) = encode(self.mode, l, r);
        if self.mode == Mode::St {
            let level = self.detect(0, a).max(self.detect(1, b));
            let g = self.step(0, level);
            self.ch[1].gr = g;
            self.ch[1].last_lin = self.ch[0].last_lin;
        } else {
            let la = self.detect(0, a);
            let lb = self.detect(1, b);
            self.step(0, la);
            self.step(1, lb);
        }
        self.gr_db = self.ch[0].gr.max(self.ch[1].gr);
        let ya = self.finish(0, a, a);
        let yb = self.finish(1, b, b);
        let (wl, wr) = decode(self.mode, ya, yb);
        (l + w * (wl - l), r + w * (wr - r))
    }
}
