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

**Offline sidecar is enforced by the CLI.** When `embed::store::init` resolves a
sidecar (`$MEM0_EMBED_MODEL_DIR` / `<exe_dir>/models/` / cache dir), it sets
`HF_HUB_OFFLINE=1` and clears `HF_HOME` for the process before constructing the
`TextEmbedding`. This makes the sidecar authoritative and fully offline — fastembed
neither pings the HF API to resolve the revision nor lets a stray `HF_HOME` override
`with_cache_dir`. End users need only point at the sidecar (e.g.
`export MEM0_EMBED_MODEL_DIR=...`); no `HF_HUB_OFFLINE` on their part.

**Primed behind a firewall:** if the build/prime machine cannot reach `huggingface.co`,
prime the cache via a mirror instead of step 2 — e.g. with the `hf` CLI's bundled
`huggingface_hub` (anonymous, `token=False`):
`HF_ENDPOINT=https://hf-mirror.com python -c "from huggingface_hub import snapshot_download; snapshot_download('intfloat/multilingual-e5-small', allow_patterns=['onnx/model.onnx','tokenizer.json','config.json','special_tokens_map.json','tokenizer_config.json'], cache_dir='target/release/models', token=False)"`
then proceed to step 4.

The binary still works without the sidecar (falls back to lazy download).
