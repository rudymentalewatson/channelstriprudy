//! Utilità comuni: conversioni dB, modalità ST/DUO/MS, dissolvenza on/off.

#[inline]
pub fn db_to_lin(db: f64) -> f64 {
    10f64.powf(db / 20.0)
}

#[inline]
pub fn lin_to_db(x: f64) -> f64 {
    20.0 * x.max(1e-12).log10()
}

/// Coefficiente di un filtro a un polo: dopo `time_ms` raggiunge circa il 63% del salto.
#[inline]
pub fn smooth_coeff(time_ms: f64, rate_hz: f64) -> f64 {
    1.0 - (-1.0 / (time_ms * 0.001 * rate_hz).max(1e-9)).exp()
}

/// Modalità di elaborazione di un modulo.
/// - `St`: stereo collegato, un solo set di parametri per L e R.
/// - `Duo`: due canali indipendenti L e R, con set A per L e set B per R.
/// - `Ms`: mid e side, con set A per il mid e set B per il side.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    St,
    Duo,
    Ms,
}

impl Default for Mode {
    fn default() -> Self {
        Mode::St
    }
}

/// Dal dominio L/R al dominio del modulo.
#[inline]
pub fn encode(mode: Mode, l: f64, r: f64) -> (f64, f64) {
    match mode {
        Mode::Ms => ((l + r) * 0.5, (l - r) * 0.5),
        _ => (l, r),
    }
}

/// Dal dominio del modulo a L/R.
#[inline]
pub fn decode(mode: Mode, a: f64, b: f64) -> (f64, f64) {
    match mode {
        Mode::Ms => (a + b, a - b),
        _ => (a, b),
    }
}

/// Parametri comuni a tutti i moduli: acceso o escluso, modalità e due set (A e B).
#[derive(Clone, Copy, Debug)]
pub struct Module<P: Copy> {
    pub on: bool,
    pub mode: Mode,
    pub sets: [P; 2],
}

impl<P: Copy + Default> Default for Module<P> {
    fn default() -> Self {
        Module { on: true, mode: Mode::St, sets: [P::default(); 2] }
    }
}

impl<P: Copy> Module<P> {
    /// Set da usare per il canale `ch` (0 o 1) nella modalità corrente.
    #[inline]
    pub fn set_for(&self, ch: usize) -> &P {
        match self.mode {
            Mode::St => &self.sets[0],
            _ => &self.sets[ch & 1],
        }
    }
}

/// Dissolvenza tra segnale originale e segnale elaborato, per accendere e spegnere
/// un modulo senza click. A dissolvenza finita e modulo spento, l'elaborazione si salta.
#[derive(Clone, Copy, Debug)]
pub struct Fade {
    w: f64,
    step: f64,
}

impl Fade {
    pub fn new(sr: f64) -> Self {
        Fade { w: 1.0, step: 1.0 / (0.005 * sr).max(1.0) }
    }

    /// Avanza di un campione verso lo stato richiesto e restituisce il peso del segnale elaborato.
    #[inline]
    pub fn tick(&mut self, on: bool) -> f64 {
        if on {
            if self.w < 1.0 {
                self.w = (self.w + self.step).min(1.0);
            }
        } else if self.w > 0.0 {
            self.w = (self.w - self.step).max(0.0);
        }
        self.w
    }
}

/// Valore che insegue un obiettivo con un filtro a un polo, aggiornato a ogni sotto-blocco.
#[derive(Clone, Copy, Debug)]
pub struct Smooth {
    pub cur: f64,
    tgt: f64,
    c: f64,
}

impl Smooth {
    pub fn new(v: f64, time_ms: f64, rate_hz: f64) -> Self {
        Smooth { cur: v, tgt: v, c: smooth_coeff(time_ms, rate_hz) }
    }
    #[inline]
    pub fn set(&mut self, t: f64) {
        self.tgt = t;
    }
    #[inline]
    pub fn step(&mut self) -> f64 {
        self.cur += (self.tgt - self.cur) * self.c;
        if (self.tgt - self.cur).abs() < 1e-9 {
            self.cur = self.tgt;
        }
        self.cur
    }
    pub fn snap(&mut self, v: f64) {
        self.cur = v;
        self.tgt = v;
    }
}

/// Dimensione dei sotto-blocchi con cui si aggiornano i coefficienti.
pub const SUB: usize = 32;
