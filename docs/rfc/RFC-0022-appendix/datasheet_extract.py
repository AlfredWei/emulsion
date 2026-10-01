#!/usr/bin/env python3
"""RFC-0022 appendix: re-extract every published grain figure the RFC cites,
directly from the manufacturers' own data sheets (not from reviews or forum
posts). Downloads the PDFs, extracts their text with pypdf, prints the grain
passages. Needs network access:

    python3 -m venv venv && ./venv/bin/pip install pypdf
    ./venv/bin/python datasheet_extract.py

Output is compared by eye with the RFC's table; PDF text extraction loses
table layout, so the numbers are read from the printed lines, not parsed.
"""
import logging, re, subprocess, sys, tempfile, os

logging.disable(logging.CRITICAL)
import pypdf

SOURCES = {
    "Kodak Portra 400 (E-4050, Feb 2016)": "https://business.kodakmoments.com/sites/default/files/files/products/e4050_portra_400.pdf",
    "Kodak Gold 200 (E-7022, 2023)": "https://kodakprofessional.com/sites/default/files/wysiwyg/pro/resources/E7022%20Gold%20tech%20sheet.pdf",
    "Kodak Tri-X 320/400 (F-4017, Oct 2021)": "https://kodakprofessional.com/sites/default/files/wysiwyg/film/f4017_trix_320400.pdf",
    "Kodak Print Grain Index explainer (E-58, Jul 2000)": "https://filmcolors.org/wp-content/uploads/2025/12/Kodak_Print-Grain-Index_E-58.pdf",
    "Fujicolor Superia X-TRA 400 (product bulletin)": "https://125px.com/docs/film/fuji/superia_xtra400_datasheet.pdf",
    "Kodak Vision3 500T 5219/7219 (technical data)": "https://www.kodak.com/content/products-brochures/Film/VISION3_5219_7219_Technical-data.pdf",
    "Ilford technical information (Delta/Pan F/Pan/XP2/HP5...)": "https://tomzych.com/files/pdf/foto/ilford_tech_info_full.pdf",
}
PATTERN = re.compile(r"granular|print grain|grain index|rms|jnd|just.noticeable|visual threshold|magnification|Print Size|Negative [Ss]ize|Print Grain\s*$|resolving|lines/mm|T-?GRAIN", re.I)

for name, url in SOURCES.items():
    path = os.path.join(tempfile.gettempdir(), re.sub(r"\W+", "_", name) + ".pdf")
    subprocess.run(["curl", "-sL", "-m", "90", "-A", "Mozilla/5.0", "-o", path, url], check=False)
    print("=" * 78, "\n" + name, "\n" + url)
    try:
        reader = pypdf.PdfReader(path)
    except Exception as e:  # network failure, moved file, ...
        print("  could not read:", e)
        continue
    shown = 0
    for i, page in enumerate(reader.pages):
        for line in (page.extract_text() or "").split("\n"):
            if PATTERN.search(line) and shown < 40:
                print(f"  p{i+1}: {line.strip()[:160]}")
                shown += 1
    if shown == 0:
        print("  (no grain statements found)")
