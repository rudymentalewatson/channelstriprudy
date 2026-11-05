# Multi Channel Strip

Channel strip di mastering: Input → Tone (4 bande, 5 forme) → Limiter → Filter → EQ finale → Clipper → Output, più pannello Analysis (LUFS, picco/RMS, correlazione, goniometro).

## Come ottenere il VST3 (Windows)

1. Crea un repository su GitHub e carica il contenuto di questa cartella (così com'è).
2. Apri la scheda **Actions** → **Build VST3 (Windows)** → **Run workflow**.
3. A fine lavoro scarica l'artefatto **MultiChannelStrip-VST3**: dentro c'è la cartella `Multi Channel Strip.vst3`.
4. Copiala in `C:\Program Files\Common Files\VST3\` e fai riscansionare i plugin dal DAW.

In alternativa, con Rust installato: `cd mcs-plugin` e `cargo xtask bundle multi_channel_strip --release`.

## Struttura
- `mcs-dsp`: motore audio, senza dipendenze, con i test (`cargo test --release`).
- `mcs-plugin`: collegamento a nih-plug e interfaccia grafica (egui).
