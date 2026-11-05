//! Oversampling con filtri half-band a fase lineare (struttura polifase).
//! Ogni stadio raddoppia la frequenza; fino a 4 stadi in cascata (2x, 4x, 8x, 16x).

use std::f64::consts::PI;

fn bessel_i0(x: f64) -> f64 {
    let mut sum = 1.0;
    let mut term = 1.0;
    let h = x * 0.5;
    for k in 1..60 {
        term *= (h / k as f64) * (h / k as f64);
        sum += term;
        if term < 1e-18 * sum {
            break;
        }
    }
    sum
}

/// Buffer circolare di lunghezza potenza di due.
#[derive(Clone, Debug)]
struct Ring {
    buf: Vec<f64>,
    pos: usize,
    mask: usize,
}

impl Ring {
    fn new(n: usize) -> Self {
        assert!(n.is_power_of_two());
        Ring { buf: vec![0.0; n], pos: 0, mask: n - 1 }
    }
    #[inline]
    fn push(&mut self, x: f64) {
        self.pos = (self.pos + 1) & self.mask;
        self.buf[self.pos] = x;
    }
    /// k = 0: il campione più recente.
    #[inline]
    fn get(&self, k: usize) -> f64 {
        self.buf[(self.pos + self.buf.len() - k) & self.mask]
    }
    fn reset(&mut self) {
        for v in self.buf.iter_mut() {
            *v = 0.0;
        }
    }
}

/// Un raddoppio di frequenza: `p` è il numero di coppie di tap dispari (2p tap utili).
#[derive(Clone, Debug)]
pub struct Stage {
    p: usize,
    /// Tap pari della forma causale: e[i] = c[2i], con somma 1 (guadagno dell'up).
    e: Vec<f64>,
    up_hist: Ring,
    down_a: Ring,
    down_b: Ring,
}

impl Stage {
    pub fn new(p: usize, beta: f64) -> Self {
        assert!(p.is_power_of_two() && p >= 4);
        let half_len = (2 * p) as f64;
        let i0b = bessel_i0(beta);
        let mut e = vec![0.0; 2 * p];
        // offset dispari n = 2i - (2p - 1), per i = 0..2p
        for (i, tap) in e.iter_mut().enumerate() {
            let n = 2.0 * i as f64 - (2.0 * p as f64 - 1.0);
            let w = bessel_i0(beta * (1.0 - (n / half_len).powi(2)).max(0.0).sqrt()) / i0b;
            let h = (PI * n / 2.0).sin() / (PI * n) * w;
            *tap = 2.0 * h;
        }
        // normalizza il guadagno DC delle fasi dispari a 1 (somma dei tap pari = 1)
        let s: f64 = e.iter().sum();
        for t in e.iter_mut() {
            *t /= s;
        }
        Stage { p, e, up_hist: Ring::new(2 * p), down_a: Ring::new(2 * p), down_b: Ring::new(2 * p) }
    }

    pub fn reset(&mut self) {
        self.up_hist.reset();
        self.down_a.reset();
        self.down_b.reset();
    }

    /// Un campione in ingresso, due in uscita (prima la fase FIR, poi quella ritardata).
    #[inline]
    fn up(&mut self, x: f64) -> (f64, f64) {
        self.up_hist.push(x);
        let mut acc = 0.0;
        for i in 0..2 * self.p {
            acc += self.e[i] * self.up_hist.get(i);
        }
        (acc, self.up_hist.get(self.p - 1))
    }

    /// Due campioni in ingresso (nello stesso ordine prodotto da `up`), uno in uscita.
    #[inline]
    fn down(&mut self, a: f64, b: f64) -> f64 {
        self.down_a.push(a);
        self.down_b.push(b);
        let mut acc = 0.5 * self.down_a.get(self.p - 1);
        for i in 0..2 * self.p {
            acc += 0.5 * self.e[i] * self.down_b.get(i);
        }
        acc
    }
}

fn run<F: FnMut(f64) -> f64>(stages: &mut [Stage], x: f64, f: &mut F) -> f64 {
    match stages.split_first_mut() {
        None => f(x),
        Some((s, rest)) => {
            let (a, b) = s.up(x);
            let ya = run(rest, a, f);
            let yb = run(rest, b, f);
            s.down(ya, yb)
        }
    }
}

/// Oversampler fino a 16x. Il primo stadio ha il filtro più ripido, i successivi sono più corti.
#[derive(Clone, Debug)]
pub struct Oversampler {
    stages: Vec<Stage>,
    active: usize,
}

impl Default for Oversampler {
    fn default() -> Self {
        Self::new()
    }
}

impl Oversampler {
    pub fn new() -> Self {
        Oversampler {
            stages: vec![Stage::new(16, 9.0), Stage::new(8, 8.0), Stage::new(8, 8.0), Stage::new(8, 8.0)],
            active: 0,
        }
    }

    /// 0 = nessun oversampling, 1 = 2x, 2 = 4x, 3 = 8x, 4 = 16x.
    pub fn set_factor_log2(&mut self, n: usize) {
        let n = n.min(4);
        if n != self.active {
            self.active = n;
            self.reset();
        }
    }

    pub fn factor_log2(&self) -> usize {
        self.active
    }

    pub fn reset(&mut self) {
        for s in self.stages.iter_mut() {
            s.reset();
        }
    }

    #[inline]
    pub fn process<F: FnMut(f64) -> f64>(&mut self, x: f64, f: &mut F) -> f64 {
        let n = self.active;
        run(&mut self.stages[..n], x, f)
    }
}

/// Latenza (in campioni alla frequenza base) di un oversampler con `log2` stadi, misurata
/// con una risposta all'impulso. Da chiamare solo in inizializzazione.
pub fn measure_latency(log2: usize) -> usize {
    if log2 == 0 {
        return 0;
    }
    let mut os = Oversampler::new();
    os.set_factor_log2(log2);
    let mut best = (0usize, 0.0f64);
    for n in 0..512 {
        let x = if n == 0 { 1.0 } else { 0.0 };
        let y = os.process(x, &mut |v| v);
        if y.abs() > best.1 {
            best = (n, y.abs());
        }
    }
    best.0
}
