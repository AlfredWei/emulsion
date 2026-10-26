# RFC-0032: Remove objects: AI inpainting for spot removal (M7, the "generative fill" decision)

- Status: Draft for review (slice A, the bake-off, run on 2026-10-26: results in §9; user answers to §8 recorded there)
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
- **The patch is a stored file, not a cache entry (provisional: the user asked for a small study of this before it is decided, slice C2).** A GAN's output can differ between CPU and CoreML, so regenerating it later could change a picture the user already accepted. It lives next to the catalog data (backed up with it) and is deleted with its op. If the file is missing the op shows a "needs regenerating" state rather than silently re-running.
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
| **B** | Licence step and ADR: Places2 terms (§9.4), LaMa weights file, bundle vs download of 208 MB, where patches live | Needs the user's licence decision |
| **C** | UX mock of the Remove tool (§3.4): [remove-tool-mockup.html](../ux/mockups/remove-tool-mockup.html) | **Done** (awaits the user's use of it) |
| **C2** | **Small study: stored patch vs regenerate on demand** (§3.2, the user's question 3). Run with the mock and a prototype: 5 to 8 photographers (the user first), tasks "remove, edit further, close, come back / open on the other machine / copy the catalog", compare *a fixed stored result* with *a result that re-runs and may differ*: measure surprise, trust, and what they would pick. Decides §3.2 before slice D builds it | After C |
| **D** | Engine and job: `inpaint_run` helper op (LaMa), crop / blend, **detail transfer** for holes the 512 px network sees reduced, the `remove` op with stored patches, CPU and GPU compositing | After B and C2 |
| **E** | Develop UI: the Remove tool, paint / click / lasso, accept / undo, Heal fallback, large-region guard, history entries, model manager entry | After D |
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

Open, for the user:

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
