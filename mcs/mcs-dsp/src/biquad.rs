//! Filtri biquad (forma TDF2, calcolo in f64) e coefficienti RBJ.

use std::f64::consts::PI;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Coeffs {
    pub b0: f64,
    pub b1: f64,
    pub b2: f64,
    pub a1: f64,
    pub a2: f64,
}

impl Coeffs {
    pub const IDENTITY: Coeffs = Coeffs { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0 };

    fn norm(b0: f64, b1: f64, b2: f64, a0: f64, a1: f64, a2: f64) -> Coeffs {
        let r = 1.0 / a0;
        Coeffs { b0: b0 * r, b1: b1 * r, b2: b2 * r, a1: a1 * r, a2: a2 * r }
    }

    /// Modulo della risposta in frequenza (lineare) a `f` Hz.
    pub fn magnitude(&self, sr: f64, f: f64) -> f64 {
        let w = 2.0 * PI * f / sr;
        let (c1, s1) = (w.cos(), w.sin());
        let (c2, s2) = ((2.0 * w).cos(), (2.0 * w).sin());
        let nr = self.b0 + self.b1 * c1 + self.b2 * c2;
        let ni = -(self.b1 * s1 + self.b2 * s2);
        let dr = 1.0 + self.a1 * c1 + self.a2 * c2;
        let di = -(self.a1 * s1 + self.a2 * s2);
        ((nr * nr + ni * ni) / (dr * dr + di * di)).sqrt()
    }
}

#[inline]
fn clamp_f(sr: f64, f: f64) -> f64 {
    f.clamp(5.0, sr * 0.49)
}

pub fn peaking(sr: f64, f: f64, q: f64, gain_db: f64) -> Coeffs {
    let a = 10f64.powf(gain_db / 40.0);
    let w0 = 2.0 * PI * clamp_f(sr, f) / sr;
    let alpha = w0.sin() / (2.0 * q);
    let cw = w0.cos();
    Coeffs::norm(
        1.0 + alpha * a,
        -2.0 * cw,
        1.0 - alpha * a,
        1.0 + alpha / a,
        -2.0 * cw,
        1.0 - alpha / a,
    )
}

pub fn low_shelf(sr: f64, f: f64, q: f64, gain_db: f64) -> Coeffs {
    let a = 10f64.powf(gain_db / 40.0);
    let w0 = 2.0 * PI * clamp_f(sr, f) / sr;
    let alpha = w0.sin() / (2.0 * q);
    let cw = w0.cos();
    let t = 2.0 * a.sqrt() * alpha;
    Coeffs::norm(
        a * ((a + 1.0) - (a - 1.0) * cw + t),
        2.0 * a * ((a - 1.0) - (a + 1.0) * cw),
        a * ((a + 1.0) - (a - 1.0) * cw - t),
        (a + 1.0) + (a - 1.0) * cw + t,
        -2.0 * ((a - 1.0) + (a + 1.0) * cw),
        (a + 1.0) + (a - 1.0) * cw - t,
    )
}

pub fn high_shelf(sr: f64, f: f64, q: f64, gain_db: f64) -> Coeffs {
    let a = 10f64.powf(gain_db / 40.0);
    let w0 = 2.0 * PI * clamp_f(sr, f) / sr;
    let alpha = w0.sin() / (2.0 * q);
    let cw = w0.cos();
    let t = 2.0 * a.sqrt() * alpha;
    Coeffs::norm(
        a * ((a + 1.0) + (a - 1.0) * cw + t),
        -2.0 * a * ((a - 1.0) + (a + 1.0) * cw),
        a * ((a + 1.0) + (a - 1.0) * cw - t),
        (a + 1.0) - (a - 1.0) * cw + t,
        2.0 * ((a - 1.0) - (a + 1.0) * cw),
        (a + 1.0) - (a - 1.0) * cw - t,
    )
}

pub fn lowpass(sr: f64, f: f64, q: f64) -> Coeffs {
    let w0 = 2.0 * PI * clamp_f(sr, f) / sr;
    let alpha = w0.sin() / (2.0 * q);
    let cw = w0.cos();
    Coeffs::norm((1.0 - cw) * 0.5, 1.0 - cw, (1.0 - cw) * 0.5, 1.0 + alpha, -2.0 * cw, 1.0 - alpha)
}

pub fn highpass(sr: f64, f: f64, q: f64) -> Coeffs {
    let w0 = 2.0 * PI * clamp_f(sr, f) / sr;
    let alpha = w0.sin() / (2.0 * q);
    let cw = w0.cos();
    Coeffs::norm((1.0 + cw) * 0.5, -(1.0 + cw), (1.0 + cw) * 0.5, 1.0 + alpha, -2.0 * cw, 1.0 - alpha)
}

/// Passa-banda con guadagno 0 dB al centro (per il SOLO delle bande).
pub fn bandpass(sr: f64, f: f64, q: f64) -> Coeffs {
    let w0 = 2.0 * PI * clamp_f(sr, f) / sr;
    let alpha = w0.sin() / (2.0 * q);
    let cw = w0.cos();
    Coeffs::norm(alpha, 0.0, -alpha, 1.0 + alpha, -2.0 * cw, 1.0 - alpha)
}

/// Passa-tutto del secondo ordine (usato per la rete di sfasamento).
pub fn allpass(sr: f64, f: f64, q: f64) -> Coeffs {
    let w0 = 2.0 * PI * clamp_f(sr, f) / sr;
    let alpha = w0.sin() / (2.0 * q);
    let cw = w0.cos();
    Coeffs::norm(1.0 - alpha, -2.0 * cw, 1.0 + alpha, 1.0 + alpha, -2.0 * cw, 1.0 - alpha)
}

#[derive(Clone, Copy, Debug)]
pub struct Biquad {
    pub c: Coeffs,
    z1: f64,
    z2: f64,
}

impl Default for Biquad {
    fn default() -> Self {
        Biquad { c: Coeffs::IDENTITY, z1: 0.0, z2: 0.0 }
    }
}

impl Biquad {
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    pub fn set(&mut self, c: Coeffs) {
        self.c = c;
    }

    pub fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }

    #[inline]
    pub fn process(&mut self, x: f64) -> f64 {
        let y = self.c.b0 * x + self.z1;
        self.z1 = self.c.b1 * x - self.c.a1 * y + self.z2;
        self.z2 = self.c.b2 * x - self.c.a2 * y;
        y
    }
}

/// Q delle sezioni di un filtro di Butterworth a 12, 24 e 48 dB per ottava.
const BUTTER_Q: [&[f64]; 3] = [
    &[0.7071067811865476],
    &[0.5411961001461969, 1.3065629648763764],
    &[0.5097955791041592, 0.6013448869350453, 0.8999762231364156, 2.5629154477415055],
];

/// Cascata di biquad (fino a 4 sezioni) per i filtri a pendenza variabile.
#[derive(Clone, Copy, Debug)]
pub struct Cascade {
    pub s: [Biquad; 4],
    pub n: usize,
}

impl Default for Cascade {
    fn default() -> Self {
        Cascade { s: [Biquad::new(); 4], n: 1 }
    }
}

impl Cascade {
    /// `slope`: 0 = 12 dB/ott, 1 = 24 dB/ott, 2 = 48 dB/ott.
    pub fn set_butterworth(&mut self, sr: f64, f: f64, slope: usize, highpass_filter: bool) {
        let qs = BUTTER_Q[slope.min(2)];
        self.n = qs.len();
        for (i, q) in qs.iter().enumerate() {
            let c = if highpass_filter { highpass(sr, f, *q) } else { lowpass(sr, f, *q) };
            self.s[i].set(c);
        }
    }

    pub fn reset(&mut self) {
        for b in self.s.iter_mut() {
            b.reset();
        }
    }

    #[inline]
    pub fn process(&mut self, mut x: f64) -> f64 {
        for i in 0..self.n {
            x = self.s[i].process(x);
        }
        x
    }

    /// Risposta in frequenza dell'intera cascata (lineare).
    pub fn magnitude(&self, sr: f64, f: f64) -> f64 {
        let mut m = 1.0;
        for i in 0..self.n {
            m *= self.s[i].c.magnitude(sr, f);
        }
        m
    }
}
