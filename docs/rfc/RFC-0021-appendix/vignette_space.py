#!/usr/bin/env python3
"""RFC-0021 appendix: what multiplying a vignetting gain in gamma-encoded
sRGB (what `apply_lens_correction` does today) does compared with multiplying
it in linear light (what lensfun's own manual says the model requires).
Pure python, no dependencies:   python3 vignette_space.py
"""


def lin(c):
    c /= 255
    return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4


def enc(l):
    l = min(max(l, 0), 1)
    v = l * 12.92 if l <= 0.0031308 else 1.055 * l ** (1 / 2.4) - 0.055
    return round(v * 255)


print("gain  input  encoded-space(today)  linear-light  today-minus-linear")
for g in (1.25, 1.5, 2.0, 4.1):
    for c in (32, 64, 128, 192):
        e = min(255, round(c * g))
        l = enc(lin(c) * g)
        print(f"{g:4}  {c:5}  {e:20}  {l:12}  {e - l:+d}")
