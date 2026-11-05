//! Multi Channel Strip: nucleo DSP, senza dipendenze esterne.

pub mod biquad;
pub mod chain;
pub mod clipper;
pub mod filters;
pub mod input_output;
pub mod limiter;
pub mod loudness;
pub mod oversample;
pub mod tone;
pub mod util;

pub use chain::{Chain, ChainParams};
pub use util::{Mode, Module};
