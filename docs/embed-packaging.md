# Release packaging for the `embed` feature

`cargo build --features embed --release` produces the binary. To make the common path
offline, fetch the default model into `<release_dir>/models/multilingual-e5-small/`:

1. Build: `cargo build --features embed --release`
2. Prime the cache once: run `./target/release/mem0 embed "warmup"` (downloads into the
   fastembed cache as a subdir named **`models--intfloat--multilingual-e5-small`** —
   spike-verified; fastembed's `model_code` for `MultilingualE5Small` is
   `intfloat/multilingual-e5-small`, not `Qdrant/…`). Five files land in
   `blobs/` (+ symlinks under `snapshots/<rev>/`): `onnx/model.onnx` (~470 MB),
   `tokenizer.json`, `config.json`, `special_tokens_map.json`, `tokenizer_config.json`.
3. Ship that cache subdir beside the binary, **preserving its name**:
   `mkdir -p target/release/models && cp -R .fastembed_cache/models--intfloat--multilingual-e5-small target/release/models/`
   (or from wherever `FASTEMBED_CACHE_DIR`/`HF_HOME` pointed). This works because
   `<exe_dir>/models/` is a `SearchRoot` and `resolve()` looks for
   `models--intfloat--multilingual-e5-small`.
4. Archive `target/release/{mem0, models/}` together.

**`HF_HOME` caveat:** fastembed's `pull_from_hf` prefers `HF_HOME` over the
`with_cache_dir(...)` value, so if the end-user has `HF_HOME` set, the shipped
sidecar under `<exe_dir>/models/` is bypassed (fastembed will look in `$HF_HOME`
instead). This is harmless — the binary still works (lazy download fallback) —
but document it. For a guaranteed-offline sidecar, advise users to leave
`HF_HOME` unset, or have the CLI unset `HF_HOME` for the embed path (out of
scope for v1.3). `HF_HUB_OFFLINE=1` + a fully-primed cache loads with no network.

The binary still works without the sidecar (falls back to lazy download).
