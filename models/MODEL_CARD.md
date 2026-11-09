# SMaLL-100 INT8 for Syousetu Translate

This is the application's prepared Q8_0 INT8 GGUF, directly usable by its Rust
CPU inference engine. Portable releases include it; optional recovery downloads
the same file. No local conversion, Python,
transformers, bitsandbytes, CUDA or original FP32 weight file is required.

- Source: https://huggingface.co/alirezamsh/small100
- Upstream revision: `8ab680e26a596d2e3d2d2d17ae0f68df1037328c`
- Upstream FP32 SHA-256: `dd3b845a36ea4ed90437fd0b9b477e30c21f144d3658679fd5c945e3c96b0fbc`
- INT8 bytes: `354200608`
- INT8 SHA-256: `536bff54568334a3baf800d02984c53f0649af620309577f431877bde350ec9c`
- License: MIT; original copyright/license included in `LICENSE-MIT-SMaLL100.txt`.

The maintainer exporter is Rust/Candle, built with `--features model-tools`.
It packs self-attention Q/K/V and cross-attention K/V weights along complete
rows before deterministic Q8_0 quantization. Bias/normalization tensors remain
floating point. Generation uses the trained SMaLL-100 architecture with greedy
decoding and direct Japanese-to-Korean language selection. This format is not
a bitsandbytes checkpoint and is not intended to load in Python Transformers.

The portable release includes the tokenizer/configuration files from the pinned
upstream revision. Recovery downloads use that same revision. The complete model
is 360,332,983 bytes rather than 1,337,106,147 bytes; existing verified files are reused.

Small multilingual models can mistranslate names, idioms and fiction. This model
does not reproduce ezTrans output. User corrections use a separate local memory;
they are not embedded in, uploaded with, or trained into this model release.
