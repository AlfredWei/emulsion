"""Read one member of a remote zip over HTTP Range requests (the central directory plus that member only). Usage: remote_member.py URL MEMBER DEST"""
import sys, io, zipfile, urllib.request
class Remote(io.RawIOBase):
    def __init__(self, url):
        self.url, self.pos = url, 0
        self.size = int(urllib.request.urlopen(urllib.request.Request(url, method="HEAD")).headers["Content-Length"])
    def seekable(self): return True
    def readable(self): return True
    def tell(self): return self.pos
    def seek(self, off, whence=0):
        self.pos = off if whence == 0 else self.pos + off if whence == 1 else self.size + off
        return self.pos
    def read(self, n=-1):
        if n < 0: n = self.size - self.pos
        n = min(n, self.size - self.pos)
        if n <= 0: return b""
        req = urllib.request.Request(self.url, headers={"Range": f"bytes={self.pos}-{self.pos + n - 1}"})
        data = urllib.request.urlopen(req, timeout=120).read(); self.pos += len(data); return data
    def readinto(self, b):
        d = self.read(len(b)); b[:len(d)] = d; return len(d)
url, member, dest = sys.argv[1:4]
z = zipfile.ZipFile(io.BufferedReader(Remote(url), 1 << 20))
info = z.getinfo(member); print(member, info.compress_size, "bytes compressed")
open(dest, "wb").write(z.read(member)); print("saved", dest)
