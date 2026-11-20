"""Generate the topographic contour pattern used as a background motif
(ui/packages/kit/src/topo.svg). Original artwork: contour lines traced over a
smooth, tileable noise field with marching squares.

    python tools/topo/generate.py > ui/packages/kit/src/topo.svg
"""
import math
import random

W = H = 640          # tile size (px)
GRID = 4             # sampling step
LEVELS = 14          # number of contour levels
SEED = 7

random.seed(SEED)

# Tileable value noise: sum of periodic sine "hills" (wrap at W/H).
waves = []
for _ in range(9):
    kx = random.choice([1, 2, 3])
    ky = random.choice([1, 2, 3])
    waves.append((kx, ky, random.uniform(0, 2 * math.pi), random.uniform(0, 2 * math.pi), random.uniform(0.4, 1.0)))


def field(x, y):
    v = 0.0
    for kx, ky, px, py, a in waves:
        v += a * math.sin(2 * math.pi * kx * x / W + px) * math.cos(2 * math.pi * ky * y / H + py)
    return v


nx, ny = W // GRID + 1, H // GRID + 1
vals = [[field(i * GRID, j * GRID) for i in range(nx)] for j in range(ny)]
lo = min(min(r) for r in vals)
hi = max(max(r) for r in vals)


def interp(p1, p2, v1, v2, t):
    f = 0.5 if v1 == v2 else (t - v1) / (v2 - v1)
    return (p1[0] + f * (p2[0] - p1[0]), p1[1] + f * (p2[1] - p1[1]))


paths = []
for li in range(1, LEVELS):
    t = lo + (hi - lo) * li / LEVELS
    segs = []
    for j in range(ny - 1):
        for i in range(nx - 1):
            x, y = i * GRID, j * GRID
            c = [(x, y), (x + GRID, y), (x + GRID, y + GRID), (x, y + GRID)]
            v = [vals[j][i], vals[j][i + 1], vals[j + 1][i + 1], vals[j + 1][i]]
            idx = sum(1 << k for k in range(4) if v[k] > t)
            if idx in (0, 15):
                continue
            edges = []
            for k in range(4):
                a, b = k, (k + 1) % 4
                if (v[a] > t) != (v[b] > t):
                    edges.append(interp(c[a], c[b], v[a], v[b], t))
            if len(edges) == 2:
                segs.append((edges[0], edges[1]))
            elif len(edges) == 4:
                segs.append((edges[0], edges[1]))
                segs.append((edges[2], edges[3]))
    # Join segments into polylines.
    key = lambda p: (round(p[0], 2), round(p[1], 2))
    adj = {}
    for s in segs:
        adj.setdefault(key(s[0]), []).append(s)
        adj.setdefault(key(s[1]), []).append(s)
    used = set()
    for s in segs:
        if id(s) in used:
            continue
        used.add(id(s))
        line = [s[0], s[1]]
        for end in (1, 0):
            while True:
                tip = line[-1] if end else line[0]
                nxt = None
                for cand in adj.get(key(tip), []):
                    if id(cand) not in used:
                        nxt = cand
                        break
                if not nxt:
                    break
                used.add(id(nxt))
                p = nxt[1] if key(nxt[0]) == key(tip) else nxt[0]
                if end:
                    line.append(p)
                else:
                    line.insert(0, p)
        if len(line) < 6:
            continue
        # Light simplification: every second point.
        pts = line[::2] + ([line[-1]] if len(line) % 2 == 0 else [])
        d = "M" + " L".join(f"{p[0]:.1f},{p[1]:.1f}" for p in pts)
        major = li % 4 == 0
        paths.append((d, major))

out = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" viewBox="0 0 {W} {H}" fill="none" stroke="#000" stroke-linecap="round" stroke-linejoin="round">']
for d, major in paths:
    out.append(f'<path d="{d}" stroke-width="{1.4 if major else 0.8}"/>')
out.append("</svg>")
print("\n".join(out))
