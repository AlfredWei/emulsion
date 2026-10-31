"""Parallel ranged download (the COCO host caps each connection at ~100 KB/s). Usage: fetch_ranges.py URL DEST [connections=16]"""
import sys, os, urllib.request, concurrent.futures as cf, time
url, dest = sys.argv[1], sys.argv[2]
conns = int(sys.argv[3]) if len(sys.argv) > 3 else 16
size = int(urllib.request.urlopen(urllib.request.Request(url, method="HEAD")).headers["Content-Length"])
CH = 4 << 20
chunks = [(s, min(s + CH, size) - 1) for s in range(0, size, CH)]
with open(dest, "wb") as f: f.truncate(size)
done = [0]
def get(c):
    s, e = c
    for attempt in range(8):
        try:
            req = urllib.request.Request(url, headers={"Range": f"bytes={s}-{e}"})
            data = urllib.request.urlopen(req, timeout=60).read()
            if len(data) != e - s + 1: raise IOError("short read")
            with open(dest, "r+b") as f: f.seek(s); f.write(data)
            done[0] += len(data); return
        except Exception as ex:
            time.sleep(1 + attempt)
    raise RuntimeError(f"chunk {s} failed")
t = time.time()
with cf.ThreadPoolExecutor(conns) as ex:
    futs = [ex.submit(get, c) for c in chunks]
    for i, fu in enumerate(cf.as_completed(futs)):
        fu.result()
        if i % 10 == 0: print(f"{done[0]/1e6:.0f}/{size/1e6:.0f} MB {time.time()-t:.0f}s", flush=True)
print("done", size, f"{time.time()-t:.0f}s")
