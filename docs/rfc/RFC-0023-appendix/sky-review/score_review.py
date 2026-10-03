"""Unblind review_answers.json with review_key.json: acceptable rate per model, overall and by category (Wilson 95% CI)."""
import json, math, collections
k = json.load(open("review_key.json")); a = json.load(open("review_answers.json"))
assert set(a) == set(k), set(k) ^ set(a)
def wilson(x, n, z=1.96):
    if n == 0: return (0, 0)
    p = x / n; d = 1 + z * z / n; c = p + z * z / (2 * n); h = z * math.sqrt(p * (1 - p) / n + z * z / (4 * n * n))
    return ((c - h) / d, (c + h) / d)
acc = {"ours": collections.Counter(), "skyseg": collections.Counter()}; tot = collections.Counter()
rows = []
for i, m in k.items():
    ans = a[i]; ok = {"A": [m["A"]], "B": [m["B"]], "both": ["ours", "skyseg"], "none": []}[ans]
    tot[m["category"]] += 1; tot["ALL"] += 1
    for mod in ok: acc[mod][m["category"]] += 1; acc[mod]["ALL"] += 1
    rows.append((int(i), m["category"], ans, ok))
print("photo cat answer -> acceptable")
for r in sorted(rows): print(*r)
print()
for mod in acc:
    x, n = acc[mod]["ALL"], tot["ALL"]; lo, hi = wilson(x, n)
    print(f"{mod:7s} acceptable {x}/{n} = {x/n:.0%}  (95% CI {lo:.0%}-{hi:.0%})")
print("\nby category (ours / skyseg acceptable, of n):")
for c in dict.fromkeys(m["category"] for m in k.values()):
    print(f"  {c:15s} {acc['ours'][c]} / {acc['skyseg'][c]} of {tot[c]}")
both = sum(1 for r in rows if r[2] == "both"); none = sum(1 for r in rows if r[2] == "none")
print(f"\nboth acceptable {both}, neither {none}, only ours {sum(1 for r in rows if r[3]==['ours'])}, only skyseg {sum(1 for r in rows if r[3]==['skyseg'])}")
