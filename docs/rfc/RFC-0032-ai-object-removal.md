# RFC-0032: Remove objects: AI inpainting for spot removal (M7, the "generative fill" decision)

- Status: Engine, licence and bundling decided 2026-11-02 in [ADR-0010](../adr/ADR-0010-object-removal-engine.md) (slice B done); slice A results in §9; slices D to F not started
- Date: 2026-10-26
- Relates to: [MILESTONES.md](../../PRD/MILESTONES.md) M7 (the decision this RFC must make before any implementation), [RFC-0023](RFC-0023-on-device-ai-inference.md) §5.6 (generative removal "not surveyed"; M7 owns it), [RFC-0031](RFC-0031-intelligent-culling.md) §5 slice F (the user's decision: generative fill, framed as spot removal, after culling), [RFC-0026](RFC-0026-click-select-segmentation.md) (click-to-select an object: the natural way to make the mask), [RFC-0027](RFC-0027-ai-denoise.md) / [RFC-0029](RFC-0029-ai-super-resolution.md) (the `emulsion-ai` helper, the model manager and consent flow, a cached result beside the edit stack), [RFC-0025](RFC-0025-composable-masking.md), [RFC-0020](RFC-0020-grain-particle-noise.md) (grain, for matching a patch to its surroundings), [ADR-0009](../adr/ADR-0009-ml-inference-runtime.md)

## 0. Process note

Design, desk research, and (added 2026-10-26) the slice A bake-off of §5 and §9. The two ONNX files were downloaded **on the user's go-ahead** into the user's `mask_training/inpaint/models/` (a sibling repo, gitignored for `*.onnx`): `migan_pipeline_v2.onnx` (28,079,181 bytes, SHA-256 `6f1f3530…8c40b`) from `huggingface.co/andraniksargsyan/migan`, and `lama_fp32.onnx` (208,044,816 bytes, SHA-256 `1faef530…eb68d6`) from `huggingface.co/Carve/LaMa-ONNX`. The test photographs (20, in the user's five categories) came from Wikimedia Commons, **CC0 / public domain / CC BY only** (never SA, NC, ND), listed with licence and author in `mask_training/inpaint/photos/meta.json`. The scripts that produced every number are in `mask_training/inpaint/` (`fetch_photos.py`, `prepare_cases.py`, `run_models.py`, `score.py`); the only code added to the app repo is the PatchMatch module and its ignored probe. **Nothing is in the app yet.** Numbers are measured on one Apple M1 Pro; Windows is not measured.

User decision recorded 2026-10-20 (RFC-0031 §5): *generative fill is great for spot removal, so it comes after culling.* This RFC reads that as: **remove an unwanted object or blemish and fill it plausibly**, not "invent new content from a text prompt".

## 1. Problem

The app has a **clone / heal** tool (the `Spot` mask: circular dabs, a source offset the user chooses, an optional heal that matches colour). It is right for dust spots and small blemishes, where one good source patch is nearby. It is the wrong tool when:

1. the object is **big or irregular** (a person at the edge of a beach shot, a bin, a cable across a sky): there is no one source patch, and cloning leaves repeated texture;
2. the background is **structured** (a fence, a building edge) and has to continue behind the object;
3. the user wants it in **one gesture**: click the thing, press Remove.

MILESTONES.md M7 says the decision between *a bundled on-device generative model* and *a non-generative content-aware fill* must be made and written down **before** implementation, because it changes the milestone's shape. This RFC makes the proposal.

## 2. Candidates (what is known, and from where)

"Generative fill" in the Photoshop sense is two different things, and only one is about removing:

- **Inpainting for removal**: fill the hole so it looks like the surroundings. No prompt. Feed-forward networks do this in about a second.
- **Prompted generation** ("put a lake here"): needs a diffusion model and a text encoder. Out of scope (§4).

| Candidate | What | Licence as read | Size (as read) | Notes |
|---|---|---|---|---|
| **PatchMatch-style content-aware fill** | Classic non-model fill: copy best-matching patches from the same photo | none (we would write it, in Rust) | no download | The "non-generative" branch of MILESTONES. Good on textures and small to medium holes, poor on structure and large holes. Not measured here. |
| **MI-GAN** (Picsart, ICCV 2023) | A small GAN inpainter built for phones; ONNX pipeline does crop-around-mask, resize to 512, inpaint and blend itself | Code: MIT ([GitHub](https://github.com/Picsart-AI-Research/MI-GAN) lists MIT; the repo has a separate `LICENSE-WEIGHTS` file whose text, as read, is MIT-style and names only the "Software"). The ONNX re-host's card says MIT ([HF](https://huggingface.co/andraniksargsyan/migan)). | `migan_pipeline_v2.onnx` **28.1 MB** ([HF file list](https://huggingface.co/andraniksargsyan/migan/tree/main)) | Trained on **Places2** (512) or FFHQ (256, faces only: not considered). Self-reported 512 px latency about 0.54 s on an iPhone 14 Pro Max ([paper supplement](https://openaccess.thecvf.com/content/ICCV2023/supplemental/Sargsyan_MI-GAN_A_Simple_ICCV_2023_supplemental.pdf)). Inputs: uint8 RGB image and a binary uint8 mask. Network runs at a fixed 512. |
| **LaMa** (Samsung, "big-lama") | Fourier-convolution inpainter, strong on large masks and repeating structure | Code Apache-2.0; the ONNX port ([Carve/LaMa-ONNX](https://huggingface.co/Carve/LaMa-ONNX)) says Apache-2.0 | `lama_fp32.onnx` **about 208 MB** (a mirror lists 208,044,816 bytes) | **Fixed 512 x 512** in the ONNX; a custom FFT block, so likely CPU only on ORT (unchecked). Another port notes Places2 as **CC-BY 4.0**; that is a third party's claim, not verified. |
| ZITS++ | Structure-aware, heavier | not checked | not checked | Leads some authors' benchmarks ([paper](https://arxiv.org/pdf/2210.05950)); named, not pursued. |
| MAT, Co-Mod-GAN | Large-hole GANs | not checked (MAT's code licence is recalled as non-commercial, unverified) | not checked | Named; excluded until a licence is read. |
| Stable Diffusion inpainting | Diffusion, can follow a prompt | OpenRAIL-family (recalled, **unverified**) | about 2 GB (recalled, **unverified**) | Heavy, GPU-hungry, slow. The ZITS++ paper reports an SD baseline failing above 1K on 48 GB. Out of scope (§4). |
| FLUX.1 Fill [dev] | Diffusion | **FLUX.1-dev non-commercial** (per a [package listing](https://pypi.org/project/fluxbfl/)) | **23.8 GB** weights | Excluded on licence and size. |

**What the research did not give** (slice A measured it, §9): peak memory and CPU time of MI-GAN and LaMa on a desktop, whether either runs on ORT's CoreML and DirectML providers, how they look on **full-resolution** photographs (both work at 512 px; our photos are 24 to 60 MP), and any comparison against a plain PatchMatch fill. Published quality numbers are the authors' own, on Places2 and similar, at 512 px.

## 3. What this RFC proposes

### 3.1 The decision (after the bake-off, §9)

**Do not ship a diffusion / prompted generative model in M7** (user: the complex part moves to a later slice, §4). Ship **removal by inpainting**, with **LaMa as the engine**, **subject to the licence decision in §9.4** (both models were trained on Places2, whose terms were not obtained). The bake-off's rule was that a model ships only if it is clearly better than a non-model fill; **the non-model baseline written for it (PatchMatch) lost by a wide margin on everything above a small spot**, so the "ship PatchMatch alone" fallback is **not** viable as it stands (§9.3 says why that is partly the baseline's fault). What the evidence supports:

- **LaMa** for removal: closest to the original in 46 of 60 cases, the only engine that **continues structure** (window rows, a facade grid, a ceiling arch) and the one that **did not invent objects** in the sheets looked at. Costs: 208 MB, 2.3 s per removal on this machine's CPU (CoreML is slower, §9.2), fixed 512 px working size.
- **MI-GAN is out for anything above a small spot**: fast (0.3 s) and sharp on small holes, but on medium and large holes it **invented objects** (a white blob in a mesh, a green figure in a gap, cables in a window wall). A removal tool that adds things is worse than none.
- **Large holes are poor for every engine** (smeared texture, lost structure): the tool needs a **large-region guard** (§3.4) rather than a promise.
- The existing **clone / heal stays**, untouched, as the manual tool, and as the answer when the model's result is wrong ("Use Heal instead").

### 3.2 It is an edit, not a new photo

Unlike Enlarge (RFC-0029), removal **keeps the pixel grid**, so it belongs in the edit stack: one new op kind, `remove` (working name), per removal:

- the **mask** (normalised, resolution-independent: brush strokes or a polygon; a click-select result from RFC-0026 is converted to a polygon, then grown by a few pixels because inpainters need the halo and shadow covered),
- the engine and its version (`patchmatch-1`, `migan-512-places2`, ...),
- a **patch id** pointing to the stored result: a PNG of the region (mask box plus context) with a feathered alpha, written once.

The pipeline applies patches **first**, to the source pixels, before tone, colour and the other masks (as the Spot mask reads pre-mask pixels today; and as denoise's cached stage sits ahead of tone, RFC-0027). So every other edit sees the removed picture.

- **Non-destructive and undoable** through the existing history, like any op.
- **Per-photo**: excluded from Copy Settings and Auto Sync the way crop and spot removal already are (RFC-0028 §3.5), because a mask is only meaningful on its own photo.
- **The patch is a stored blob in the catalog database, not a cache entry and not a file (ADR-0010 §4: the backup is the SQLite file alone; still provisional: the user asked for a small study of this before it is decided, slice C2; §10 measured that the reason given here is much weaker than assumed for LaMa).** A GAN's output can differ between CPU and CoreML, so regenerating it later could change a picture the user already accepted. It lives in the catalog database (so every backup and catalog copy carries it) and is deleted with its op. If the blob is missing the op shows a "needs regenerating" state rather than silently re-running.
- **Not exportable to XMP** (no standard to carry it); the existing sidecar story for AI results applies.
- CPU interpreter and GPU path both just **composite a stored image over a region**; no model runs in the render path.

### 3.3 The job

- Input is the **unedited full-resolution decode**, cropped around the mask with context.
- **Context rule (to be measured):** if the mask's box plus a margin fits in 512 px at native resolution, run at native resolution (no loss); otherwise reduce to 512, inpaint, and put the original's high-frequency detail and grain back (RFC-0020's generator can match the surrounding grain), so a large removal on a 24 MP photo is not a soft smudge. How well that works is the main open quality question.
- A new helper op `inpaint_run` next to `denoise_run` and `upscale_run`, on the same reader thread, with `progress` and `cancel`; one job at a time; Select Subject waits.
- Memory is bounded by the crop (512 px network input), not by the photo: much smaller than denoise or enlargement. Not measured.

### 3.4 UX (designed first as an interactive mock)

The RFC-0028 / 0029 / 0031 way: mock before code. The mock is [remove-tool-mockup.html](../ux/mockups/remove-tool-mockup.html) (a synthetic beach photo: dust spots, a wire, a person with a shadow, a bin, a small crowd). Paint, Click object (three candidates), Lasso, the result with Accept / Undo / Use Heal instead / hold-for-Before, the "shadow is still there" and "only part of the wire" offers, the large-region guard, the model consent step, and two selectors for the open questions (bundle vs download; stored vs re-run patch, with *Close and reopen photo* and *Patch file missing*) all work. The shape it carries:

- A **Remove** tool in Develop's tool strip beside Heal / Clone. **Three ways to mark:** paint, click an object (Select Subject's click, with the three candidates), or lasso.
- The result appears **in place in about a second or two**, with **Accept** / **Undo**, and a **Before / After** that holds the key. **Use Heal instead** is one click on the same mask, so the old tool is never taken away.
- A few spots can be marked in one go; each is its own op in the history list.
- **No prompt box, no "generative" wording.** Plain "Remove". The model manager says what is downloaded and why, as for the other AI tools.
- A **large-region guard**: above a size limit the tool says the result may be poor and offers to proceed.
- Honest limits shown in the mock's copy: it invents plausible background; it can repeat texture; faces and text in the filled area are not reliable.

## 4. Not in this RFC

- **Prompted generation / text-to-image fill / outpainting** (diffusion, GBs, GPU). **The user's decision (2026-10-26): removal only now; the complex part moves to a later slice (slice G).** Revisit after removal ships; the PRD's non-goals already put Firefly-style features out of scope unless revisited.
- **Sky replacement, object replacement, face or body generation.**
- **Cloud inference** (PRD: no cloud).
- **Batch removal across photos** and **removal in Library**: Develop only.
- **Video.**

## 5. How the bake-off judges (slice A)

Cases (about 20 photographs, mine and the user's): a dust spot on sky; a wire across a sky; a small object on grass or sand; a person at the edge of a beach shot; an object against a fence or building edge; a large hole (a quarter of the frame, expected to be bad); skin blemish; a hole across a gradient; a hole over text (expected to fail).

Measured: **time and peak memory** on this Mac (CPU, and CoreML where the model loads) and on the Windows CI runner; **ground-truth error** by masking a region of a real photo and comparing the fill with what was there (PSNR / SSIM on the masked area, as a sanity signal only); and **looked at by eye at 100 % on a full-resolution photo**, which is the judgement that matters and the one RFC-0023 §4.7 learned not to skip. The user rules on the by-eye table.

## 6. Slice plan

| Slice | What | Status |
|---|---|---|
| **A** | Bake-off: PatchMatch, MI-GAN, LaMa on 60 hole cases over 20 photographs; times, memory, providers | **Done (§9)**; the by-eye table awaits the user's ruling |
| **B** | Licence step and ADR: Places2 terms (§9.4), LaMa weights file, bundle vs download of 208 MB, where patches live | **Done 2026-11-02: [ADR-0010](../adr/ADR-0010-object-removal-engine.md)** (LaMa accepted, bundled, patches in the catalog) |
| **C** | UX mock of the Remove tool (§3.4): [remove-tool-mockup.html](../ux/mockups/remove-tool-mockup.html) | **Done** (awaits the user's use of it) |
| **C2** | **Small study: stored patch vs regenerate on demand** (§3.2, the user's question 3). **Measured part done (§10); the study with people is ready to run ([protocol](RFC-0032-appendix/c2-patch-study.md)).** Run with the mock and a prototype: 5 to 8 photographers (the user first), tasks "remove, edit further, close, come back / open on the other machine / copy the catalog", compare *a fixed stored result* with *a result that re-runs and may differ*: measure surprise, trust, and what they would pick. Decides §3.2 before slice D builds it | After C |
| **D** | (**D1 built 2026-11-02**: the helper op `inpaint_run`, §11; **D2a built**: the patch table and the bake, §12; **D2d built**: the patches reach the screen, thumbnail and export, §12.5; design in §12: **no GPU compositing, the patch is baked into the source the canvas loads**) Engine and job: `inpaint_run` helper op (LaMa), crop / blend, **detail transfer** for holes the 512 px network sees reduced, the `remove` op with stored patches (a catalog table, ADR-0010 §4), the baked source, the bundled-models entry | Next (B is done; C2's study with people not run, ADR-0010 "Not settled") |
| **E** | (**E1 built 2026-11-04**: Remove tool with brush and lasso, run, undo last, large-area guard; §13) Develop UI: the Remove tool, paint / click / lasso, accept / undo, Heal fallback, large-region guard, history entries, model manager entry | After D |
| **F** | Quality work the bake-off shows is needed: detail and grain matching, halo growth, better PatchMatch as a no-download fallback if wanted | After E |
| **G** | **Later, optional: the complex part** (prompted fill, diffusion), only if the user wants it after living with removal | Not scheduled |

## 7. Not verified

- **Windows**, in every respect (LaMa's FFT block on ORT's CPU provider and DirectML, timing, memory).
- **Real removal cases.** The holes are random blobs with known ground truth; real removals have an object with a shadow and halo, a mask drawn by a person or by click-select, and a purpose. The user's own photographs were not used (the user asked for web photographs).
- **Whether detail transfer fixes the soft medium and large fills** (§9.3): not tried.
- **A competent non-model fill.** The PatchMatch here is a first implementation (§9.3); it says little about what a good one can do.
- **Licences** (§9.4): Places2's terms, and the weights files' own notices.
- **CoreML**: the provider run covered only four cases and the process then ended without a result or an error message (not understood); what it did measure was far slower than CPU.
- **Quality on faces, text, and very textured sky or foliage** beyond what the sheets show.
- The edit-stack and GPU-path design (§3.2) against the real pipeline code: read from comments only.

## 8. Decisions and open questions

Answered by the user on 2026-10-26:

1. **Download for the bake-off:** yes, into the `mask_training` folder (done, §0).
2. **Scope:** removal only; **the complex part moves to a later slice** (§4, slice G).
3. **Stored patch:** arrange **a small study** to make sure users get the best experience (slice C2) before deciding.
4. **If no model beats the non-model fill, ship the non-model fill alone:** yes (a model did beat it, so this did not trigger; see §3.1).
5. **Test photographs:** find them on the web: people, portrait, landscape, architecture, building (done: 20 Commons photographs, §0).

Answered by the user on 2026-11-02 (recorded in [ADR-0010](../adr/ADR-0010-object-removal-engine.md)): **A and B: accept** (LaMa is the engine, MI-GAN out above small holes, a guard for large holes; Places2-trained weights accepted, its terms still unread); **C: bundle it** (installer about 208 MB larger). The three items below are kept as the questions were asked.

Was open, for the user:

- **A. The by-eye ruling (§9.3):** do you agree LaMa is the engine, MI-GAN is out above small holes, and large holes get a guard? The contact sheets are in `mask_training/inpaint/out/sheets/`.
- **B. The licence question (§9.4):** is a model trained on Places2 acceptable for this product? (The same kind of question as the sky model's CC BY training data in `mask_training/README.md`.)
- **C. Bundle or download:** LaMa is 208 MB. The other models are bundled in the installer; this one would make the installer about 208 MB larger. Bundle, or download on first use with the existing consent flow?

## 9. Bake-off results (slice A, 2026-10-26)

### 9.1 What was run

20 photographs (4 each: people, portrait, landscape, architecture, building; 2,606 to 3,840 px wide; some are historic scans), **3 holes each: about 150, 500 and 1,100 px across in native photo pixels** (S, M, L: dust spot / small object, a person at a distance, a large object), irregular blobs at random places, **60 cases**. Each case is a square crop around the hole with context (512 px for S, about 1,100 px for M, up to 2,420 px for L) with the original as ground truth. **MI-GAN** takes the crop at native size (its pipeline reduces to 512 itself); **LaMa** takes the crop reduced to 512, and only the hole of its result is pasted back; **PatchMatch** (the module in `app/src-tauri/src/inpaint_patchmatch.rs`, §9.3) works on the crop reduced to at most 1,536 px.

### 9.2 Time and memory (Apple M1 Pro, ORT 1.30, CPU provider)

| | MI-GAN-512 | LaMa fp32 | PatchMatch (ours) |
|---|---|---|---|
| File | 28.1 MB | 208 MB | none |
| Time per removal, median (range), 60 cases | **0.29 s** (0.23 to 0.63) | **2.30 s** (2.19 to 3.21) | 1.20 s median, 3.7 s max (8 cases run at once, so each is slower than alone) |
| Peak memory, one process | about 880 MB (1,100 px crop) | about 750 MB (fixed 512) | not measured |
| CoreML provider | 2.2 to 4.3 s on the 4 cases that ran (**7 to 15 times slower than CPU**) | 8.6 to 10.9 s (**about 4 times slower**) | n/a |

ORT's CoreML provider split the graphs into hundreds of partitions (585 for LaMa), the same pathology RFC-0023 found for SCUNet: **use the CPU provider**. Peak memory is of a Python process with the model loaded and two runs of random input.

### 9.3 Quality

Scored inside the hole against the original. **PSNR is only a sanity signal** (a blur scores well); `detail` is the high-frequency energy of the fill relative to the original's (1.0 = as sharp; far below 1 = a smudge).

| Hole | n | PatchMatch PSNR / detail | MI-GAN PSNR / detail | LaMa PSNR / detail |
|---|---|---|---|---|
| S (~150 px) | 20 | 21.8 dB / 0.51 | 21.6 dB / **0.98** | **23.2 dB** / 0.89 |
| M (~500 px) | 20 | 17.0 dB / 0.40 | 16.2 dB / 0.43 | **18.1 dB** / 0.29 |
| L (~1,100 px) | 20 | 14.3 dB / 0.14 | 13.6 dB / 0.31 | **16.0 dB** / 0.10 |
| all | 60 | 17.7 / 0.35 | 17.2 / 0.53 | **19.1** / 0.29 |

Closest to the original: **LaMa in 46 of 60 cases**, PatchMatch 13, MI-GAN 1. LaMa leads in every category (people, portrait, landscape, architecture, building).

**By eye** (five of the twenty contact sheets looked at, all three sizes each: landscape 1, building 1, people 3, architecture 1, portrait 2; the sheets are in `mask_training/inpaint/out/sheets/`):

- **Small holes: LaMa's fills are hard to find** (snow, a girder, a ceiling arch, a shirt). MI-GAN's are good on plain areas but leave faint smears on mouldings and fabric; PatchMatch leaves a flat patch on busy texture (foliage, mouldings, a shirt).
- **Medium holes: LaMa continues the structure** (it redrew the rows of windows under a cable, the facade grid, the ceiling frieze), softly. **MI-GAN invents objects** (a white blob in a metal mesh, tangled cables and a balcony in a window wall). PatchMatch fills with a flat colour patch.
- **Large holes: every engine fails in its own way**: LaMa gives smeared mush with the right colours and rough structure, MI-GAN builds plausible but *wrong* scenery (a green figure in a gap, a small building where a street was), PatchMatch a grey cloud. A **guard is required**.
- **On a 24 MP photograph the medium fill is soft**: LaMa sees the crop reduced to about 0.47 of native, and its `detail` is 0.29. This is the main quality problem to solve (slice D's detail transfer).

**The PatchMatch baseline is weak, and partly through my implementation**: it is a first version (Wexler-style, one patch size, a centre-copy last pass), not tuned, and a competent content-aware fill would do better on medium holes. What it does show is the ceiling for "no model": on structured scenes it cannot continue lines, which is what a user removing an object from a building or a street needs.

### 9.4 Licences (what was read)

- **LaMa**: code Apache-2.0 (the ONNX port's card, and the original's `big-lama` card on Hugging Face, both say Apache-2.0; the repo's `LICENSE` text was not retrievable here). **Trained on Places2.**
- **MI-GAN**: code MIT; the weights' file `LICENSE-WEIGHTS` has MIT wording that speaks only of "Software". **Trained on Places2.**
- **Places2**: the terms page could not be reached (connection refused), so **its terms were not read**. From memory (not verified) it is distributed for non-commercial research and education, and its images are third parties' photographs. **This is the open question for the user (§8 B)**: it applies to both models, and to any weights trained on that data. It is the same kind of question `mask_training/README.md` leaves to the product owner for CC BY training images.

## 10. Slice C2, the measured part: how much does a regenerated result change? (2026-10-26)

The argument for a **stored** patch (§3.2) was that regenerating a removal might change a picture the user already accepted. That is a measurable claim, so it was measured before asking anyone (script `mask_training/inpaint/determinism.py`; LaMa fp32, ORT 1.30 on the M1 Pro; 12 cases over four photographs, hole pixels only, on the 0..255 scale).

| Same input, run again | Max difference in the hole | Meaning |
|---|---|---|
| Same session, second run | **0.0** on all 12 | bit-identical |
| New session (model reloaded) | **0.0** on all 12 | bit-identical |
| 1 thread vs 8 threads | **at most 0.002** | rounding noise, below one level |
| **CPU vs CoreML** (3 cases) | **12.7 / 21.9 / 28.7**; mean 0.45 / 0.93 / 1.75; 3 % / 12 % / 31 % of hole pixels differ by more than 2 levels | a different texture grain; same content |

[The CPU / CoreML pair for the large case](RFC-0032-appendix/c2_cpu_vs_coreml.png) (left CPU, middle CoreML, right the difference ×8): the structure, the colours and the arches are the same; the difference is fine texture.

What this changes:

- **LaMa is a feed-forward network with no random draw**, so on the same machine and provider a regenerated removal is **identical**. "Regenerate may change what you accepted" does not hold within one machine; it holds across **providers** (a GPU path, DirectML, CoreML), **ORT or model versions**, and **machines of different kinds** (a catalog copied between a Mac and a Windows PC). Across providers the change is texture, not content.
- **Regeneration is not free**: 2.3 s per removal on CPU (§9.2), so re-running every removal when a photo opens (variant B in the mock) would make a photo with five removals wait about 12 s. It would have to be cached anyway, so variant B is a **cache with a re-run on a miss**, not "no stored file".
- **A diffusion fill (slice G) is different**: it samples, so a re-run gives a different picture. A stored patch is then mandatory. Choosing "stored" now keeps one design for both.

**Revised hypothesis for the study (the design the numbers favour, to be confirmed by people):** keep the patch as a stored file next to the catalog (**the saved result is the truth**, nothing re-runs on open), but treat it as **recomputable**: if the file is missing (catalog copied without its patches, a restore) the entry shows "needs regenerating" with **one button that re-runs it from the stored mask**, and says so if the result is not guaranteed identical (a different provider, a newer model). Never regenerate silently. This costs a few hundred kilobytes per removal and removes the main failure of A (a lost file is a lost edit) without B's waiting.

**What the measurements do not settle** (so the study with people stays): whether photographers *want* the accepted result frozen even when regenerating would be identical; whether a texture-level change after a catalog move is noticed or minded; whether "needs regenerating" is understood; and which of A, B and the hybrid they would pick. The protocol is [RFC-0032-appendix/c2-patch-study.md](RFC-0032-appendix/c2-patch-study.md).

**Not verified:** Windows (DirectML or CPU on another CPU family may differ from the Mac CPU by more than 0.002; not measured); CoreML on more than three cases (the earlier provider run ended silently, §7); a GPU path that does not exist yet.

## 11. Slice D1: the helper op (2026-11-02)

`inpaint_run` in the `emulsion-ai` helper (`bin/emulsion-ai/inpaint.rs`; the pure part is `ai/inpaint_pipeline.rs`, shared with the app side by `#[path]` like the upscaler's): input is the photograph at `image_path`, an 8-bit grayscale PNG `mask_path` of the **same size** (filled = 128 or more), the model at `model`; output is an RGBA PNG **patch** at `out_path` and `InpaintResult { x, y, width, height, window, reduced, ms }`. It streams four `progress` steps and can be cancelled between them (a single network pass cannot be interrupted, about 2 to 6 s on the CPU).

- **The window** is the bake-off's rule (§9.1) made exact: a square centred on the mask's bounding box, **2.2 times its longer side but never under 512 px**, kept inside the photograph (shifted at an edge, capped at its shorter side). Up to a hole of about 230 px it is the network's own 512 window **at native resolution**; beyond that it is reduced to 512 and the answer enlarged back (`reduced: true`: the fill is softer than the photograph around it; **detail transfer is not built**, so this is slice F's problem as the RFC says).
- **The patch** has the network's answer where the mask is, **fading out over about 4 px beyond it** (a dilation, then a blur of the mask; alpha is 1 on the mask and 0 farther out), and is cut to that box, so it is a few hundred KB, not a window. The mask is **not** grown here: the UI grows a click-selected mask first (§3.2). Applying a patch (`apply_patch`, what the render path will do) is `dst + (patch - dst) * alpha`, so every pixel outside the fade is the photograph's own.
- **CPU provider only**, session settings as the other models; the session is lazy and `release` drops it.

**Verified:** 10 unit tests without weights (the box, the window at native and reduced sizes, at an edge, over a large hole and on a small image, the network's inputs for three window sizes, the alpha, a patch touching only the mask, the fade, a reduced window) and the protocol; Rust `--lib` 724 and the helper 64 pass. **With the real model** (`lama_fp32.onnx` from the bake-off, a 3840 x 2560 interior, a 513 px disc in the middle): window 1129 px, reduced, patch 529 x 529 placed correctly, **no pixel beyond the fade changed, pixels inside changed**, 5.6 s in a release build including loading the model; looked at by eye: the mantelpiece and a lamp are gone and the wall, panelling and mural continue, with the soft, repeating texture the bake-off reported for large holes. **Not verified:** the Windows build of the helper with this op; the app side (nothing calls the op yet: no supervisor method, no job, no `remove` op, no table, no compositing); the 2.3 s figure for a native-size hole in this code (the run above was a reduced window); a mask drawn by a person; memory.

## 12. Slice D, the design: how a removal reaches the screen, the export and the catalog (2026-11-02)

Written after reading the code the first draft (§3.2) only guessed at, and it **changes slice D's plan**: the draft said "CPU interpreter and GPU path both composite a stored image". They do not need to.

### 12.1 Bake the patch into the picture the canvas loads; no shader

Develop's GPU canvas grades a *source texture* (a downscaled decode). When a photo is AI-denoised, the source texture is replaced by a PNG built on the CPU (`denoise_preview.rs`: "the shader, the masks and the histogram are untouched"). **A removal uses the same mechanism.** The backend builds the source with the photograph's patches laid over it (`removal::apply_removals`, scaled to the tier: draft or 1:1 full), and the canvas loads that file instead of the plain preview. Consequences, all good: the GPU shaders, the mask code and the histogram see the removed picture with **no change and no CPU / GPU parity to maintain**; every consumer that already works from a decode (the graded preview and thumbnail, soft proof, export) calls the same function; and the order the draft wanted, *patches first, before every other edit*, is what happens, because the patch is in the source.

- **Order with AI Denoise:** denoise blends the kept denoised copy of the *unedited* photograph into the preview; applying that blend after the patch would pull the removed object back in at the denoise amount. So the order is plain preview, then the denoise blend, **then the patches**.
- **Cost:** a patch is laid in place (microseconds); what costs is writing the source PNG again (the denoise precedent: written once per photograph, tier and key, only the latest kept). A removal's file is keyed by the hash of the patch ids and the tier.
- **Before / After** shows the plain preview, as it does for denoise: it shows the photograph before any edit, removals included. That is a decision, not an accident: the toggle is "before editing".
- **Export** applies the patches to the full decode, **before** lens correction, perspective, tone and crop (the patch coordinates are in the full decode, so geometry must not have moved anything yet).
- **Scaling:** a patch carries `x, y, w, h` in a decode of `src_w x src_h`; on a smaller decode they are scaled by the ratio and the patch is resized with its colour premultiplied by alpha (so a fade keeps its colour). A patch whose decode has another aspect ratio (the source file was replaced) is **skipped**, never guessed at; the op then shows "needs regenerating". Built and tested (`removal.rs`).

### 12.2 The stored patch and the `remove` op

- **Table** `removal_patches(id, version_id -> image_versions ON DELETE CASCADE, x, y, w, h, src_w, src_h, engine, png)`; built (`catalog/removal_patches.rs`), with a test that a catalog **backup carries the patch** (the reason for a blob, ADR-0010 §4).
- **The op.** The edit stack merges ops by name (`merge_ops`), so removals are **one op**: `{"op": "remove", "items": [{"id", "patch": <patch id>, "engine"}]}`, in the order they were made. The op carries ids, never pixels (the stack is written on every slider change and every history row copies it). It is excluded from presets, Copy Settings and Auto Sync like `crop`, `ai_denoise` and the masks (`PRESET_EXCLUDED_OP_NAMES`).
- **Undo.** A history row holds the whole stack, so undoing past a removal and redoing it needs the patch to still exist. Patches are therefore **kept as long as their photograph is**; deleting a removal in the UI removes the item from the op, not the row. A sweep that deletes the patches no stack and no history row names (`delete_removal_patches_except`) exists and is tested but is **not wired**: it matters only for a photograph with many removals and a long history, and a wrong sweep loses an edit.
- **The mask is not stored yet.** ADR-0010 §5's "needs regenerating" button needs it to re-run; the item gets a `mask` field when that button is built (slice E). Until then a lost patch is shown as lost.

### 12.3 What is built, what is next

Built (slice D2a, this section's PR): the table and its catalog functions, `removal::apply_removals`. **D2b, built: the `remove_object` command** (§12.4). **D2c, built: the `remove` op helpers in `api/develop.js` and its exclusion from presets, Copy Settings and Auto Sync.** **D2d, built: the baked source for Develop, the graded preview, the thumbnail and every export-like render** (§12.5). **Next:** slice E (the Remove tool). (D2e) done early with D2b: the bundled-models entry, `models/INPAINT_MODEL.md`, startup seeding into `inpaint_models/`.

**Not verified:** all of §12.1 on the screen (only the pure scaling is tested); the cost of writing the source PNG for a 24 MP full tier (the denoise blend's number is the proxy; not measured here); that the canvas can take a second kind of replacement source without disturbing the denoise swap's sequencing (`DevelopCanvas.svelte` guards it with sequence numbers; this is the riskiest edit in D).

### 12.4 Slice D2b: the `remove_object` command (2026-11-03)

`remove_object(version_id, shapes, grow_px?)` returns **one result per separate thing** the marks ask to remove, each `{ patch_id, engine, x, y, w, h, src_w, src_h, reduced, ms }`, and stores the patches (`removal_patches`); the frontend adds them to the `remove` op (D2c). `removal_cancel` stops the job between things (and the helper stops the one it is running at its next step).

- **Marks** are `MaskShape`s in coordinates normalised to the photograph: a `polygon` (lasso, even-odd) or a `stroke` (round brush along points, radius a fraction of the longer side). They are rasterised on the **full decode** (the same decode export uses, so a patch's coordinates mean the same everywhere) and **grown by 6 px** by default, the halo and soft shadow an object leaves (`grow_px` overrides).
- **One window per separate thing, not one per call.** My first version sent the whole mask as one: two small objects far apart gave one window spanning both, reduced 3 to 4 times, and **both fills came out blurred streaks** (seen by eye on the bake-off photograph). `mask_clusters` now splits the mask into connected pieces and merges pieces only when one lies inside the other's window (the network would otherwise copy from the neighbouring mark); everything else is its own run with a window sized for it. At most 12 things per call.
- **The helper is given only the window** (a crop of the image and of the mask, the size the helper's own planner would choose), so the 24 MP decode is never written out; the helper's coordinates are relative to the window and the job adds its origin. The two inputs and the patch go through the work folder `removal_work/` under the cache root and are removed whatever happens.
- **Nothing is stored unless every thing was filled**: a cancel or a failure part-way leaves no patch behind.
- **The model** is `inpaint_models/lama_fp32.onnx` under the cache root, seeded from the installer at startup and on first use (`bundled_models::INPAINT`); a build without it answers "the removal model is not installed".

**Verified:** unit tests for the rasteriser (polygon, concave polygon, stroke, dab, growth, clipping) and the clustering (far apart, sharing a window, a chain); the bundle-manifest test now covers the LaMa pin; **the whole job through the real helper binary and the real model** (`removal_commands` ignored test): on a 3840 x 2560 photograph a brush stroke across a panelled wall and a lasso over chairs gave two patches (427 x 248 and 335 x 412 px, 5.4 s and 2.4 s including the model load, release build), the work folder empty afterwards, the patches applied with `apply_removals`; by eye the stroke is gone cleanly and the lasso area is smeared but contained (a hard region for any engine, RFC §9.3). **Not verified:** the command inside the running app (nothing in the UI calls it); Tauri-side cancel; Windows; a mark drawn by a person.


### 12.5 Slice D2d: how the patches reach every render (2026-11-03)

- **The stack carries the pixels, but only on its way to a render.** The renders (graded preview, soft proof, thumbnail, export, print, PDF, enlargement) work from an `EditStack` and have no catalog; the stack names a patch by id. `removal::inline_patches(catalog, version_id, stack)` returns a copy whose `remove` items also hold the patch (`baked`: place, size of the decode it refers to, the PNG as base64), and `removal::bake(image, stack)` lays them. **That copy is never written back**: the stored stack keeps ids (§12.2). I chose this over adding a `patches` parameter to about twenty functions and their tests because one function at the place a command reads the stack covers every consumer below it, and the file names the renders build from a **hash of the stack** then tell two sets of patches apart without a second key. The cost: a render path that reads a stack from the catalog and forgets to call it silently shows no removal. There are eleven such places in `lib.rs` and `upscale_commands.rs` (graded preview, soft proof, history and snapshot hover previews, the CPU-fallback / preset preview, thumbnail, export, print, PDF, enlargement, its preview); a test covers the render functions, not that each command calls it.
- **Where the patches go.** `render_full_resolution` (export, print, enlargement) right after the denoise blend and before lens correction; `ensure_graded_preview_for_hash` (graded preview, soft proof, thumbnail) likewise on the draft decode. Order everywhere: plain, denoise blend, patches (§12.1).
- **The Develop source.** `get_develop_source_preview(path, hash, amount, full, version_id, patch_ids)` replaces `get_denoised_develop_preview`: one file with the denoise mix and the patches (`removal_preview::ensure_baked`, `bake-<hash>-<tier>[_dn<amount>-<secs>]-<key of the patch ids>.png`, only the latest per photo and tier kept). Nothing to mix and nothing to lay answers `null` (the plain preview). **The canvas has one swap, not two**: `appliedDenoise` gained the patch ids it carries (`removals`), `applyDenoiseSource` compares them with what is wanted (none while Before / After shows the photograph before any edit), and the same sequence numbers guard the in-flight request. The canvas exposes `data-removals-source` as it does `data-denoise-source`.
- **The CPU-fallback / preset preview** (`preview_edit_stack`) gained an optional `version_id`, because its stack comes from the frontend; without it a removal would vanish from the fallback canvas and from a preset's hover preview.
- **Verified:** unit tests for the helpers, the Develop source (scaling, denoise first, reuse and sweep, undo) and one test that the graded preview and the export render show a patch only when the stack carries it; `cargo test --lib` 756, vitest 1198; **and a real run** (`e2e/specs/develop-removal-source.e2e.js`, local only: it needs the 208 MB model, which it copies into the app's `inpaint_models` folder of a throwaway HOME) on a 1280 px preview of a night photograph: a mark made through `remove_object`, named in the stack; the canvas source carries the patch id, differs from the plain preview in the box around the mark (mean difference 30.7 of 255) and by 0 in a corner; `\` (Before) switches to the plain source and back; the graded preview shows the same difference; the thumbnail regenerates; an export differs from an export of the same photograph without the removal and is repeatable.
- **Not verified:** the 1:1 tier on the screen (zooming in with a removal; the code path is the same file builder, not looked at); the cost of writing the source for a 24 MP full tier; Windows; that every export-like command calls `inline_patches` (see above); the look of an export (only that it differs); Copy Settings of a photograph that has removals (the op is excluded, tested on the frontend only).

## 13. Slice E1: the Remove tool (2026-11-04)

- **What is built.** A *Remove* button in the Develop tool strip (after Red Eye). It arms as `masks.activeTool === "remove"`, which gives the canvas its full-photograph view for free and keeps one tool active at a time. **Brush** and **Lasso** marks are drawn on `RemoveOverlay.svelte`, a layer mounted inside the canvas's mask overlay, so its box is the canvas's and a mark's normalised coordinates are the photograph's (the tools above it route through `DevelopCanvas`'s own pointer handlers; this one has its own and does not touch them). **Remove** (or Enter) sends the marks to `remove_object`, names the returned patches in the `remove` op through the develop store (`addRemovals`, one history row "Remove object", thumbnail regenerated); **Undo last** takes the newest item out (`removeRemoval`, row "Undo remove"; the patch stays stored so History can bring it back). Cmd/Ctrl + wheel is handed on to the canvas's zoom, Space passes pointers through to its pan.
- **Large-area guard (§3.4).** If the marks' joint extent is over a quarter of the photograph's longer side the first press says the fill may look soft or repeat and offers **Remove anyway**.
- **No model, no marking.** `removal_available` (the pinned file is installed) is asked when the tool is armed; without it the strip says so and the layer takes no marks.
- **If the user switches photos mid-job** the patches are written to the photo they were made on (straight to the catalog), not the open one.
- **Not in E1** (still slice E; click-to-select is E2, §14): *Use Heal instead*, the "needs regenerating" state (needs the mask stored in the item), a progress bar (a removal takes seconds), the "model manager entry" (the model is bundled, so there is none), removal of a single listed item other than the newest, marks on a cropped view (the tool shows the whole photograph while armed, like the mask tools).
- **Verified:** vitest 1216 (store, actions incl. failure, cancel, large mark, photo switched mid-job), svelte-check 0 errors; e2e `develop-remove-tool.e2e.js` (no model: arms, says the model is missing, takes no marks); **real model through the tool's controls** (`develop-remove-tool-real-model.e2e.js`, local): a painted stroke is "1 mark", Remove finishes in 7.6 s on a 1280 px photograph, the patch is in the stack and the canvas source, History shows "Remove object", a lasso is a mark and a second removal follows the first, a large mark asks first, Undo last takes the newest out and the canvas follows. **Looked at:** screenshots of the strip and History row; **the canvas itself is black in WebKit screenshots (the GPU canvas is not captured), so the fill was not seen on screen in this run**, only measured (§12.5).
- **Not verified:** a real mouse drag (the spec uses synthetic pointer events, like the mask-panel drag), touch, the tool on a zoomed-in photograph, Windows, key handling beside the other Develop shortcuts (Backspace and Enter act only while marks are pending and the focus is not in a field), what the strip looks like when it is narrow (it has grown by one more block of controls).

## 14. Slice E2: click-to-select (2026-11-05)

- **What is built.** A third mode, **Select**, beside Brush and Lasso. A click runs the Select Subject model (`segment_prepare` once per photograph, then `segment_decode`), takes its best candidate and lays it over the photograph as a red tint; further clicks refine the same object (Alt-click takes a part away, from the candidate shown, as in Select Subject), **Shift-click** keeps it as a mark and starts another, **Another guess** / Tab cycles the model's candidates. Changing mode or drawing a mark keeps the object as an ordinary mark. The tool's own code (`removalActions.js`) calls the segment API directly and does not use `segment.svelte.js`, so a Remove session and a mask-panel Select Subject session cannot disturb each other.
- **The mark is the cut.** A selected object is `{kind: "segment", logits}`: the model's 256 x 256 gray PNG, stretched over the photograph and thresholded at logit 0 by `rasterise_mask` (bilinear), then grown and split into separate things like any other mark. The screen-only box (for the large-area guard) and tint never reach the job. A cut that cannot be read selects nothing and does not fail the job.
- **Model.** Select needs the Select Subject model (a download the user agrees to). Remove does not own that dialog: without the model the strip says "install it once from Select Subject", and Brush and Lasso carry on.
- **Not in E2:** *Use Heal instead*, "needs regenerating", deleting a removal other than the newest, a candidate picker beyond cycling.
- **Verified:** vitest 1227 (selection, refinement, Alt, Shift, Tab, no-hit click, helper restart, missing model, mode change, what the job is sent, release), `cargo test --lib removal::` (cut stretched over another aspect, bad cut), svelte-check 0 errors; **real models through the tool's controls** (local `develop-remove-tool-real-model.e2e.js`, needs `SEGMENT_MODELS_DIR`): a click on a 1280 px photograph tints a region, Another guess changes it, Remove (14.9 s with the large-area question) puts a patch in the stack and the canvas source, and **the filled water is visible in the screenshot**.
- **Not verified:** a real mouse click (synthetic pointer events), the tint's position on a zoomed or panned photograph, Alt/Shift with a real keyboard, that the model's best candidate is the object the user meant (it picked a water region on the bridge photograph, which is why Another guess exists), Windows.

## 15. Slice E3: delete a listed removal (2026-11-06)

- **What is built.** With two or more removals on a photograph the Remove strip lists them as chips ("1 ×", "2 ×", oldest first). A chip takes that removal out of the `remove` op (`handleRemoveDelete`, History row "Delete removal", thumbnail regenerated); the canvas source follows as it does for *Undo last*. The patch stays stored (§12.3: History can bring the removal back), so no sweep was wired. A single removal still has only *Undo last*.
- **Not in E3:** *Use Heal instead*, "needs regenerating" (needs the mask stored in the item), showing *where* a listed removal is (the chip is numbered, not located), a candidate picker beyond cycling.
- **Verified:** vitest (`removalActions.test.js`: a middle item out and the rest kept, an unknown patch and a running job ignored), svelte-check 0 errors; **real LaMa through the tool's controls** (local `develop-remove-tool-real-model.e2e.js`, 7 passing in 72 s): a second removal is added, the first is deleted by its chip, the stack and the canvas source keep only the second, History's last row is "Delete removal".
- **Not verified:** a real mouse click on a chip, the strip's width with many chips (nothing wraps or caps them), Windows, the Select step in that run (no `SEGMENT_MODELS_DIR`).

## 16. Slice E4: Heal instead (2026-11-07)

- **What is built.** A **Heal instead** button beside **Remove** while Brush / Lasso marks are pending. It turns the marks into one Spot Removal (heal) mask (`healFromMarks.js`, `handleRemoveHealInstead`): a stroke becomes dabs along its path (spacing half a radius, radius converted from the longer side to the Spot dab's width fraction), a lasso is filled with a grid of dabs plus dabs along its outline (at most 600; the spacing grows to fit), and the source is placed beside the area on the first side with room (right, left, below, above) by the area's own size, else on the roomiest side. The mask is selected, the marks dropped, the tool left, one History row "Heal instead". It is the manual tool the RFC keeps (§4), reached from the marks the user already drew; the model does not run.
- **Not in E4:** a click-selected object is refused with a toast (it is a model cut, not geometry; turning it into dabs would need the cut rasterised in the page); healing a finished removal (the mask is not stored with the item, §12.3), which is the same gap "needs regenerating" has; a smarter source than "beside the area" (the existing Spot placement is just as plain); the 8-mask limit is respected by refusing, not by making room.
- **Verified:** vitest 1241 (8 geometry cases: stroke spacing and radius units on a portrait photograph, lasso inside and edge, a concave notch left empty, the dab cap, source side and clearance; 4 action cases: mask created and selected, a selected object refused, the mask limit, no marks / running), svelte-check 0 errors; **the real app** (local `develop-remove-tool-real-model.e2e.js`, 8 passing): a painted stroke and **Heal instead** give a `spot_mask` with mode `heal`, dabs starting at the stroke, a non-zero source offset, no new removal, the tool left, History's last row "Heal instead".
- **Not verified:** how the healed result looks (the canvas is black in WebKit screenshots), whether the automatic source is a good one on a real photograph, a real mouse, Windows.

## 17. Slice E5, step 1: the hole is stored with each patch (2026-11-07)

Follows the [stored-mask proposal](RFC-0032-appendix/stored-mask.md) (§3 to §4, step 1); the proposal's decisions were taken as recommended.

- **What is built.** `removal_patches` gains `mask_x`, `mask_y`, `mask_png` (nullable; added by `ALTER TABLE`, so existing catalogs migrate on open and a backup is still the one SQLite file). `remove_object` stores, with each patch, the hole it filled: a 1-channel PNG of the separate thing's **bounding box** (not the patch rectangle the proposal named: the window the model saw is planned from that box, and `mask_clusters` guarantees the window holds no other marked thing, so the box is all a redo needs). `regenerate_removal(version_id, patch_id)` rebuilds the same window, places the hole in it, runs the model once and stores the result as a **new** patch with the same hole (the old row stays, so History can bring it back); the caller swaps the item in the `remove` op. `removals_with_mask(version_id, patch_ids)` says which removals can be redone. A patch made before this has no hole: not redoable, nothing breaks. If the photograph changed size since the removal, the redo fails with "no longer fits" instead of guessing.
- **Verified:** `cargo test --lib` (catalog: a hole comes back with its patch, only to its own photograph, absent for an old patch; the backup test still passes); **the real model and helper** (`removal_commands` ignored test, release): on a 1280 x 715 decode a brush stroke and a lasso gave two patches (161 x 94 and 130 x 135 px), and **redoing each from its stored hole gave the same place, the same hole and a byte-identical patch PNG** on this machine (the proposal's open question, for this path).
- **Not verified:** the size of a stored hole on real removals (not measured); a redo after a different provider or model version (only the same machine and provider); the commands from the running app (nothing in the UI calls them yet: step 2 is the *Try again* action on the chips, step 3 *Heal instead* for a finished removal).


## 18. Slice E5, step 3: Heal for a finished removal, and for a selected object (2026-11-07)

Follows the [stored-mask proposal](RFC-0032-appendix/stored-mask.md) §4 step 3 (steps 1 and 2 of it: §17, and *Try again*, which is still to do).

- **What is built.** `healFromMarks.js` gains a raster path: `planHeal(shapes, aspect, grids)` takes `MaskGrid`s (a mask placed on the photograph by a normalised box) and puts one dab on each grid cell (about a tenth of the hole's longer side) that the hole touches, radius 0.8 of the cell so a cell is fully covered; the dab cap and the source placement are the same as for strokes and outlines. Two things use it. (1) A **finished removal**: the new command `removal_hole(version_id, patch_id)` returns the stored hole, `maskRaster.js` reads the PNG, and `handleRemoveHealRemoval(patch)` takes the removal out of the `remove` op and adds the selected Spot Removal (heal) mask in **one** edit, "Heal instead" (the patch stays stored, so History brings the removal back). A **Heal** chip appears beside each removal's delete chip in the Remove strip, only for removals that have a hole (`removals_with_mask`, read into `removal.healable` when the list changes); older removals show none. (2) A **click-selected object** pending in the tool: E4 refused it; now its 256 x 256 cut is read the same way and healed, together with any Brush / Lasso marks.
- **Not in this step:** *Try again* (step 2); editing the marks of a finished removal; a smarter source than "beside the area".
- **Verified:** vitest (planner: empty mask, whole-hole coverage and nothing outside the box, a ring's middle left out, a selected object with another mark, the cap; actions: swap in one edit with the dabs inside the hole's box, last removal drops the op, no stored hole, mask limit, running, healable list), svelte-check, `cargo check`; the real app with the real model (see PROGRESS).
- **Not verified:** how the healed result looks (the canvas is black in WebKit screenshots), a selected object healed in the real app (no `SEGMENT_MODELS_DIR` here), the hole sampling on a large real removal (the cap may coarsen it), a real mouse, Windows.

## 19. Slice E5, step 2: Try again (2026-11-07)

Follows the [stored-mask proposal](RFC-0032-appendix/stored-mask.md) §4 step 2.

- **What is built.** A **Try again** chip in the Remove strip, beside a removal's Heal chip, for a removal that **has a stored hole and was made with another engine id than the installed one** (`removal_engine`, read when the tool is armed; the item carries its `engine`). It runs `regenerate_removal` (§17) through `handleRemoveRedo`: the strip shows "Removing…" with Cancel as for any removal, the new patch takes the old one's place in the `remove` op (`replaceRemoval`) in one edit, History row "Redo removal", thumbnail regenerated. The old patch row stays, so History brings the old result back. Failures (no stored hole, "no longer fits" after the photograph changed size, cancel) are shown as for Remove and change nothing.
- **Why only when the engine differs.** §10 measured LaMa as deterministic on one machine and provider, and §17 confirmed a byte-identical redo from the stored hole. A chip that re-ran the same model would do nothing visible, so it is absent. The chip's tooltip says the result may differ. The engine id names the model, not the execution provider, so a change of provider alone (GPU to CPU, another machine) does not show the chip; that case is not covered.
- **Verified:** vitest (redo swaps in place with the installed engine in one edit; failure and cancel leave the stack alone; written straight to the catalog when the user moved to another photo; unknown removal and running ignored; the engine is read on arming), svelte-check, `cargo check`; the real app with the real model (`develop-remove-tool-real-model`, 10 passing): no chip on a fresh removal; after its engine id is rewritten to an older one the chip appears and the redo replaces the patch in place, keeps both rows and their holes, and writes "Redo removal".
- **Not verified:** an actual second engine (the spec says a real removal came from an older one by rewriting its `engine` in the stack), a redo after a provider change, the look of a changed result, a real mouse, Windows.

## 20. Slice E5, step 4: a removal that is missing or no longer fits is called out (2026-11-07)

Follows the [stored-mask proposal](RFC-0032-appendix/stored-mask.md) §2 and §4 step 4: with the patch a blob in the catalog, the two reachable bad states are a patch **row that is missing** and a patch whose decode has **another aspect ratio** than the photograph (the source file was replaced), which the render skips (§12.1).

- **What is built.** `removal_info(version_id, patch_ids)` replaces `removals_with_mask`: for each patch the decode it was made on and whether it has a hole; a removal with no row is left out. `removal.rows` keeps the answer per patch (`null` = no row), read when the list changes (`refreshRemovalRows`). `removalFit.js` judges a row against the open photograph with the render's own tolerance (1 % relative difference of the aspect ratios). A removal with a problem shows a red line in the Remove strip, "Removal n is missing" or "Removal n no longer fits this photo: take it out" (the tooltip explains), and loses its Heal and Try again chips (neither could work); **Undo last** or the numbered chip takes it out as for any removal. Nothing is regenerated or deleted automatically.
- **Why no "Regenerate" button here.** As the proposal found, the hole lives in the same row as the patch, so a missing row has no hole, and a photograph of another shape would be filled from a hole made for the old one. The honest action is to take the removal out.
- **Verified:** vitest (the judgment: not asked, missing, same shape on a smaller decode, a pixel of rounding, another shape, rotated, size unknown; the rows are read, a failure and a stale answer are ignored), `cargo test --lib catalog::removal_patches`, svelte-check; the real app with the real model (`develop-remove-tool-real-model`): a stack item naming a patch with no row is called out as "Removal 2 is missing", the real removal is not, and Undo last takes the missing one out and the call-out goes.
- **Not verified:** the "no longer fits" case in the real app (the page cannot change a patch's stored size or replace the file; only the pure judgment is tested), that the render's skip and this judgment agree on a real replaced file, a real mouse, Windows.

## 21. Slice F1: does adding texture to a soft fill help? A first measurement (2026-11-08)

Slice F's main problem is §9.3's: on a medium or large hole LaMa sees the window reduced and its fill is soft (`detail` 0.29 median for M, 0.10 for L). This is the cheapest possible test of "detail transfer", run on the bake-off's 60 cases with the stored LaMa fills (no new model run): [f1_detail_proto.py](RFC-0032-appendix/f1_detail_proto.py).

- **Method.** In the fill, add band-passed (difference of Gaussians, sigma 0.8 and 2.0 px) white noise on the luma only. Its amplitude is what the fill lacks against the ring just outside the hole (RMS of the same band in the ring, 4 to 24 px out, against RMS in the fill: `sqrt(max(0, ring² - fill²))`), faded in over 6 px from the edge. This is a stand-in for a real texture source (a bank taken from the ring, or a grain model), not the thing to ship.
- **Measured (60 cases, scored inside the hole against the original).** Median `detail`: S 0.89 → 0.92, **M 0.29 → 0.49**, **L 0.10 → 0.55**, all 0.29 → 0.69. PSNR falls by about 0.2 dB (all 19.1 → 18.9; S 23.2 → 23.0, M 18.1 → 18.0, L 16.0 → 15.8). `detail` only counts energy, not whether it is the right texture.
- **By eye** ([f1_detail_sheet.png](RFC-0032-appendix/f1_detail_sheet.png): every fifth M or L case, 8 of them, original | LaMa | LaMa + detail). **Helps** where the real area is textured: plaster (building2 M), a carved wall (architecture3 L), a little on grass. **Hurts** where the real area is clean or smooth: a white sky (people4 M), a plain wall (portrait2 L), the soft background of a bokeh shot (people1 L) get a visible speckle that the photograph does not have. Where the structure itself is wrong (building4 L, a street in the place of a shop front), texture does not help. The noise is also grain-like and colourless, so it only reads as right where the real texture is grain.
- **Conclusion.** The measure moves (`detail` 0.29 → 0.69) and the picture sometimes does not follow, so **a naive "match the ring's energy" is not a default**. What this leaves for F: (1) gate it, so it is applied only when the ring is clearly textured and the fill is clearly softer (the clean-sky cases have a small ring energy, but the ring there still carries a little sensor noise that this adds back at the wrong scale); (2) take the texture from the ring itself (patches of the photograph's own high-pass), not from noise, so it carries the right grain and scale; (3) judge by eye on the user's own photographs, which this study did not use.
- **Not verified:** a real texture source; the gate; any photograph of the user's; whether a reduced-window cause (the 512 px network) is better fixed by a second pass of LaMa on tiles at native size (not tried); Windows. 8 of the 40 M and L sheets looked at, chosen as every fifth, not by result.

**Needs from the user:** whether detail transfer is worth pursuing before real removals on your own photographs show how often the soft fill is the problem (the removal tool now exists, so this can be judged on real use); if yes, the next step is (2) with a gate.

## 22. Slice F2: texture from the ring itself, gated: worse than F1 by eye (2026-11-08)

The second candidate left by §21: take the missing texture from the photograph's own surroundings instead of noise, and apply it only when it is warranted ([f2_ring_texture_proto.py](RFC-0032-appendix/f2_ring_texture_proto.py)).

- **Method.** The high-pass (luma minus a Gaussian of sigma 2) of the ring 4 to 40 px outside the hole is cut into 32 px blocks; blocks of the ring are copied at random over the hole and overlap-added with a Hann window, scaled to the deficit as in §21. **The gate:** only when the ring's high-pass RMS is at least 2.5 grey levels and the fill's is at most 0.6 of it.
- **Measured (60 cases).** The gate fires on 39 (S 4, M 15, L 20). Median `detail`: S 0.89 → 0.90, **M 0.29 → 0.86**, **L 0.10 → 0.91**, all 0.29 → 0.90; PSNR falls about 0.4 dB (all 19.1 → 18.7). By the number this beats §21's noise (0.69).
- **By eye** (the same 8 cases as §21, [f2_ring_texture_sheet.png](RFC-0032-appendix/f2_ring_texture_sheet.png)): **worse than §21's noise and often worse than plain LaMa.** A sigma-2 high-pass of a real photograph is not grain, it holds edges, so copying its blocks copies **edge fragments**: wire pieces and lettering ("a" shapes) appear in a white sky, scratches over a plain wall, a repeating pattern over a blurred background, and the block seams show as a tile. On the plaster and the carved wall it reads as plausible damage but not as the right damage. The gate did not help here: in these eight cases it fired on all of them, including the clean sky and the bokeh, because their rings do carry enough high-pass energy once a nearby edge falls inside the 40 px band.
- **What this teaches.** (1) **`detail` (Laplacian energy) is a poor judge** of a fill: both attempts raised it a great deal while the picture got worse on the clean cases; it cannot tell grain from edges or noise. It stays a sanity signal, as §9.3 said. (2) Both "add something to the soft fill" routes are post-hoc paint over a fill whose structure is already decided; neither knows where texture belongs. (3) The remaining routes address the cause: LaMa sees a **reduced** window (§9.1). A **native-size second pass** (run the network again on tiles of the hole at the photograph's own resolution, seeded with the first fill) would give the fine texture its own scale; it costs time (each tile about 2 to 3 s) and may still smear. Or accept the soft fill and rely on the guard, **Try again** and **Heal instead** (all built) for the cases where it is not good enough.
- **Recommendation: stop slice F's detail transfer here** and judge real removals on the user's photographs first (the Remove tool now runs under `make dev`); open the native-size second pass only if soft fills turn out to be the main complaint. Slice F's other half, halo growth, is already a parameter (`grow_px` in `rasterise_mask`) and needs real cases, not a bake-off.
- **Not verified:** the native-size second pass (not tried); the user's photographs; any gate better than a ring-energy threshold; Windows. Looked at 8 of 40 M and L cases (every fifth), the same as §21.

## 23. "No longer fits" checked in the real window; a replaced file is not noticed at all (2026-11-09)

The one backlog check left that needed no photographs: §20's "no longer fits" call-out in the running app. New spec `develop-removal-no-longer-fits` (local only, needs the LaMa file): a real removal on the bridge photograph (stored decode 1280 x 715, no call-out), then the patch row's recorded decode size is rewritten to 715 x 1280 in the catalog file with `sqlite3`, the photograph is reopened, and the Remove strip shows **"Removal 1 no longer fits this photo: take it out"**; **Undo last** takes it out and the call-out goes. **2 passing in 2 min 16 s**; the screenshot shows the red text (the canvas is black in WebKit screenshots, as before).

**What this does not show, and a finding.** My first attempt overwrote the source file with a photograph of another shape. **Develop did not notice**: the catalog records a photograph's pixel size and content hash only at import (the one later writers are the upscale job and a one-time header repair at startup), and the Develop preview cache is keyed by that stored hash, so after the file changes on disk the open photograph still reports its old size, the call-out cannot fire, and the old preview is served. The app has no relink or re-read command. So today a file replaced in place is simply not seen until it is imported again (a new row, with no stack carried over); the "no longer fits" case is reachable only when a stack or patch is paired with a differently shaped decode by other means (a copied stack, an edited catalog). Whether that should change (re-read the file when its modified time moves, or a "re-read this photo" action) is a product question, not made here.

**Still not verified:** that the render's skip (`apply_removals`, same 1 % tolerance) and the strip agree on a pixel-level real decode (the spec edits the recorded size, it does not make a differently shaped decode of the same patch), a real mouse, Windows.
