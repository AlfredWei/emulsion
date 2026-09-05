# Vendored fork of `rsraw` — why this exists

**Origin**: [`rsraw` v0.1.1](https://crates.io/crates/rsraw) from crates.io, upstream repo [github.com/hexilee/rsraw](https://github.com/hexilee/rsraw), MIT-licensed. Copied here (source only — no LibRaw C++ source lives in this crate, that's `rsraw-sys`'s vendored copy) on 2026-09-05, not written from scratch. Extends this project's existing `vendor/rsraw-sys/` vendoring (see that directory's own `PATCH.md`) to the safe wrapper crate too — until now only `rsraw-sys`'s build was patched, `rsraw` itself was a normal crates.io dependency.

**Why vendored**: HDR merge (RFC-0003) needs a genuinely linear-light, non-auto-brightened decode from LibRaw — the standard `gamm=[1,1]` / `no_auto_bright=1` / `output_bps=16` recipe every RAW-based HDR tool uses. Those fields live on `libraw_output_params_t`, reachable only through `RawImage`'s private `raw_data: *mut sys::libraw_data_t` pointer — the existing `set_use_camera_wb`/`set_use_camera_matrix` methods (`src/raw.rs`) are the *only* way anything outside this crate can touch that struct, and neither of them (nor anything else upstream exposes) can set `gamm`/`no_auto_bright`. Forking was the only option short of reimplementing the whole safe wrapper from `rsraw-sys` directly in `app/src-tauri`, which would have thrown away everything else this crate already provides (EXIF/GPS/lens metadata extraction, thumbnail extraction, the existing 8-bit decode path every other part of this app already depends on).

## What's actually changed vs. upstream

One new method on `RawImage` (`src/raw.rs`), added alongside the existing `set_use_camera_wb`/`set_use_camera_matrix` (same unsafe-pointer-write pattern, same file, same struct):

```rust
pub fn set_linear_output(&mut self) {
    unsafe {
        (*self.raw_data).params.gamm = [1.0, 1.0, 0.0, 0.0, 0.0, 0.0];
        (*self.raw_data).params.no_auto_bright = 1;
    }
}
```

Nothing else in the crate is modified — every existing consumer (`raw_decode.rs`'s `decode_preview`/`decode_develop_preview`, `metadata.rs`'s EXIF extraction) is byte-for-byte unaffected; this method is additive and only called from the new `raw_decode::decode_linear` (HDR merge's own decode path).

## How this is wired into the app

`app/src-tauri/Cargo.toml`'s existing `[patch.crates-io]` section (previously only `rsraw-sys`) gains a second entry: `rsraw = { path = "vendor/rsraw" }`. This vendored `rsraw`'s own `Cargo.toml` declares a plain `rsraw-sys = "0.1"` version dependency (not a relative path to the sibling vendor directory) — Cargo's patch resolution redirects it to `vendor/rsraw-sys` automatically, exactly the same mechanism that already redirects the top-level app's own `rsraw-sys` dependency, so the two vendored crates don't need to know about each other's location.

## Status

Verified fields exist and have the expected names/types against this repo's own vendored LibRaw header (`vendor/rsraw-sys/LibRaw/libraw/libraw_types.h`) before writing this fork — see [RFC-0003](../../../../docs/rfc/RFC-0003-hdr-merge.md) §3.1 for the full verification. See [ADR-0003](../../../../docs/adr/ADR-0003-raw-decoding.md)'s dated update for build/CI confirmation status.
