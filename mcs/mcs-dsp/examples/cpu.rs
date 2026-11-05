use mcs_dsp::*;
use std::time::Instant;
fn main() {
    for os in [0usize, 1, 2, 3] {
        let mut ch = Chain::new(48000.0);
        let mut p = ChainParams::default();
        p.os_log2 = os;
        for b in 0..4 { p.tone.sets[0].bands[b].gain_db = 3.0; }
        p.limiter.sets[0].in_db = 6.0;
        p.limiter.sets[0].thresh_db = -3.0;
        p.filter.sets[0].pres_db = 2.0;
        p.eqf.sets[0].hp_freq = 30.0;
        p.eqf.sets[0].lp_freq = 18000.0;
        p.clipper.sets[0].drive_db = 6.0;
        let n = 48000 * 20;
        let mut l: Vec<f32> = (0..n).map(|i| ((i as f32) * 0.05).sin() * 0.7).collect();
        let mut r = l.clone();
        let t = Instant::now();
        for (cl, cr) in l.chunks_mut(512).zip(r.chunks_mut(512)) { ch.process(&p, cl, cr); }
        let s = t.elapsed().as_secs_f64();
        println!("oversampling {}x: {:.2}% di un core a 48 kHz stereo", 1 << os, s / 20.0 * 100.0);
    }
}
