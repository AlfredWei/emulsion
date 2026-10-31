# RFC-0037 appendix: the detector comparison

Offline Python, run from `mask_training/detector_eval/` (outside this repository; needs the `mask_training/venv` with torch, torchvision, transformers, onnxruntime, pycocotools). Nothing here is imported by the app.

| Step | Script |
|---|---|
| fetch library-hosted models | `fetch_models.py` (YOLOX ONNX files come from the YOLOX GitHub release) |
| COCO val2017 (the host caps each connection) | `fetch_ranges.py`, `remote_member.py` (one JSON member of the annotations zip) |
| build the set (faceless persons via the app's YuNet port, `docs/rfc/RFC-0033-appendix/faceheur.py`) | `build_set.py` |
| run detectors / score | `detectors.py`, `run_detectors.py`, `evaluate.py` |
| box to mask with SAM 2 Tiny (the app's preprocessing) | `sam_cut.py`, `run_cuts.py`, `score_cuts.py` |
| ONNX export check | `export_onnx.py` |

Paths inside the scripts are those of the machine they ran on.
