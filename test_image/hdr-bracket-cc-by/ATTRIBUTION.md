# Attribution — HDR merge test bracket

Every other file under `test_image/` is CC0/public domain. These three are the
one deliberate exception, approved 2026-09-06 specifically because a genuine
CC0 RAW exposure bracket could not be found (raw.pixls.us explicitly refuses
bracket series by policy; every real bracket found on Wikimedia Commons/Flickr
was JPEG-only, which `merge_hdr_bracket` rejects since HDR merge needs a
linear RAW decode — see RFC-0003).

## Source

3 of the 5 frames (`gt1`, `gt3`, `gt5` — the darkest, middle, and brightest of
a 5-shot AEB bracket) from the *non-ghosted, motion-free* `ground_truth/raw`
subset of `complex/image_set1` in:

> Karaduzovic-Hadziabdic, K.; Hasic Telalovic, J.; Mantiuk, R. K. (2017).
> *Multi-exposure image stacks for testing HDR deghosting methods* [Dataset].
> Apollo - University of Cambridge Repository.
> https://doi.org/10.17863/CAM.6881

Extracted directly from `exposure_stacks_part1.zip` via targeted HTTP Range
requests against the archive's own central directory — only these 3 files
(~27MB total) were downloaded, not the full 1.22GB archive.

The "ground truth" (not "ghosted") subset was chosen deliberately: it has no
intentional subject motion between frames, matching this app's own HDR merge
scope (RFC-0003 §2 explicitly does not implement ghost/moving-object removal).

## License

**Creative Commons Attribution 4.0 International (CC BY 4.0)**
https://creativecommons.org/licenses/by/4.0/

Unlike this repo's other CC0 test images, reuse of these 3 files requires
attribution to the authors above.

## Verification

Confirmed working end to end via this repo's own real-bracket integration
test:

```
cd app/src-tauri
EMULSION_TEST_HDR_BRACKET_DIR=../../test_image/hdr-bracket-cc-by \
  cargo test --lib hdr_merge::tests::merges_a_real_bracket_and_catalogs_the_result_with_provenance -- --nocapture
```
