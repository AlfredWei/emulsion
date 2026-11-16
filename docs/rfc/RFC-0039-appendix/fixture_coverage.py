"""What the lens-profile golden fixture covers, measured against the lens database it was made from.

Reads only the two inputs RFC-0039 allows a clean-room implementer: the fixture JSON and the database XML.

	python3 -I docs/rfc/RFC-0039-appendix/fixture_coverage.py \
		app/src-tauri/src/testdata/lens_profile_golden.json \
		"$(dirname "$(cargo metadata --format-version 1 --manifest-path app/src-tauri/Cargo.toml \
			| python3 -c 'import json,sys; print(next(p["manifest_path"] for p in json.load(sys.stdin)["packages"] if p["name"] == "lensfun"))')")/data/db"
"""

import collections
import glob
import json
import sys
import xml.etree.ElementTree as ET


def default_name(el, tag):
	return next((n.text for n in el.findall(tag) if not n.get("lang")), "") or ""


def main(fixture_path, db_dir):
	files = sorted(glob.glob(db_dir + "/*.xml"))
	models = collections.Counter()
	lenses_by_name = collections.defaultdict(list)
	cameras = 0
	for path in files:
		root = ET.parse(path).getroot()
		cameras += len(root.findall("camera"))
		for lens in root.findall("lens"):
			lenses_by_name[(default_name(lens, "maker") + " " + default_name(lens, "model")).strip()].append(lens)
		for el in root.iter():
			if el.tag in ("distortion", "tca", "vignetting") and el.get("model"):
				models[el.tag + " " + el.get("model")] += 1
	tags = collections.Counter(el.tag for path in files for el in ET.parse(path).getroot().iter())
	lens_count = sum(len(v) for v in lenses_by_name.values())
	print(f"database: {len(files)} XML files, {cameras} cameras, {lens_count} lenses")
	print(f"  calibration entries: {dict(sorted(models.items()))}")
	real_focal_attrs = sum(1 for path in files for d in ET.parse(path).getroot().iter("distortion") if d.get("real-focal"))
	print(f"  <center> elements: {tags['center']}, <type> elements: {tags['type']}, real-focal attributes: {real_focal_attrs}")
	print(f"  lens names (maker + model) shared by more than one entry: {sum(1 for v in lenses_by_name.values() if len(v) > 1)}")

	queries = json.load(open(fixture_path))["queries"]
	resolved = [q for q in queries if q["expected"]]
	print(f"fixture: {len(queries)} queries, {len(resolved)} resolve")
	kinds = collections.Counter()
	for q in resolved:
		e = q["expected"]
		kinds["distortion " + (e["distortion"]["model"] if e["distortion"] else "none")] += 1
		kinds["tca " + (e["tca"]["model"] if e["tca"] else "none")] += 1
		kinds["vignetting " + ("pa" if e["vignetting"] else "none")] += 1
		kinds["camera unmatched"] += "camera unmatched" in e["camera"]
		kinds["non-zero lens centre"] += e["lens_center_x"] != 0 or e["lens_center_y"] != 0
	print(f"  resolved by kind: {dict(sorted(kinds.items()))}")
	print(f"  queries with camera_make missing but camera_model present: {sum(1 for q in queries if q['camera_make'] is None and q['camera_model'])}")

	at_sample = between = outside = 0
	real_focal = collections.Counter()
	for q in resolved:
		entries = lenses_by_name.get(q["expected"]["lens"])
		if not entries:
			continue
		samples = entries[0].findall("calibration/distortion")
		focals = [float(d.get("focal")) for d in samples]
		f = q["focal_length"]
		if focals and (f < min(focals) - 1e-4 or f > max(focals) + 1e-4):
			outside += 1
		hit = [d for d in samples if abs(float(d.get("focal")) - f) < 1e-4]
		if not hit:
			between += 1
			continue
		at_sample += 1
		d = hit[0]
		if d.get("real-focal"):
			predicted, rule = float(d.get("real-focal")), "explicit real-focal"
		elif d.get("model") == "ptlens":
			predicted, rule = f * (1 - float(d.get("a", 0)) - float(d.get("b", 0)) - float(d.get("c", 0))), "focal*(1-a-b-c)"
		elif d.get("model") == "poly3":
			predicted, rule = f * (1 - float(d.get("k1"))), "focal*(1-k1)"
		else:
			predicted, rule = f, "focal"
		real_focal[rule + (" holds" if abs(predicted - q["expected"]["real_focal"]) < 1e-4 * f else " fails")] += 1
	print(f"  resolved focal lengths: {at_sample} at a calibration sample, {between} between samples, {outside} outside the calibrated range")
	print(f"  real_focal at a calibration sample, by the rule it follows: {dict(sorted(real_focal.items()))}")

	groups = collections.defaultdict(list)
	for q in queries[:-30]:
		groups[" ".join((q["lens_model"] or "").lower().split())].append(q["expected"] is not None)
	never = sum(1 for v in groups.values() if not any(v))
	mixed = sum(1 for v in groups.values() if any(v) and not all(v))
	print(f"  database-derived lens names: {len(groups)}, never resolve in any spelling: {never}, resolve in some spellings only: {mixed}")


if __name__ == "__main__":
	main(sys.argv[1], sys.argv[2])
