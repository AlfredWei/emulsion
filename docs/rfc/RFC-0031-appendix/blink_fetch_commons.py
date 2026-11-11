"""Fetches photographs of people with closed / squinting / sleeping eyes from Wikimedia Commons categories (permissive licences only),
as <=2000 px JPEG renditions, into ./commons, with attribution.csv. Local research use; CC BY / BY-SA need credit if ever shown."""
import csv, json, os, re, sys, time, urllib.parse, urllib.request
UA = "emulsion-research/0.1 (personal photo-app research)"
HERE = os.path.dirname(os.path.abspath(__file__)); OUT = os.environ.get("OUT") or os.path.join(HERE, "commons")
CATS = (os.environ.get("CATS") or "Closed eyes|People with closed eyes|Sleeping people|Squinting|Blinking|Yawning").split("|")
MINW = int(os.environ.get("MINW", "1200"))
SEEN = set(l.split(",")[-1].strip() for f in (os.environ.get("EXCLUDE") or "").split("|") if f and os.path.exists(f) for l in open(f))
OK = re.compile(r"^(cc0|public domain|pd|cc[- ]by(-sa)?[- ]\d|cc[- ]by(-sa)?$)", re.I)
def get(url, timeout=60):
    for wait in (5, 15, 40, 90, 0):
        try: return urllib.request.urlopen(urllib.request.Request(url, headers={"User-Agent": UA}), timeout=timeout).read()
        except urllib.error.HTTPError as e:
            if e.code != 429 or not wait: raise
            print("429, waiting", wait, "s"); time.sleep(wait)
def api(**p):
    time.sleep(1.5)
    return json.loads(get("https://commons.wikimedia.org/w/api.php?" + urllib.parse.urlencode(dict(format="json", **p))))
def members(cat):
    out, cont = [], {}
    while len(out) < 300:
        d = api(action="query", list="categorymembers", cmtitle="Category:" + cat, cmtype="file", cmlimit=100, **cont)
        out += [m["title"] for m in d.get("query", {}).get("categorymembers", [])]
        if "continue" not in d: break
        cont = d["continue"]
    return out
titles = []
for c in CATS:
    try: t = members(c)
    except Exception as e: print("skip", c, e); continue
    print(c, len(t)); titles += [x for x in t if x.lower().endswith((".jpg", ".jpeg"))]
titles = list(dict.fromkeys(titles)); print(len(titles), "jpeg files in the categories")
rows = []
for i in range(0, len(titles), 20):
    d = api(action="query", titles="|".join(titles[i:i + 20]), prop="imageinfo", iiprop="url|size|extmetadata", iiurlwidth=1920)
    for pg in d["query"]["pages"].values():
        ii = (pg.get("imageinfo") or [None])[0]
        if not ii: continue
        md = ii.get("extmetadata", {}); lic = md.get("LicenseShortName", {}).get("value", "")
        if ii.get("descriptionurl") in SEEN or not OK.match(lic.strip()) or ii["width"] < MINW or "nonfree" in md.get("Copyrighted", {}).get("value", "").lower(): continue
        rows.append(dict(title=pg["title"], url=ii.get("thumburl") or ii["url"], w=ii["width"], h=ii["height"], licence=lic,
                         author=re.sub("<[^>]+>", "", md.get("Artist", {}).get("value", "")).strip()[:80], page=ii["descriptionurl"]))
print(len(rows), "with an accepted licence and >= 1200 px")
os.makedirs(OUT, exist_ok=True); n = 0
with open(os.path.join(OUT, "attribution.csv"), "w", newline="") as fh:
    w = csv.writer(fh); w.writerow(["file", "licence", "author", "page"])
    for r in rows[:int(sys.argv[1]) if len(sys.argv) > 1 else 120]:
        name = f"c{n + 1:03d}_" + re.sub(r"[^A-Za-z0-9]+", "_", r["title"][5:])[:50] + ".jpg"
        try:
            data = get(r["url"], 90)
        except Exception as e:
            print("fail", r["title"], e); time.sleep(2); continue
        open(os.path.join(OUT, name), "wb").write(data); w.writerow([name, r["licence"], r["author"], r["page"]]); n += 1; time.sleep(3.0)
print("downloaded", n)
