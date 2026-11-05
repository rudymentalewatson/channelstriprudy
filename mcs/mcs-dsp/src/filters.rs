//! Modulo Filter (presenza) e EQ finale (passa-alti e passa-bassi a pendenza variabile).

use crate::biquad::{self, Biquad, Cascade};
use crate::util::*;

#[derive(Clone, Copy, Debug)]
pub struct FilterParams {
    pub pres_db: f64,
    pub pres_freq: f64,
}
impl Default for FilterParams {
    fn default() -> Self {
        FilterParams { pres_db: 0.0, pres_freq: 5000.0 }
    }
}

pub struct Filter {
    sr: f64,
    mode: Mode,
    on: bool,
    fade: Fade,
    g: [Smooth; 2],
    f: [Smooth; 2],
    bq: [Biquad; 2],
    act: [bool; 2],
}

impl Filter {
    pub fn new(sr: f64) -> Self {
        let rate = sr / SUB as f64;
        Filter {
            sr,
            mode: Mode::St,
            on: true,
            fade: Fade::new(sr),
            g: [Smooth::new(0.0, 15.0, rate); 2],
            f: [Smooth::new(5000.0, 15.0, rate); 2],
            bq: [Biquad::new(); 2],
            act: [false; 2],
        }
    }

    pub fn update(&mut self, p: &Module<FilterParams>) {
        if p.mode != self.mode {
            self.mode = p.mode;
            self.bq[0].reset();
            self.bq[1].reset();
        }
        self.on = p.on;
        for ch in 0..2 {
            let sp = p.set_for(ch);
            self.g[ch].set(sp.pres_db);
            self.f[ch].set(sp.pres_freq);
            let g = self.g[ch].step();
            let f = self.f[ch].step();
            self.act[ch] = g.abs() >= 0.005;
            if self.act[ch] {
                self.bq[ch].set(biquad::peaking(self.sr, f, 0.9, g));
            }
        }
    }

    #[inline]
    pub fn process(&mut self, l: f64, r: f64) -> (f64, f64) {
        let w = self.fade.tick(self.on);
        if w <= 0.0 {
            return (l, r);
        }
        let (a, b) = encode(self.mode, l, r);
        let ya = if self.act[0] { self.bq[0].process(a) } else { a };
        let yb = if self.act[1] { self.bq[1].process(b) } else { b };
        let (wl, wr) = decode(self.mode, ya, yb);
        (l + w * (wl - l), r + w * (wr - r))
    }
}

#[derive(Clone, Copy, Debug)]
pub struct EqFinaleParams {
    pub hp_freq: f64,
    /// 0 = 12, 1 = 24, 2 = 48 dB/ott
    pub hp_slope: usize,
    pub lp_freq: f64,
    pub lp_slope: usize,
}
impl Default for EqFinaleParams {
    fn default() -> Self {
        EqFinaleParams { hp_freq: 20.0, hp_slope: 1, lp_freq: 20000.0, lp_slope: 1 }
    }
}

pub const HP_OFF: f64 = 20.5;
pub const LP_OFF: f64 = 19000.0;

pub struct EqFinale {
    sr: f64,
    mode: Mode,
    on: bool,
    fade: Fade,
    hpf: [Smooth; 2],
    lpf: [Smooth; 2],
    hp: [Cascade; 2],
    lp: [Cascade; 2],
    hp_act: [bool; 2],
    lp_act: [bool; 2],
}

impl EqFinale {
    pub fn new(sr: f64) -> Self {
        let rate = sr / SUB as f64;
        EqFinale {
            sr,
            mode: Mode::St,
            on: true,
            fade: Fade::new(sr),
            hpf: [Smooth::new(20.0, 20.0, rate); 2],
            lpf: [Smooth::new(20000.0, 20.0, rate); 2],
            hp: [Cascade::default(); 2],
            lp: [Cascade::default(); 2],
            hp_act: [false; 2],
            lp_act: [false; 2],
        }
    }

    pub fn update(&mut self, p: &Module<EqFinaleParams>) {
        if p.mode != self.mode {
            self.mode = p.mode;
            for ch in 0..2 {
                self.hp[ch].reset();
                self.lp[ch].reset();
            }
        }
        self.on = p.on;
        for ch in 0..2 {
            let sp = p.set_for(ch);
            self.hpf[ch].set(sp.hp_freq);
            self.lpf[ch].set(sp.lp_freq);
            let hf = self.hpf[ch].step();
            let lf = self.lpf[ch].step();
            self.hp_act[ch] = hf > HP_OFF;
            self.lp_act[ch] = lf < LP_OFF;
            if self.hp_act[ch] {
                self.hp[ch].set_butterworth(self.sr, hf, sp.hp_slope, true);
            }
            if self.lp_act[ch] {
                self.lp[ch].set_butterworth(self.sr, lf, sp.lp_slope, false);
            }
        }
    }

    #[inline]
    pub fn process(&mut self, l: f64, r: f64) -> (f64, f64) {
        let w = self.fade.tick(self.on);
        if w <= 0.0 {
            return (l, r);
        }
        let (a, b) = encode(self.mode, l, r);
        let mut y = [a, b];
        for ch in 0..2 {
            if self.hp_act[ch] {
                y[ch] = self.hp[ch].process(y[ch]);
            }
            if self.lp_act[ch] {
                y[ch] = self.lp[ch].process(y[ch]);
            }
        }
        let (wl, wr) = decode(self.mode, y[0], y[1]);
        (l + w * (wl - l), r + w * (wr - r))
    }
}
