//! Limiter feed-forward con lookahead, rilascio dipendente dal programma e sidechain filtrabile.

use crate::biquad::{self, Biquad};
use crate::util::*;

#[derive(Clone, Copy, Debug)]
pub struct LimiterParams {
    /// Guadagno in ingresso (spinge il segnale contro la soglia).
    pub in_db: f64,
    pub thresh_db: f64,
    pub release_ms: f64,
    pub auto_release: bool,
    pub sc_hp: bool,
    pub sc_bell: bool,
    pub sc_lp: bool,
}
impl Default for LimiterParams {
    fn default() -> Self {
        LimiterParams {
            in_db: 0.0,
            thresh_db: 0.0,
            release_ms: 100.0,
            auto_release: true,
            sc_hp: false,
            sc_bell: false,
            sc_lp: false,
        }
    }
}

pub const LOOKAHEAD_MS: f64 = 1.5;

/// Minimo mobile su finestra fissa (coda monotona).
struct MinWin {
    val: Vec<f64>,
    ti: Vec<u64>,
    head: usize,
    len: usize,
    t: u64,
    win: u64,
}
impl MinWin {
    fn new(win: usize) -> Self {
        MinWin { val: vec![1.0; win + 2], ti: vec![0; win + 2], head: 0, len: 0, t: 0, win: win as u64 }
    }
    fn push(&mut self, v: f64) -> f64 {
        let cap = self.val.len();
        while self.len > 0 {
            let back = (self.head + self.len - 1) % cap;
            if self.val[back] >= v {
                self.len -= 1;
            } else {
                break;
            }
        }
        let pos = (self.head + self.len) % cap;
        self.val[pos] = v;
        self.ti[pos] = self.t;
        self.len += 1;
        while self.ti[self.head] + self.win <= self.t {
            self.head = (self.head + 1) % cap;
            self.len -= 1;
        }
        self.t += 1;
        self.val[self.head]
    }
}

/// Un motore di riduzione di guadagno: rilascio, tenuta del minimo e media (attacco morbido).
struct GainEngine {
    env: f64,
    avg_gr: f64,
    min: MinWin,
    ring: Vec<f64>,
    pos: usize,
    sum: f64,
    sc: [Biquad; 3],
    pub gr_db: f64,
}

impl GainEngine {
    fn new(la: usize) -> Self {
        GainEngine {
            env: 1.0,
            avg_gr: 0.0,
            min: MinWin::new(la),
            ring: vec![1.0; la],
            pos: 0,
            sum: la as f64,
            sc: [Biquad::new(); 3],
            gr_db: 0.0,
        }
    }

    #[inline]
    fn tick(&mut self, det: f64, thr: f64, rel_c: f64, auto: bool, rel_c_auto: (f64, f64), avg_c: f64) -> f64 {
        let tg = if det > thr { thr / det } else { 1.0 };
        let rc = if auto {
            // più la riduzione media è profonda, più il rilascio è lento
            let k = (self.avg_gr * 6.0).min(1.0);
            rel_c_auto.0 + (rel_c_auto.1 - rel_c_auto.0) * k
        } else {
            rel_c
        };
        if tg < self.env {
            self.env = tg;
        } else {
            self.env += (1.0 - self.env) * rc;
        }
        self.avg_gr += ((1.0 - self.env) - self.avg_gr) * avg_c;
        let m = self.min.push(self.env);
        let la = self.ring.len();
        self.sum += m - self.ring[self.pos];
        self.ring[self.pos] = m;
        self.pos += 1;
        if self.pos == la {
            self.pos = 0;
            // evita la deriva numerica della somma mobile
            self.sum = self.ring.iter().sum();
        }
        let g = self.sum / la as f64;
        self.gr_db = lin_to_db(g);
        g
    }

    fn reset(&mut self) {
        self.env = 1.0;
        self.avg_gr = 0.0;
        for b in self.sc.iter_mut() {
            b.reset();
        }
    }
}

pub struct Limiter {
    sr: f64,
    mode: Mode,
    on: bool,
    fade: Fade,
    la: usize,
    delay: [Vec<f64>; 2],
    delay_g: [Vec<f64>; 2],
    dpos: usize,
    eng: [GainEngine; 2],
    in_g: [Smooth; 2],
    thr: [Smooth; 2],
    in_lin: [f64; 2],
    thr_lin: [f64; 2],
    rel_c: [f64; 2],
    auto: [bool; 2],
    rel_auto: (f64, f64),
    avg_c: f64,
    sc_on: [[bool; 3]; 2],
}

impl Limiter {
    pub fn new(sr: f64) -> Self {
        let la = ((LOOKAHEAD_MS * 0.001 * sr).round() as usize).max(2);
        let rate = sr / SUB as f64;
        Limiter {
            sr,
            mode: Mode::St,
            on: true,
            fade: Fade::new(sr),
            la,
            delay: [vec![0.0; la], vec![0.0; la]],
            delay_g: [vec![0.0; la], vec![0.0; la]],
            dpos: 0,
            eng: [GainEngine::new(la), GainEngine::new(la)],
            in_g: [Smooth::new(0.0, 15.0, rate); 2],
            thr: [Smooth::new(0.0, 15.0, rate); 2],
            in_lin: [1.0; 2],
            thr_lin: [1.0; 2],
            rel_c: [0.0; 2],
            auto: [true; 2],
            rel_auto: (smooth_coeff(40.0, sr), smooth_coeff(500.0, sr)),
            avg_c: smooth_coeff(250.0, sr),
            sc_on: [[false; 3]; 2],
        }
    }

    /// Latenza in campioni (il lookahead meno uno, perché il ritardo applicato è la-1).
    pub fn latency(&self) -> usize {
        self.la - 1
    }

    pub fn gr_db(&self) -> (f64, f64) {
        (self.eng[0].gr_db, self.eng[1].gr_db)
    }

    pub fn update(&mut self, p: &Module<LimiterParams>) {
        if p.mode != self.mode {
            self.mode = p.mode;
            self.eng[0].reset();
            self.eng[1].reset();
        }
        self.on = p.on;
        for ch in 0..2 {
            let sp = if self.mode == Mode::St { &p.sets[0] } else { &p.sets[ch] };
            self.in_g[ch].set(sp.in_db);
            self.thr[ch].set(sp.thresh_db);
            self.in_lin[ch] = db_to_lin(self.in_g[ch].step());
            self.thr_lin[ch] = db_to_lin(self.thr[ch].step());
            self.rel_c[ch] = smooth_coeff(sp.release_ms.max(1.0), self.sr);
            self.auto[ch] = sp.auto_release;
            let now = [sp.sc_hp, sp.sc_bell, sp.sc_lp];
            if now != self.sc_on[ch] {
                self.sc_on[ch] = now;
            }
            let e = &mut self.eng[ch];
            e.sc[0].set(if sp.sc_hp { biquad::highpass(self.sr, 150.0, 0.7071) } else { biquad::Coeffs::IDENTITY });
            e.sc[1].set(if sp.sc_bell { biquad::peaking(self.sr, 3000.0, 0.7, 6.0) } else { biquad::Coeffs::IDENTITY });
            e.sc[2].set(if sp.sc_lp { biquad::lowpass(self.sr, 8000.0, 0.7071) } else { biquad::Coeffs::IDENTITY });
        }
    }

    #[inline]
    fn detect(&mut self, ch: usize, x: f64) -> f64 {
        let mut y = x * self.in_lin[ch];
        let on = self.sc_on[ch];
        for i in 0..3 {
            if on[i] {
                y = self.eng[ch].sc[i].process(y);
            }
        }
        y.abs()
    }

    #[inline]
    pub fn process(&mut self, l: f64, r: f64) -> (f64, f64) {
        let w = self.fade.tick(self.on);
        // segnale originale ritardato di la-1 campioni, scritto in anello di lunghezza la
        let d = self.dpos;
        let (dl, dr) = (self.delay[0][d], self.delay[1][d]);
        // l'anello di lunghezza la restituisce, prima di scrivere, il campione di la passi fa;
        // il ritardo desiderato è la-1, quindi scrivo prima e leggo la posizione successiva
        self.delay[0][d] = l;
        self.delay[1][d] = r;
        let rd = if d + 1 == self.la { 0 } else { d + 1 };
        let (dl, dr) = if self.la >= 2 { (self.delay[0][rd], self.delay[1][rd]) } else { (dl, dr) };
        self.dpos = rd;

        let (a, b) = encode(self.mode, l, r);
        // il segnale già moltiplicato per il guadagno d'ingresso viene ritardato insieme al guadagno usato dal rilevatore
        self.delay_g[0][d] = a * self.in_lin[0];
        self.delay_g[1][d] = b * self.in_lin[1];
        let (da, db) = (self.delay_g[0][rd], self.delay_g[1][rd]);
        let (ga, gb);
        if self.mode == Mode::St {
            let det = self.detect(0, a).max(self.detect(1, b));
            let g = self.tick_engine(0, det);
            ga = g;
            gb = g;
        } else {
            let d0 = self.detect(0, a);
            let d1 = self.detect(1, b);
            ga = self.tick_engine(0, d0);
            gb = self.tick_engine(1, d1);
        }
        let (wa, wb) = (da * ga, db * gb);
        let (wl, wr) = decode(self.mode, wa, wb);
        if w >= 1.0 {
            (wl, wr)
        } else {
            (dl + w * (wl - dl), dr + w * (wr - dr))
        }
    }

    #[inline]
    fn tick_engine(&mut self, ch: usize, det: f64) -> f64 {
        let (thr, rc, auto, ra, ac) = (self.thr_lin[ch], self.rel_c[ch], self.auto[ch], self.rel_auto, self.avg_c);
        self.eng[ch].tick(det, thr, rc, auto, ra, ac)
    }
}
