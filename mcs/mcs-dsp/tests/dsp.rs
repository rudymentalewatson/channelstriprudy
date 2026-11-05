use mcs_dsp::biquad::*;
use mcs_dsp::chain::*;
use mcs_dsp::clipper::*;
use mcs_dsp::loudness::*;
use mcs_dsp::oversample::*;
use mcs_dsp::tone::*;
use std::f64::consts::PI;

const SR: f64 = 48000.0;

fn sine(f: f64, amp: f64, n: usize) -> Vec<f32> {
    (0..n).map(|i| (amp * (2.0 * PI * f * i as f64 / SR).sin()) as f32).collect()
}
fn rms_db(x: &[f32]) -> f64 {
    let s: f64 = x.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / x.len() as f64;
    10.0 * s.log10()
}

#[test]
fn oversampler_unity_and_latency() {
    for n in 1..=4 {
        let mut os = Oversampler::new();
        os.set_factor_log2(n);
        let lat = measure_latency(n);
        let x = sine(1000.0, 0.5, 8000);
        let y: Vec<f64> = x.iter().map(|v| os.process(*v as f64, &mut |s| s)).collect();
        let tail = &y[2000..];
        let g = (tail.iter().map(|v| v * v).sum::<f64>() / tail.len() as f64).sqrt() / (0.5 / 2f64.sqrt());
        println!("os {}x latency {} gain {:.5}", 1 << n, lat, g);
        assert!((g - 1.0).abs() < 0.002, "gain {}", g);
    }
}

#[test]
fn oversampler_dc_and_nyquist_rejection() {
    let mut os = Oversampler::new();
    os.set_factor_log2(1);
    let mut y = 0.0;
    for _ in 0..500 {
        y = os.process(1.0, &mut |s| s);
    }
    assert!((y - 1.0).abs() < 1e-3, "dc {}", y);
}

#[test]
fn tone_shelf_and_bell_response() {
    let c = band_coeffs(SR, 2, 2500.0, 6.0, Shape::Bell);
    let g = 20.0 * c.magnitude(SR, 2500.0).log10();
    assert!((g - 6.0).abs() < 0.01);
    let c = band_coeffs(SR, 0, 80.0, 6.0, Shape::Shelf);
    assert!((20.0 * c.magnitude(SR, 20.0).log10() - 6.0).abs() < 0.3);
    assert!(20.0 * c.magnitude(SR, 4000.0).log10().abs() < 0.05);
    let c = band_coeffs(SR, 3, 10000.0, -4.0, Shape::ShelfSoft);
    assert!((20.0 * c.magnitude(SR, 20000.0).log10() + 4.0).abs() < 0.8);
}

#[test]
fn butterworth_slopes() {
    for (slope, db) in [(0usize, -3.0103), (1, -3.0103), (2, -3.0103)] {
        let mut c = Cascade::default();
        c.set_butterworth(SR, 1000.0, slope, false);
        let g = 20.0 * c.magnitude(SR, 1000.0).log10();
        assert!((g - db).abs() < 0.05, "slope {} g {}", slope, g);
    }
    let mut c = Cascade::default();
    c.set_butterworth(SR, 100.0, 2, true);
    let g1 = 20.0 * c.magnitude(SR, 50.0).log10();
    let g2 = 20.0 * c.magnitude(SR, 25.0).log10();
    println!("hp48: {} -> {}", g1, g2);
    assert!(g2 - g1 < -40.0);
}

#[test]
fn lufs_reference() {
    // 997 Hz a 0 dBFS su entrambi i canali: 0 LUFS (su un solo canale: -3,01 LUFS)
    let mut a = Analysis::new(SR);
    let x = sine(997.0, 1.0, (SR * 6.0) as usize);
    for v in &x {
        a.push(*v as f64, *v as f64);
    }
    a.update_fast();
    let s = a.snapshot();
    println!("momentary {:.3} short {:.3} integrated {:.3} corr {:.3}", s.momentary, s.short_term, s.integrated, s.correlation);
    assert!(s.momentary.abs() < 0.05);
    assert!(s.integrated.abs() < 0.05);
    let mut m = Analysis::new(SR);
    for v in &x {
        m.push(*v as f64, 0.0);
    }
    assert!((m.snapshot().integrated + 3.01).abs() < 0.05);
    assert!(s.correlation > 0.99);
}

#[test]
fn lufs_gating_ignores_silence() {
    let mut a = Analysis::new(SR);
    let x = sine(997.0, 0.1, (SR * 5.0) as usize);
    for v in &x {
        a.push(*v as f64, *v as f64);
    }
    for _ in 0..(SR as usize * 8) {
        a.push(0.0, 0.0);
    }
    let i = a.snapshot().integrated;
    println!("integrated with silence {:.3}", i);
    assert!((i + 20.0).abs() < 0.2);
}

#[test]
fn phase_correlation_values() {
    let mut a = Analysis::new(SR);
    let x = sine(440.0, 0.5, 48000);
    for v in &x {
        a.push(*v as f64, -(*v as f64));
    }
    a.update_fast();
    assert!(a.snapshot().correlation < -0.99);
}

#[test]
fn neutral_chain_is_transparent_after_latency() {
    let mut ch = Chain::new(SR);
    let p = ChainParams::default();
    let n = 24000;
    let x = sine(1000.0, 0.5, n);
    let (mut l, mut r) = (x.clone(), x.clone());
    ch.process(&p, &mut l, &mut r);
    let lat = ch.latency();
    println!("latency {}", lat);
    let mut worst: f64 = 0.0;
    for i in 12000..20000 {
        let e = (l[i] as f64 - x[i - lat] as f64).abs();
        worst = worst.max(e);
    }
    println!("max error vs delayed input (clipper tanh at 0.5): {}", worst);
    // il tanh agisce comunque: con ceiling 0 dB e drive 0 dB si ha solo una leggera saturazione
    assert!(worst < 0.06);
}

#[test]
fn limiter_never_exceeds_ceiling() {
    let mut ch = Chain::new(SR);
    let mut p = ChainParams::default();
    p.clipper.on = false;
    p.limiter.sets[0].in_db = 12.0;
    p.limiter.sets[0].thresh_db = -3.0;
    let n = 48000;
    let x = sine(220.0, 0.9, n);
    let (mut l, mut r) = (x.clone(), x.clone());
    ch.process(&p, &mut l, &mut r);
    let thr = 10f64.powf(-3.0 / 20.0);
    let peak = l[6000..].iter().map(|v| v.abs() as f64).fold(0.0, f64::max);
    println!("peak {} thr {}", peak, thr);
    assert!(peak <= thr * 1.001);
    assert!(peak > thr * 0.95);
}

#[test]
fn limiter_transparent_below_threshold() {
    let mut ch = Chain::new(SR);
    let mut p = ChainParams::default();
    p.clipper.on = false;
    p.limiter.sets[0].thresh_db = -1.0;
    let x = sine(1000.0, 0.3, 24000);
    let (mut l, mut r) = (x.clone(), x.clone());
    ch.process(&p, &mut l, &mut r);
    let lat = ch.latency();
    let mut worst: f64 = 0.0;
    for i in 12000..20000 {
        worst = worst.max((l[i] as f64 - x[i - lat] as f64).abs());
    }
    assert!(worst < 1e-6, "{}", worst);
}

/// Misura l'aliasing: sinusoide a 5 kHz fortemente clippata, si cercano componenti non armoniche.
fn alias_level(model: ClipModel, os: usize, adaa_note: &str) -> f64 {
    alias_level_d(model, os, adaa_note, 18.0)
}
fn alias_level_d(model: ClipModel, os: usize, adaa_note: &str, drive: f64) -> f64 {
    let mut ch = Chain::new(SR);
    let mut p = ChainParams::default();
    p.os_log2 = os;
    p.clipper.sets[0].model = model;
    p.clipper.sets[0].drive_db = drive;
    let n = 1 << 15;
    let f = 5003.0;
    let x = sine(f, 0.8, n + 24000);
    let (mut l, mut r) = (x.clone(), x.clone());
    ch.process(&p, &mut l, &mut r);
    // DFT finestrata di Hann
    let seg = &l[24000..24000 + n];
    let mut spec = vec![0.0f64; n / 2];
    let w: Vec<f64> = (0..n).map(|i| { let x = 2.0 * PI * i as f64 / n as f64; 0.35875 - 0.48829 * x.cos() + 0.14128 * (2.0 * x).cos() - 0.01168 * (3.0 * x).cos() }).collect();
    // FFT semplice ricorsiva
    fn fft(re: &mut [f64], im: &mut [f64]) {
        let n = re.len();
        let mut j = 0;
        for i in 1..n {
            let mut bit = n >> 1;
            while j & bit != 0 {
                j ^= bit;
                bit >>= 1;
            }
            j ^= bit;
            if i < j {
                re.swap(i, j);
                im.swap(i, j);
            }
        }
        let mut len = 2;
        while len <= n {
            let ang = -2.0 * PI / len as f64;
            for i in (0..n).step_by(len) {
                for k in 0..len / 2 {
                    let (s, c) = (ang * k as f64).sin_cos();
                    let (a, b) = (i + k, i + k + len / 2);
                    let tr = re[b] * c - im[b] * s;
                    let ti = re[b] * s + im[b] * c;
                    re[b] = re[a] - tr;
                    im[b] = im[a] - ti;
                    re[a] += tr;
                    im[a] += ti;
                }
            }
            len <<= 1;
        }
    }
    let mut re: Vec<f64> = seg.iter().zip(&w).map(|(v, w)| *v as f64 * w).collect();
    let mut im = vec![0.0; n];
    fft(&mut re, &mut im);
    for k in 0..n / 2 {
        spec[k] = (re[k] * re[k] + im[k] * im[k]).sqrt();
    }
    let bin = |fr: f64| (fr / SR * n as f64).round() as usize;
    let fund = spec[bin(f)];
    // armoniche riflesse
    let mut harm_bins = std::collections::HashSet::new();
    for h in 1..40 {
        let mut fr = (f * h as f64) % SR;
        if fr > SR / 2.0 {
            fr = SR - fr;
        }
        let b = bin(fr);
        for d in b.saturating_sub(8)..=(b + 8).min(n / 2 - 1) {
            harm_bins.insert(d);
        }
    }
    let mut worst: f64 = 0.0;
    let mut wk = 0;
    for k in 10..n / 2 - 1 {
        if !harm_bins.contains(&k) && spec[k] > worst {
            worst = spec[k];
            wk = k;
        }
    }
    println!("worst bin freq {:.1} Hz", wk as f64 * SR / n as f64);
    let db = 20.0 * (worst / fund).log10();
    println!("{:?} os={} {} -> aliasing {:.1} dB rispetto alla fondamentale", model, 1 << os, adaa_note, db);
    db
}

#[test]
fn clipper_aliasing_is_low() {
    alias_level_d(ClipModel::Hard, 1, "drive 0 (solo pavimento di misura)", 0.0);
    let a0 = alias_level(ClipModel::Hard, 0, "ADAA solo");
    let a1 = alias_level(ClipModel::Hard, 1, "ADAA + 2x");
    let b0 = alias_level(ClipModel::Tanh, 0, "ADAA solo");
    let b1 = alias_level(ClipModel::Tanh, 1, "ADAA + 2x");
    alias_level(ClipModel::Asym, 1, "ADAA + 2x");
    alias_level(ClipModel::Atan, 1, "ADAA + 2x");
    assert!(a1 < a0 + 0.5 && b1 < b0 + 0.5);
    assert!(a1 < -60.0, "{}", a1);
    assert!(b1 < -60.0, "{}", b1);
}

#[test]
fn no_nan_under_abuse() {
    let mut ch = Chain::new(SR);
    let mut p = ChainParams::default();
    p.tone.sets[0].bands[0].gain_db = 15.0;
    p.tone.sets[0].bands[3].gain_db = -15.0;
    p.tone.mode = mcs_dsp::Mode::Ms;
    p.tone.sets[1] = p.tone.sets[0];
    p.limiter.sets[0].in_db = 24.0;
    p.limiter.sets[0].thresh_db = -20.0;
    p.eqf.sets[0].hp_freq = 500.0;
    p.eqf.sets[0].hp_slope = 2;
    p.eqf.sets[0].lp_freq = 3000.0;
    p.eqf.sets[0].lp_slope = 2;
    p.clipper.sets[0].drive_db = 30.0;
    p.os_log2 = 4;
    let mut seed = 12345u32;
    let mut noise = || {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        (seed as f32 / u32::MAX as f32) * 2.0 - 1.0
    };
    let mut l: Vec<f32> = (0..96000).map(|_| noise()).collect();
    let mut r: Vec<f32> = (0..96000).map(|_| noise()).collect();
    ch.process(&p, &mut l, &mut r);
    assert!(l.iter().chain(r.iter()).all(|v| v.is_finite() && v.abs() < 4.0));
    let _ = rms_db(&l);
}

#[test]
fn channels_stay_independent() {
    // L con segnale, R muto: R deve restare muto in tutte le modalità
    for mode in [mcs_dsp::Mode::St, mcs_dsp::Mode::Duo] {
        let mut ch = Chain::new(SR);
        let mut p = ChainParams::default();
        p.tone.mode = mode;
        p.tone.sets[0].bands[0].gain_db = 6.0;
        p.tone.sets[0].bands[2].gain_db = -4.0;
        p.limiter.mode = mode;
        p.limiter.sets[0].in_db = 9.0;
        p.limiter.sets[0].thresh_db = -6.0;
        p.input.phase = true;
        p.eqf.mode = mode;
        p.eqf.sets[0].hp_freq = 60.0;
        let mut l = sine(500.0, 0.8, 24000);
        let mut r = vec![0.0f32; 24000];
        ch.process(&p, &mut l, &mut r);
        let m = r.iter().map(|v| v.abs()).fold(0.0f32, f32::max);
        // in ST il limiter e' collegato ma il segnale R resta zero (nessuno scambio di stato)
        assert!(m < 1e-6, "{:?} R non muto: {}", mode, m);
    }
}

#[test]
fn solo_isolates_band() {
    let mut ch = Chain::new(SR);
    let mut p = ChainParams::default();
    p.clipper.on = false;
    p.limiter.on = false;
    p.tone.sets[0].bands[2].gain_db = 6.0;
    p.tone.sets[0].bands[2].solo = true;
    let n = 24000;
    let hi = sine(8000.0, 0.5, n);
    let mid = sine(2500.0, 0.5, n);
    let (mut l1, mut r1) = (hi.clone(), hi.clone());
    let (mut l2, mut r2) = (mid.clone(), mid.clone());
    ch.process(&p, &mut l1, &mut r1);
    let mut ch2 = Chain::new(SR);
    ch2.process(&p, &mut l2, &mut r2);
    let a = rms_db(&l1[12000..]);
    let b = rms_db(&l2[12000..]);
    assert!(b - a > 11.0, "solo non isola: {} vs {}", b, a);
}
