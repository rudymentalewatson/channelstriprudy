//! Moduli Input e Output.

use crate::biquad::{Biquad, Coeffs};
use crate::util::*;
use std::f64::consts::PI;

#[derive(Clone, Copy, Debug)]
pub struct InputParams {
    pub trim_db: f64,
    pub pol_l: bool,
    pub pol_r: bool,
    /// Scambia L e R.
    pub swap: bool,
    /// Taglio/rialzo delle alte in stile nastro, in dB (shelf del primo ordine).
    pub tape_db: f64,
    /// Rotazione di fase nelle basse (due passa-tutto in cascata, 180° a circa 150 Hz).
    pub phase: bool,
}
impl Default for InputParams {
    fn default() -> Self {
        InputParams { trim_db: 0.0, pol_l: false, pol_r: false, swap: false, tape_db: 0.0, phase: false }
    }
}

/// Shelf alto del primo ordine: guadagno unitario in basso, `g` in alto.
pub fn first_order_high_shelf(sr: f64, f: f64, gain_db: f64) -> Coeffs {
    let g = db_to_lin(gain_db);
    let k = (PI * f.min(sr * 0.49) / sr).tan();
    // basso-shelf con guadagno DC 1/g, poi moltiplicato per g
    let gl = 1.0 / g;
    let b0 = (1.0 + gl * k) / (1.0 + k) * g;
    let b1 = (gl * k - 1.0) / (1.0 + k) * g;
    let a1 = (k - 1.0) / (1.0 + k);
    Coeffs { b0, b1, b2: 0.0, a1, a2: 0.0 }
}

pub struct Input {
    sr: f64,
    trim: Smooth,
    tape: Smooth,
    tape_f: [Biquad; 2],
    pol: [f64; 2],
    swap: bool,
    tape_act: bool,
    lin: f64,
    phase: bool,
    ap: [[Biquad; 2]; 2],
}

impl Input {
    pub fn new(sr: f64) -> Self {
        let rate = sr / SUB as f64;
        Input {
            sr,
            trim: Smooth::new(0.0, 15.0, rate),
            tape: Smooth::new(0.0, 15.0, rate),
            tape_f: [Biquad::new(); 2],
            pol: [1.0, 1.0],
            swap: false,
            tape_act: false,
            lin: 1.0,
            phase: false,
            ap: [[Biquad::new(); 2]; 2],
        }
    }
    pub fn update(&mut self, p: &InputParams) {
        if p.phase != self.phase {
            self.phase = p.phase;
            let c = if p.phase { crate::biquad::allpass(self.sr, 150.0, 0.6) } else { Coeffs::IDENTITY };
            for ch in 0..2 {
                for k in 0..2 {
                    self.ap[ch][k].set(c);
                    self.ap[ch][k].reset();
                }
            }
        }
        self.trim.set(p.trim_db);
        self.tape.set(p.tape_db);
        self.lin = db_to_lin(self.trim.step());
        let t = self.tape.step();
        self.tape_act = t.abs() >= 0.005;
        if self.tape_act {
            let c = first_order_high_shelf(self.sr, 6000.0, t);
            self.tape_f[0].set(c);
            self.tape_f[1].set(c);
        }
        self.pol = [if p.pol_l { -1.0 } else { 1.0 }, if p.pol_r { -1.0 } else { 1.0 }];
        self.swap = p.swap;
    }
    #[inline]
    pub fn process(&mut self, l: f64, r: f64) -> (f64, f64) {
        let (mut a, mut b) = if self.swap { (r, l) } else { (l, r) };
        let g = self.lin;
        a *= g * self.pol[0];
        b *= g * self.pol[1];
        if self.phase {
            let t = self.ap[0][0].process(a);
            a = self.ap[0][1].process(t);
            let t = self.ap[1][0].process(b);
            b = self.ap[1][1].process(t);
        }
        if self.tape_act {
            a = self.tape_f[0].process(a);
            b = self.tape_f[1].process(b);
        }
        (a, b)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct OutputParams {
    pub gain_l_db: f64,
    pub gain_r_db: f64,
    pub link: bool,
}
impl Default for OutputParams {
    fn default() -> Self {
        OutputParams { gain_l_db: 0.0, gain_r_db: 0.0, link: true }
    }
}

pub struct Output {
    gl: Smooth,
    gr: Smooth,
    lin: (f64, f64),
}
impl Output {
    pub fn new(sr: f64) -> Self {
        let rate = sr / SUB as f64;
        Output { gl: Smooth::new(0.0, 15.0, rate), gr: Smooth::new(0.0, 15.0, rate), lin: (1.0, 1.0) }
    }
    pub fn update(&mut self, p: &OutputParams) {
        self.gl.set(p.gain_l_db);
        self.gr.set(if p.link { p.gain_l_db } else { p.gain_r_db });
        self.lin = (db_to_lin(self.gl.step()), db_to_lin(self.gr.step()));
    }
    #[inline]
    pub fn process(&mut self, l: f64, r: f64) -> (f64, f64) {
        (l * self.lin.0, r * self.lin.1)
    }
}
