import re, sys
exe = open(sys.argv[1], "rb").read()
mp = open(sys.argv[2], errors="replace").read()
libs = ["raw_r", "zlib", "lcms2", "jasper", "freeglut", "jpeg", "turbojpeg"]
print("== linker map: lines naming each static library (Lib:Object column)")
for lib in libs:
    hits = [l for l in mp.splitlines() if re.search(r"\b" + lib + r"\.lib:", l)]
    print(f"{lib}.lib: {len(hits)} map lines")
    for l in hits[:3]:
        print("   ", l.strip()[:200])
syms = ["libraw_init", "cmsOpenProfileFromMem", "jpeg_CreateDecompress", "tjInitDecompress", "jas_init", "deflate", "inflate", "glutInit", "jas_stream_memopen", "cmsCreateTransform"]
print("== linker map: symbol names")
for s in syms:
    hits = [l for l in mp.splitlines() if re.search(r"(?<![A-Za-z0-9_])_?" + s + r"(?![A-Za-z0-9_])", l)]
    print(s, len(hits), (hits[0].strip()[:200] if hits else ""))
print("== exe strings")
for s in ["libraw_init", "cmsOpenProfileFromMem", "jpeg_CreateDecompress", "tjInitDecompress", "jas_init", "glutInit", "inflate 1.3.1", "deflate 1.3.1", "Mark Adler", "freeglut", "JasPer", "Little CMS", "LibRaw", "libjpeg-turbo", "jasper"]:
    print(repr(s), exe.count(s.encode()), "ascii;", exe.count(s.encode("utf-16-le")), "utf16")
