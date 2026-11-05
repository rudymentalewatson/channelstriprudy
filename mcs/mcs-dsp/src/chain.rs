//! Catena completa: Input, Tone, Limiter, Filter, EQ finale, Clipper, Output, più Analysis.

use crate::clipper::{Clipper, ClipperParams};
use crate::filters::*;
use crate::input_output::*;
use crate::limiter::{Limiter, LimiterParams};
use crate::loudness::Analysis;
use crate::tone::{Tone, ToneParams};
use crate::util::*;

#[derive(Clone, Copy, Debug)]
pub struct ChainParams {
    pub input: InputParams,
    pub tone: Module<ToneParams>,
    pub limiter: Module<LimiterParams>,
    pub filter: Module<FilterParams>,
    pub eqf: Module<EqFinaleParams>,
    pub clipper: Module<ClipperParams>,
    pub output: OutputParams,
    /// Oversampling del clipper: 0 = off, 1 = 2x, 2 = 4x, 3 = 8x, 4 = 16x.
    pub os_log2: usize,
}

impl Default for ChainParams {
    fn default() -> Self {
        ChainParams {
            input: Default::default(),
            tone: Default::default(),
            limiter: Default::default(),
            filter: Default::default(),
            eqf: Default::default(),
            clipper: Default::default(),
            output: Default::default(),
            os_log2: 1,
        }
    }
}

pub struct Chain {
    pub input: Input,
    pub tone: Tone,
    pub limiter: Limiter,
    pub filter: Filter,
    pub eqf: EqFinale,
    pub clipper: Clipper,
    pub output: Output,
    pub analysis: Analysis,
}

impl Chain {
    pub fn new(sr: f64) -> Self {
        let mut c = Chain {
            input: Input::new(sr),
            tone: Tone::new(sr),
            limiter: Limiter::new(sr),
            filter: Filter::new(sr),
            eqf: EqFinale::new(sr),
            clipper: Clipper::new(sr),
            output: Output::new(sr),
            analysis: Analysis::new(sr),
        };
        c.clipper.set_oversampling(1);
        c
    }

    /// Latenza totale in campioni.
    pub fn latency(&self) -> usize {
        self.limiter.latency() + self.clipper.latency()
    }

    /// Elabora un blocco in f32 sul posto.
    pub fn process(&mut self, p: &ChainParams, l: &mut [f32], r: &mut [f32]) {
        self.clipper.set_oversampling(p.os_log2);
        let n = l.len().min(r.len());
        let mut i = 0;
        while i < n {
            let end = (i + SUB).min(n);
            self.input.update(&p.input);
            self.tone.update(&p.tone);
            self.limiter.update(&p.limiter);
            self.filter.update(&p.filter);
            self.eqf.update(&p.eqf);
            self.clipper.update(&p.clipper);
            self.output.update(&p.output);
            for k in i..end {
                let (a, b) = self.input.process(l[k] as f64, r[k] as f64);
                let (a, b) = self.tone.process(a, b);
                let (a, b) = self.limiter.process(a, b);
                let (a, b) = self.filter.process(a, b);
                let (a, b) = self.eqf.process(a, b);
                let (a, b) = self.clipper.process(a, b);
                let (a, b) = self.output.process(a, b);
                self.analysis.push(a, b);
                l[k] = a as f32;
                r[k] = b as f32;
            }
            i = end;
        }
        self.analysis.update_fast();
    }
}
