//! Analisi: picco, RMS, correlazione di fase e LUFS (ITU-R BS.1770 / EBU R128).

use crate::biquad::{Biquad, Coeffs};
use std::f64::consts::PI;

/// Filtri di ponderazione K (coefficienti analogici di libebur128, validi per ogni frequenza di campionamento).
pub fn k_weighting(sr: f64) -> (Coeffs, Coeffs) {
    let f0 = 1681.974450955533;
    let g = 3.999843853973347;
    let q = 0.7071752369554196;
    let k = (PI * f0 / sr).tan();
    let vh = 10f64.powf(g / 20.0);
    let vb = vh.powf(0.4996667741545416);
    let a0 = 1.0 + k / q + k * k;
    let shelf = Coeffs {
        b0: (vh + vb * k / q + k * k) / a0,
        b1: 2.0 * (k * k - vh) / a0,
        b2: (vh - vb * k / q + k * k) / a0,
        a1: 2.0 * (k * k - 1.0) / a0,
        a2: (1.0 - k / q + k * k) / a0,
    };
    let f0 = 38.13547087602444;
    let q = 0.5003270373238773;
    let k = (PI * f0 / sr).tan();
    let a0 = 1.0 + k / q + k * k;
    let rlb = Coeffs { b0: 1.0, b1: -2.0, b2: 1.0, a1: 2.0 * (k * k - 1.0) / a0, a2: (1.0 - k / q + k * k) / a0 };
    (shelf, rlb)
}

const HIST_MIN: f64 = -70.0;
const HIST_BINS: usize = 1000; // da -70 a +30 LUFS, 0,1 LU per bin

fn energy_to_lufs(e: f64) -> f64 {
    -0.691 + 10.0 * e.max(1e-20).log10()
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Snapshot {
    pub momentary: f64,
    pub short_term: f64,
    pub integrated: f64,
    pub peak_l: f64,
    pub peak_r: f64,
    pub rms_l_db: f64,
    pub rms_r_db: f64,
    pub correlation: f64,
}

pub struct Analysis {
    sr: f64,
    kf: [[Biquad; 2]; 2],
    sub_len: usize,
    sub_count: usize,
    sub_sum: f64,
    sub_ring: [f64; 30],
    sub_pos: usize,
    sub_filled: usize,
    hist_n: Vec<u64>,
    hist_e: Vec<f64>,
    // misure rapide (non ponderate)
    sll: f64,
    srr: f64,
    slr: f64,
    corr_c: f64,
    rms_l: f64,
    rms_r: f64,
    rms_c: f64,
    peak_l: f64,
    peak_r: f64,
    peak_decay: f64,
    snap: Snapshot,
}

impl Analysis {
    pub fn new(sr: f64) -> Self {
        let (s, r) = k_weighting(sr);
        let mut kf = [[Biquad::new(); 2]; 2];
        for ch in 0..2 {
            kf[ch][0].set(s);
            kf[ch][1].set(r);
        }
        Analysis {
            sr,
            kf,
            sub_len: (sr * 0.1).round() as usize,
            sub_count: 0,
            sub_sum: 0.0,
            sub_ring: [0.0; 30],
            sub_pos: 0,
            sub_filled: 0,
            hist_n: vec![0; HIST_BINS],
            hist_e: vec![0.0; HIST_BINS],
            sll: 0.0,
            srr: 0.0,
            slr: 0.0,
            corr_c: 1.0 - (-1.0 / (0.3 * sr)).exp(),
            rms_l: 0.0,
            rms_r: 0.0,
            rms_c: 1.0 - (-1.0 / (0.3 * sr)).exp(),
            peak_l: 0.0,
            peak_r: 0.0,
            peak_decay: (-1.0 / (1.0 * sr)).exp(),
            snap: Snapshot { momentary: -120.0, short_term: -120.0, integrated: -120.0, ..Default::default() },
        }
    }

    pub fn reset_integrated(&mut self) {
        for v in self.hist_n.iter_mut() {
            *v = 0;
        }
        for v in self.hist_e.iter_mut() {
            *v = 0.0;
        }
    }

    pub fn snapshot(&self) -> Snapshot {
        self.snap
    }

    fn window_energy(&self, n: usize) -> f64 {
        let mut s = 0.0;
        for i in 0..n {
            s += self.sub_ring[(self.sub_pos + 30 - 1 - i) % 30];
        }
        s / n as f64
    }

    fn finish_sub(&mut self) {
        self.sub_ring[self.sub_pos] = self.sub_sum / self.sub_len as f64;
        self.sub_pos = (self.sub_pos + 1) % 30;
        self.sub_filled = (self.sub_filled + 1).min(30);
        self.sub_sum = 0.0;
        if self.sub_filled >= 4 {
            let e = self.window_energy(4);
            self.snap.momentary = energy_to_lufs(e);
            let l = energy_to_lufs(e);
            if l > HIST_MIN {
                let bin = (((l - HIST_MIN) * 10.0) as usize).min(HIST_BINS - 1);
                self.hist_n[bin] += 1;
                self.hist_e[bin] += e;
            }
            self.snap.integrated = self.integrated();
        }
        if self.sub_filled >= 30 {
            self.snap.short_term = energy_to_lufs(self.window_energy(30));
        } else if self.sub_filled >= 4 {
            self.snap.short_term = energy_to_lufs(self.window_energy(self.sub_filled));
        }
    }

    pub fn integrated(&self) -> f64 {
        let (mut n, mut e) = (0u64, 0.0);
        for i in 0..HIST_BINS {
            n += self.hist_n[i];
            e += self.hist_e[i];
        }
        if n == 0 {
            return -120.0;
        }
        let rel = energy_to_lufs(e / n as f64) - 10.0;
        let start = if rel > HIST_MIN { (((rel - HIST_MIN) * 10.0).ceil() as usize).min(HIST_BINS) } else { 0 };
        let (mut n2, mut e2) = (0u64, 0.0);
        for i in start..HIST_BINS {
            n2 += self.hist_n[i];
            e2 += self.hist_e[i];
        }
        if n2 == 0 {
            -120.0
        } else {
            energy_to_lufs(e2 / n2 as f64)
        }
    }

    #[inline]
    pub fn push(&mut self, l: f64, r: f64) {
        let xs = [l, r];
        let mut z = [0.0; 2];
        for ch in 0..2 {
            let t = self.kf[ch][0].process(xs[ch]);
            let y = self.kf[ch][1].process(t);
            z[ch] = y * y;
        }
        self.sub_sum += z[0] + z[1];
        self.sub_count += 1;
        if self.sub_count >= self.sub_len {
            self.sub_count = 0;
            self.finish_sub();
        }
        self.sll += (l * l - self.sll) * self.corr_c;
        self.srr += (r * r - self.srr) * self.corr_c;
        self.slr += (l * r - self.slr) * self.corr_c;
        self.rms_l += (l * l - self.rms_l) * self.rms_c;
        self.rms_r += (r * r - self.rms_r) * self.rms_c;
        self.peak_l = (self.peak_l * self.peak_decay).max(l.abs());
        self.peak_r = (self.peak_r * self.peak_decay).max(r.abs());
    }

    /// Da chiamare una volta per blocco audio: aggiorna i valori veloci dello snapshot.
    pub fn update_fast(&mut self) {
        let d = (self.sll * self.srr).sqrt();
        self.snap.correlation = if d > 1e-12 { (self.slr / d).clamp(-1.0, 1.0) } else { 0.0 };
        self.snap.rms_l_db = 10.0 * self.rms_l.max(1e-12).log10();
        self.snap.rms_r_db = 10.0 * self.rms_r.max(1e-12).log10();
        self.snap.peak_l = self.peak_l;
        self.snap.peak_r = self.peak_r;
        let _ = self.sr;
    }
}
