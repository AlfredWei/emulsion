# RFC-0032: Remove objects: AI inpainting for spot removal (M7, the "generative fill" decision)

- Status: Draft for review (desk research only; nothing measured, nothing downloaded)
- Date: 2026-10-26
- Relates to: [MILESTONES.md](../../PRD/MILESTONES.md) M7 (the decision this RFC must make before any implementation), [RFC-0023](RFC-0023-on-device-ai-inference.md) §5.6 (generative removal "not surveyed"; M7 owns it), [RFC-0031](RFC-0031-intelligent-culling.md) §5 slice F (the user's decision: generative fill, framed as spot removal, after culling), [RFC-0026](RFC-0026-click-select-segmentation.md) (click-to-select an object: the natural way to make the mask), [RFC-0027](RFC-0027-ai-denoise.md) / [RFC-0029](RFC-0029-ai-super-resolution.md) (the `emulsion-ai` helper, the model manager and consent flow, a cached result beside the edit stack), [RFC-0025](RFC-0025-composable-masking.md), [RFC-0020](RFC-0020-grain-particle-noise.md) (grain, for matching a patch to its surroundings), [ADR-0009](../adr/ADR-0009-ml-inference-runtime.md)

## 0. Process note

Design and desk research. **No product code is changed, no model was downloaded, and no number below was measured on this machine.** Every figure is either read from a model page or paper (and marked so) or is a plain statement about how the app works today. Licences are read from the pages named in §2 and **not** from the files that would ship; the training data's terms are **not resolved**. The first slice (§6, slice A) is therefore a bake-off with real numbers, and it needs the user's go-ahead to download about 236 MB of third-party model files (§8, question 1).

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

**What the research does not give** (so slice A must): peak memory and CPU time of MI-GAN and LaMa on a desktop, whether either runs on ORT's CoreML and DirectML providers, how they look on **full-resolution** photographs (both work at 512 px; our photos are 24 to 60 MP), and any comparison against a plain PatchMatch fill. Published quality numbers are the authors' own, on Places2 and similar, at 512 px.

## 3. What this RFC proposes

### 3.1 The decision, conditional on slice A

**Do not ship a diffusion / prompt-driven generative model in M7.** Ship **removal by inpainting**, and let slice A decide which engine, by a three-way bake-off on the same masks:

1. **PatchMatch** (no model, no download, deterministic, our code);
2. **MI-GAN-512** (28 MB);
3. **LaMa-fp32** (208 MB), as the quality reference.

**Rule:** a model ships only if it is **clearly better than PatchMatch on the cases a photographer cares about** (§5) and fits the size, time and licence limits. If neither beats PatchMatch clearly, **M7 ships PatchMatch only**, which is exactly the "non-generative fallback" MILESTONES.md allows, and the model question is closed with evidence. If MI-GAN is within reach of LaMa it wins on size (28 MB can be **bundled in the installer** like the other models; 208 MB is a download with the consent flow, or a bundle decision for the user).

### 3.2 It is an edit, not a new photo

Unlike Enlarge (RFC-0029), removal **keeps the pixel grid**, so it belongs in the edit stack: one new op kind, `remove` (working name), per removal:

- the **mask** (normalised, resolution-independent: brush strokes or a polygon; a click-select result from RFC-0026 is converted to a polygon, then grown by a few pixels because inpainters need the halo and shadow covered),
- the engine and its version (`patchmatch-1`, `migan-512-places2`, ...),
- a **patch id** pointing to the stored result: a PNG of the region (mask box plus context) with a feathered alpha, written once.

The pipeline applies patches **first**, to the source pixels, before tone, colour and the other masks (as the Spot mask reads pre-mask pixels today; and as denoise's cached stage sits ahead of tone, RFC-0027). So every other edit sees the removed picture.

- **Non-destructive and undoable** through the existing history, like any op.
- **Per-photo**: excluded from Copy Settings and Auto Sync the way crop and spot removal already are (RFC-0028 §3.5), because a mask is only meaningful on its own photo.
- **The patch is a stored file, not a cache entry.** A GAN's output can differ between CPU and CoreML, so regenerating it later could change a picture the user already accepted. It lives next to the catalog data (backed up with it) and is deleted with its op. If the file is missing the op shows a "needs regenerating" state rather than silently re-running.
- **Not exportable to XMP** (no standard to carry it); the existing sidecar story for AI results applies.
- CPU interpreter and GPU path both just **composite a stored image over a region**; no model runs in the render path.

### 3.3 The job

- Input is the **unedited full-resolution decode**, cropped around the mask with context.
- **Context rule (to be measured):** if the mask's box plus a margin fits in 512 px at native resolution, run at native resolution (no loss); otherwise reduce to 512, inpaint, and put the original's high-frequency detail and grain back (RFC-0020's generator can match the surrounding grain), so a large removal on a 24 MP photo is not a soft smudge. How well that works is the main open quality question.
- A new helper op `inpaint_run` next to `denoise_run` and `upscale_run`, on the same reader thread, with `progress` and `cancel`; one job at a time; Select Subject waits.
- Memory is bounded by the crop (512 px network input), not by the photo: much smaller than denoise or enlargement. Not measured.

### 3.4 UX (to be designed as an interactive mock first)

The RFC-0028 / 0029 / 0031 way: mock before code. The shape to put in the mock:

- A **Remove** tool in Develop's tool strip beside Heal / Clone. **Three ways to mark:** paint, click an object (Select Subject's click, with the three candidates), or lasso.
- The result appears **in place in about a second or two**, with **Accept** / **Undo**, and a **Before / After** that holds the key. **Use Heal instead** is one click on the same mask, so the old tool is never taken away.
- A few spots can be marked in one go; each is its own op in the history list.
- **No prompt box, no "generative" wording.** Plain "Remove". The model manager says what is downloaded and why, as for the other AI tools.
- A **large-region guard**: above a size limit the tool says the result may be poor and offers to proceed.
- Honest limits shown in the mock's copy: it invents plausible background; it can repeat texture; faces and text in the filled area are not reliable.

## 4. Not in this RFC

- **Prompted generation / text-to-image fill / outpainting** (diffusion, GBs, GPU). Revisit only if the user wants it after seeing removal; the PRD's non-goals already put Firefly-style features out of scope unless revisited.
- **Sky replacement, object replacement, face or body generation.**
- **Cloud inference** (PRD: no cloud).
- **Batch removal across photos** and **removal in Library**: Develop only.
- **Video.**

## 5. How the bake-off judges (slice A)

Cases (about 20 photographs, mine and the user's): a dust spot on sky; a wire across a sky; a small object on grass or sand; a person at the edge of a beach shot; an object against a fence or building edge; a large hole (a quarter of the frame, expected to be bad); skin blemish; a hole across a gradient; a hole over text (expected to fail).

Measured: **time and peak memory** on this Mac (CPU, and CoreML where the model loads) and on the Windows CI runner; **ground-truth error** by masking a region of a real photo and comparing the fill with what was there (PSNR / SSIM on the masked area, as a sanity signal only); and **looked at by eye at 100 % on a full-resolution photo**, which is the judgement that matters and the one RFC-0023 §4.7 learned not to skip. The user rules on the by-eye table.

## 6. Slice plan

| Slice | What | Done when |
|---|---|---|
| **A** | Bake-off spike: PatchMatch (written for the spike), MI-GAN, LaMa on §5's cases; times, memory, CoreML / CPU, licence files and training-data terms read from the real files | A table of numbers and a by-eye verdict in this RFC; the engine decision (§3.1 rule) written down |
| **B** | ADR (engine, licence, bundle vs download, where patches live) | ADR accepted by the user |
| **C** | UX mock of the Remove tool (§3.4) | The user has used the mock and answered its open points |
| **D** | Engine and job: PatchMatch in Rust and / or the `inpaint_run` helper op, crop / blend / detail transfer, the `remove` op in the edit stack with stored patches, CPU and GPU compositing | Tests: determinism, mask edge cases, patch round trip, missing patch state; a real removal in the bundled app |
| **E** | Develop UI: the Remove tool, paint / click / lasso, accept / undo, Heal fallback, history entries, model manager entry | Component tests; real-window pass |
| **F** | Quality work the bake-off shows is needed (grain matching, large-region guard, halo growth) | By-eye pass on the user's own photographs |

## 7. Not verified (everything, until slice A)

- Every speed, memory and quality claim: none was measured. MI-GAN's latency is the authors' on an iPhone; LaMa's has no figure at all here.
- **Licences of the files that would ship**: MI-GAN's `LICENSE-WEIGHTS` names only "Software" (it does not say "weights"), its two hosts disagree on detail, and **Places2's terms were not read** (one port says CC-BY 4.0). LaMa's card says Apache-2.0 and the same Places2 caveat applies. SD's licence and size are from memory.
- Whether ORT runs LaMa's FFT block on CoreML or DirectML (probably CPU only), and on Windows at all.
- Whether a 512 px result can be made to look right on a 24 to 60 MP photograph (§3.3).
- That PatchMatch is worse; it may well be good enough.
- The edit-stack and GPU-path design (§3.2) against the real pipeline code: read from comments only.

## 8. Questions for the user

1. **May I download the two ONNX files for slice A?** `migan_pipeline_v2.onnx` (28.1 MB, from `huggingface.co/andraniksargsyan/migan`) and `lama_fp32.onnx` (about 208 MB, from `huggingface.co/Carve/LaMa-ONNX`), into a scratch folder only, for measurement; nothing goes into the app until the ADR.
2. **Scope:** removal only, no prompts and no diffusion in M7: right?
3. **Stored patch** (§3.2, a real file next to the catalog, never silently regenerated): acceptable, or would you rather it regenerate on demand?
4. **If no model clearly beats PatchMatch**, ship PatchMatch alone and say so: acceptable? (My recommendation: yes.)
5. **Photographs:** can you give me 5 to 10 of your own where you would reach for Remove (an object at an edge, a wire, a blemish)? The bake-off is only as good as its cases.
