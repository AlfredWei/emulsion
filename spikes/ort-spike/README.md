# ort-spike — M6 slice 0 throwaway

Does ONNX Runtime (`ort` crate, pinned `=2.0.0-rc.13`) link and run on the project's CI platforms, and how fast is it? See [RFC-0023](../../docs/rfc/RFC-0023-on-device-ai-inference.md) and [ADR-0009](../../docs/adr/ADR-0009-ml-inference-runtime.md).

**Not part of the app and not built by the normal CI.** Standalone crate (its own `[workspace]`), run by `.github/workflows/ort-spike.yml` on `workflow_dispatch` or pushes to the `spike/ort-windows` branch. The weights are fetched at run time and never committed.

```
cargo build --release
target/release/ort-spike <model.onnx> <cpu|coreml|directml> <name=1x3x320x320> [<name=shape> ...]
```

Prints one `ORTSPIKE {...}` JSON line per run (also on failure).
