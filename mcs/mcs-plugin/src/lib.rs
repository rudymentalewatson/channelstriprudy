//! Multi Channel Strip: collegamento tra nih-plug e il nucleo DSP (mcs-dsp).

use mcs_dsp::chain::{Chain, ChainParams};
use mcs_dsp::clipper::{ClipModel, ClipperParams};
use mcs_dsp::filters::{EqFinaleParams, FilterParams};
use mcs_dsp::limiter::LimiterParams;
use mcs_dsp::tone::{Shape, ToneBand, ToneParams};
use mcs_dsp::{Mode, Module};
use nih_plug::prelude::*;
use nih_plug_egui::{create_egui_editor, EguiState};
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering::Relaxed};
use std::sync::Arc;

mod gui;

/// Dati condivisi tra il thread audio e la grafica (misure).
pub struct Shared {
    pub momentary: AtomicF32,
    pub short_term: AtomicF32,
    pub integrated: AtomicF32,
    pub peak_l: AtomicF32,
    pub peak_r: AtomicF32,
    pub rms_l: AtomicF32,
    pub rms_r: AtomicF32,
    pub corr: AtomicF32,
    pub gr: AtomicF32,
    pub reset_lufs: AtomicBool,
    /// Contatori di diagnosi.
    pub calls: AtomicU32,
    pub no_chain: AtomicU32,
    pub chans: AtomicU32,
    pub len: AtomicU32,
    pub inits: AtomicU32,
    pub resets: AtomicU32,
    pub sr: AtomicF32,
    pub scope: Mutex<Vec<(f32, f32)>>,
}

impl Default for Shared {
    fn default() -> Self {
        let z = || AtomicF32::new(0.0);
        Shared {
            momentary: AtomicF32::new(-120.0),
            short_term: AtomicF32::new(-120.0),
            integrated: AtomicF32::new(-120.0),
            peak_l: z(),
            peak_r: z(),
            rms_l: AtomicF32::new(-120.0),
            rms_r: AtomicF32::new(-120.0),
            corr: z(),
            gr: z(),
            reset_lufs: AtomicBool::new(false),
            calls: AtomicU32::new(0),
            no_chain: AtomicU32::new(0),
            chans: AtomicU32::new(0),
            len: AtomicU32::new(0),
            inits: AtomicU32::new(0),
            resets: AtomicU32::new(0),
            sr: AtomicF32::new(0.0),
            scope: Mutex::new(Vec::with_capacity(512)),
        }
    }
}

#[derive(Enum, PartialEq, Clone, Copy)]
pub enum ModeP {
    #[id = "st"]
    #[name = "ST"]
    St,
    #[id = "duo"]
    #[name = "DUO"]
    Duo,
    #[id = "ms"]
    #[name = "MS"]
    Ms,
}
impl From<ModeP> for Mode {
    fn from(m: ModeP) -> Mode {
        match m {
            ModeP::St => Mode::St,
            ModeP::Duo => Mode::Duo,
            ModeP::Ms => Mode::Ms,
        }
    }
}

#[derive(Enum, PartialEq, Clone, Copy)]
pub enum ShapeP {
    #[id = "shelf"]
    #[name = "Shelf"]
    Shelf,
    #[id = "shelf_soft"]
    #[name = "Shelf morbido"]
    ShelfSoft,
    #[id = "bell"]
    #[name = "Campana"]
    Bell,
    #[id = "bell_wide"]
    #[name = "Campana larga"]
    BellWide,
    #[id = "bell_narrow"]
    #[name = "Campana stretta"]
    BellNarrow,
}

#[derive(Enum, PartialEq, Clone, Copy)]
pub enum ClipP {
    #[id = "hard"]
    #[name = "Duro"]
    Hard,
    #[id = "tanh"]
    #[name = "Morbido"]
    Tanh,
    #[id = "asym"]
    #[name = "Asimmetrico"]
    Asym,
    #[id = "atan"]
    #[name = "Arctan"]
    Atan,
}

#[derive(Enum, PartialEq, Clone, Copy)]
pub enum SlopeP {
    #[id = "s12"]
    #[name = "12 dB/ott"]
    S12,
    #[id = "s24"]
    #[name = "24 dB/ott"]
    S24,
    #[id = "s48"]
    #[name = "48 dB/ott"]
    S48,
}

#[derive(Enum, PartialEq, Clone, Copy)]
pub enum OsP {
    #[id = "x1"]
    #[name = "Off"]
    X1,
    #[id = "x2"]
    #[name = "2x"]
    X2,
    #[id = "x4"]
    #[name = "4x"]
    X4,
    #[id = "x8"]
    #[name = "8x"]
    X8,
    #[id = "x16"]
    #[name = "16x"]
    X16,
}

fn db(name: &str, default: f32, min: f32, max: f32) -> FloatParam {
    FloatParam::new(name, default, FloatRange::Linear { min, max })
        .with_unit(" dB")
        .with_step_size(0.1)
}

fn hz(name: &str, default: f32, min: f32, max: f32) -> FloatParam {
    FloatParam::new(name, default, FloatRange::Skewed { min, max, factor: FloatRange::skew_factor(-2.0) })
        .with_unit(" Hz")
        .with_value_to_string(formatters::v2s_f32_rounded(0))
}

#[derive(Params)]
pub struct BandParams {
    #[id = "on"]
    pub on: BoolParam,
    #[id = "gain"]
    pub gain: FloatParam,
    #[id = "freq"]
    pub freq: FloatParam,
    #[id = "shape"]
    pub shape: EnumParam<ShapeP>,
    #[id = "solo"]
    pub solo: BoolParam,
}

fn band(name: &str, f: f32, lo: f32, hi: f32, shape: ShapeP) -> BandParams {
    BandParams {
        on: BoolParam::new(format!("{name} On"), true),
        gain: db(&format!("{name} Gain"), 0.0, -15.0, 15.0),
        freq: hz(&format!("{name} Freq"), f, lo, hi),
        shape: EnumParam::new(format!("{name} Forma"), shape),
        solo: BoolParam::new(format!("{name} Solo"), false),
    }
}

/// Parametri che esistono in due copie (set A e set B) per le modalità DUO e MS.
#[derive(Params)]
pub struct SetParams {
    #[nested(array, group = "Tone bande")]
    pub bands: [BandParams; 4],

    #[id = "lim_in"]
    pub lim_in: FloatParam,
    #[id = "lim_thr"]
    pub lim_thr: FloatParam,
    #[id = "lim_rel"]
    pub lim_rel: FloatParam,
    #[id = "lim_auto"]
    pub lim_auto: BoolParam,
    #[id = "sc_hp"]
    pub sc_hp: BoolParam,
    #[id = "sc_bell"]
    pub sc_bell: BoolParam,
    #[id = "sc_lp"]
    pub sc_lp: BoolParam,

    #[id = "pres_db"]
    pub pres_db: FloatParam,
    #[id = "pres_f"]
    pub pres_f: FloatParam,

    #[id = "hp_f"]
    pub hp_f: FloatParam,
    #[id = "hp_s"]
    pub hp_s: EnumParam<SlopeP>,
    #[id = "lp_f"]
    pub lp_f: FloatParam,
    #[id = "lp_s"]
    pub lp_s: EnumParam<SlopeP>,

    #[id = "clip_drive"]
    pub clip_drive: FloatParam,
    #[id = "clip_ceil"]
    pub clip_ceil: FloatParam,
    #[id = "clip_model"]
    pub clip_model: EnumParam<ClipP>,
    #[id = "clip_mix"]
    pub clip_mix: FloatParam,
    #[id = "clip_post"]
    pub clip_post: BoolParam,
}

impl SetParams {
    fn new() -> Self {
        SetParams {
            bands: [
                band("LF", 80.0, 20.0, 400.0, ShapeP::Shelf),
                band("LMF", 400.0, 100.0, 2000.0, ShapeP::Bell),
                band("HMF", 2500.0, 600.0, 8000.0, ShapeP::Bell),
                band("HF", 10000.0, 3000.0, 20000.0, ShapeP::Shelf),
            ],
            lim_in: db("Limiter In", 0.0, 0.0, 24.0),
            lim_thr: db("Limiter Soglia", 0.0, -24.0, 0.0),
            lim_rel: FloatParam::new("Limiter Rilascio", 100.0, FloatRange::Skewed { min: 5.0, max: 1500.0, factor: FloatRange::skew_factor(-1.5) })
                .with_unit(" ms")
                .with_value_to_string(formatters::v2s_f32_rounded(0)),
            lim_auto: BoolParam::new("Limiter Auto", true),
            sc_hp: BoolParam::new("Sidechain HP", false),
            sc_bell: BoolParam::new("Sidechain Presenza", false),
            sc_lp: BoolParam::new("Sidechain LP", false),
            pres_db: db("Presenza", 0.0, -8.0, 8.0),
            pres_f: hz("Presenza Freq", 5000.0, 2000.0, 12000.0),
            hp_f: hz("Passa-alti", 20.0, 20.0, 800.0),
            hp_s: EnumParam::new("Passa-alti Pendenza", SlopeP::S24),
            lp_f: hz("Passa-bassi", 20000.0, 2000.0, 20000.0),
            lp_s: EnumParam::new("Passa-bassi Pendenza", SlopeP::S24),
            clip_drive: db("Clipper Drive", 0.0, 0.0, 24.0),
            clip_ceil: db("Clipper Ceiling", 0.0, -12.0, 0.0),
            clip_model: EnumParam::new("Clipper Modello", ClipP::Tanh),
            clip_mix: FloatParam::new("Clipper Mix", 1.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit(" %")
                .with_value_to_string(formatters::v2s_f32_percentage(0)),
            clip_post: BoolParam::new("Clipper Post Clip", false),
        }
    }
}

#[derive(Params)]
pub struct McsParams {
    #[persist = "editor-state"]
    pub editor_state: Arc<EguiState>,

    #[id = "in_trim"]
    pub in_trim: FloatParam,
    #[id = "in_pol_l"]
    pub in_pol_l: BoolParam,
    #[id = "in_pol_r"]
    pub in_pol_r: BoolParam,
    #[id = "in_swap"]
    pub in_swap: BoolParam,
    #[id = "in_tape"]
    pub in_tape: FloatParam,
    #[id = "in_phase"]
    pub in_phase: BoolParam,

    #[id = "tone_on"]
    pub tone_on: BoolParam,
    #[id = "tone_mode"]
    pub tone_mode: EnumParam<ModeP>,
    #[id = "lim_on"]
    pub lim_on: BoolParam,
    #[id = "lim_mode"]
    pub lim_mode: EnumParam<ModeP>,
    #[id = "flt_on"]
    pub flt_on: BoolParam,
    #[id = "flt_mode"]
    pub flt_mode: EnumParam<ModeP>,
    #[id = "eqf_on"]
    pub eqf_on: BoolParam,
    #[id = "eqf_mode"]
    pub eqf_mode: EnumParam<ModeP>,
    #[id = "clip_on"]
    pub clip_on: BoolParam,
    #[id = "clip_mode"]
    pub clip_mode: EnumParam<ModeP>,
    #[id = "os"]
    pub os: EnumParam<OsP>,

    #[id = "out_l"]
    pub out_l: FloatParam,
    #[id = "out_r"]
    pub out_r: FloatParam,
    #[id = "out_link"]
    pub out_link: BoolParam,

    #[nested(array, group = "Set")]
    pub sets: [SetParams; 2],

}

impl Default for McsParams {
    fn default() -> Self {
        McsParams {
            editor_state: EguiState::from_size(820, 640),
            in_trim: db("Input Trim", 0.0, -24.0, 24.0),
            in_pol_l: BoolParam::new("Polarità L", false),
            in_pol_r: BoolParam::new("Polarità R", false),
            in_swap: BoolParam::new("Scambia L/R", false),
            in_tape: db("Nastro", 0.0, -6.0, 6.0),
            in_phase: BoolParam::new("Fase", false),
            tone_on: BoolParam::new("Tone On", true),
            tone_mode: EnumParam::new("Tone Modo", ModeP::St),
            lim_on: BoolParam::new("Limiter On", true),
            lim_mode: EnumParam::new("Limiter Modo", ModeP::St),
            flt_on: BoolParam::new("Filter On", true),
            flt_mode: EnumParam::new("Filter Modo", ModeP::St),
            eqf_on: BoolParam::new("EQ finale On", true),
            eqf_mode: EnumParam::new("EQ finale Modo", ModeP::St),
            clip_on: BoolParam::new("Clipper On", true),
            clip_mode: EnumParam::new("Clipper Modo", ModeP::St),
            os: EnumParam::new("Oversampling", OsP::X2),
            out_l: db("Output L", 0.0, -24.0, 12.0),
            out_r: db("Output R", 0.0, -24.0, 12.0),
            out_link: BoolParam::new("Output Link", true),
            sets: [SetParams::new(), SetParams::new()],
        }
    }
}

fn slope(s: SlopeP) -> usize {
    match s {
        SlopeP::S12 => 0,
        SlopeP::S24 => 1,
        SlopeP::S48 => 2,
    }
}

impl McsParams {
    fn to_chain(&self) -> ChainParams {
        let mut p = ChainParams::default();
        p.input.trim_db = self.in_trim.value() as f64;
        p.input.pol_l = self.in_pol_l.value();
        p.input.pol_r = self.in_pol_r.value();
        p.input.swap = self.in_swap.value();
        p.input.tape_db = self.in_tape.value() as f64;
        p.input.phase = self.in_phase.value();

        p.tone = Module { on: self.tone_on.value(), mode: self.tone_mode.value().into(), sets: p.tone.sets };
        p.limiter = Module { on: self.lim_on.value(), mode: self.lim_mode.value().into(), sets: p.limiter.sets };
        p.filter = Module { on: self.flt_on.value(), mode: self.flt_mode.value().into(), sets: p.filter.sets };
        p.eqf = Module { on: self.eqf_on.value(), mode: self.eqf_mode.value().into(), sets: p.eqf.sets };
        p.clipper = Module { on: self.clip_on.value(), mode: self.clip_mode.value().into(), sets: p.clipper.sets };

        for i in 0..2 {
            let s = &self.sets[i];
            let mut bands = ToneParams::default().bands;
            for b in 0..4 {
                let bp = &s.bands[b];
                bands[b] = ToneBand {
                    on: bp.on.value(),
                    gain_db: bp.gain.value() as f64,
                    freq: bp.freq.value() as f64,
                    shape: Shape::from_index(bp.shape.value() as usize),
                    solo: bp.solo.value(),
                };
            }
            p.tone.sets[i] = ToneParams { bands };
            p.limiter.sets[i] = LimiterParams {
                in_db: s.lim_in.value() as f64,
                thresh_db: s.lim_thr.value() as f64,
                release_ms: s.lim_rel.value() as f64,
                auto_release: s.lim_auto.value(),
                sc_hp: s.sc_hp.value(),
                sc_bell: s.sc_bell.value(),
                sc_lp: s.sc_lp.value(),
            };
            p.filter.sets[i] = FilterParams { pres_db: s.pres_db.value() as f64, pres_freq: s.pres_f.value() as f64 };
            p.eqf.sets[i] = EqFinaleParams {
                hp_freq: s.hp_f.value() as f64,
                hp_slope: slope(s.hp_s.value()),
                lp_freq: s.lp_f.value() as f64,
                lp_slope: slope(s.lp_s.value()),
            };
            p.clipper.sets[i] = ClipperParams {
                drive_db: s.clip_drive.value() as f64,
                ceiling_db: s.clip_ceil.value() as f64,
                model: ClipModel::from_index(s.clip_model.value() as usize),
                mix: s.clip_mix.value() as f64,
                post_clip: s.clip_post.value(),
            };
        }
        p.output.gain_l_db = self.out_l.value() as f64;
        p.output.gain_r_db = self.out_r.value() as f64;
        p.output.link = self.out_link.value();
        p.os_log2 = self.os.value() as usize;
        p
    }
}

pub struct MultiChannelStrip {
    params: Arc<McsParams>,
    chain: Option<Chain>,
    sample_rate: f64,
    last_latency: u32,
    shared: Arc<Shared>,
}

impl Default for MultiChannelStrip {
    fn default() -> Self {
        MultiChannelStrip { params: Arc::new(McsParams::default()), chain: None, sample_rate: 48000.0, last_latency: 0, shared: Arc::new(Shared::default()) }
    }
}

impl Plugin for MultiChannelStrip {
    const NAME: &'static str = "Multi Channel Strip";
    const VENDOR: &'static str = "Multi Channel Strip";
    const URL: &'static str = "";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: NonZeroU32::new(2),
        main_output_channels: NonZeroU32::new(2),
        ..AudioIOLayout::const_default()
    }];

    const MIDI_INPUT: MidiConfig = MidiConfig::None;
    const MIDI_OUTPUT: MidiConfig = MidiConfig::None;
    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        let params = self.params.clone();
        let shared = self.shared.clone();
        let egui_state = params.editor_state.clone();
        create_egui_editor(
            self.params.editor_state.clone(),
            gui::GuiState::default(),
            |ctx, _| gui::setup(ctx),
            move |ctx, setter, state| gui::draw(ctx, setter, state, &params, &shared, &egui_state),
        )
    }

    fn initialize(&mut self, _layout: &AudioIOLayout, cfg: &BufferConfig, ctx: &mut impl InitContext<Self>) -> bool {
        self.sample_rate = cfg.sample_rate as f64;
        self.shared.inits.fetch_add(1, Relaxed);
        self.shared.sr.store(cfg.sample_rate, Relaxed);
        let chain = Chain::new(self.sample_rate);
        self.last_latency = chain.latency() as u32;
        ctx.set_latency_samples(self.last_latency);
        self.chain = Some(chain);
        true
    }

    fn reset(&mut self) {
        self.shared.resets.fetch_add(1, Relaxed);
        // azzera gli stati interni (code di riverbero dei filtri, inviluppi) senza perdere la catena
        if self.chain.is_some() {
            self.chain = Some(Chain::new(self.sample_rate));
        }
    }

    fn process(&mut self, buffer: &mut Buffer, _aux: &mut AuxiliaryBuffers, ctx: &mut impl ProcessContext<Self>) -> ProcessStatus {
        self.shared.calls.fetch_add(1, Relaxed);
        self.shared.chans.store(buffer.channels() as u32, Relaxed);
        self.shared.len.store(buffer.samples() as u32, Relaxed);
        let Some(chain) = self.chain.as_mut() else {
            self.shared.no_chain.fetch_add(1, Relaxed);
            return ProcessStatus::Normal;
        };
        let p = self.params.to_chain();
        let slices = buffer.as_slice();
        if slices.len() >= 2 {
            let (a, b) = slices.split_at_mut(1);
            chain.process(&p, &mut a[0][..], &mut b[0][..]);
        }
        // misure per la grafica
        if self.shared.reset_lufs.swap(false, Relaxed) {
            chain.analysis.reset_integrated();
        }
        let sn = chain.analysis.snapshot();
        let sh = &self.shared;
        sh.momentary.store(sn.momentary as f32, Relaxed);
        sh.short_term.store(sn.short_term as f32, Relaxed);
        sh.integrated.store(sn.integrated as f32, Relaxed);
        sh.peak_l.store(sn.peak_l as f32, Relaxed);
        sh.peak_r.store(sn.peak_r as f32, Relaxed);
        sh.rms_l.store(sn.rms_l_db as f32, Relaxed);
        sh.rms_r.store(sn.rms_r_db as f32, Relaxed);
        sh.corr.store(sn.correlation as f32, Relaxed);
        sh.gr.store(chain.limiter.gr_db().0.min(chain.limiter.gr_db().1) as f32, Relaxed);
        if let Some(mut pts) = sh.scope.try_lock() {
            pts.clear();
            let n = slices[0].len();
            let step = (n / 128).max(1);
            let mut i = 0;
            while i < n && pts.len() < 256 {
                pts.push((slices[0][i], slices[1][i]));
                i += step;
            }
        }
        let lat = chain.latency() as u32;
        if lat != self.last_latency {
            self.last_latency = lat;
            ctx.set_latency_samples(lat);
        }
        ProcessStatus::Normal
    }
}

impl Vst3Plugin for MultiChannelStrip {
    const VST3_CLASS_ID: [u8; 16] = *b"MultiChanStrip01";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::Dynamics, Vst3SubCategory::Eq];
}

nih_export_vst3!(MultiChannelStrip);
