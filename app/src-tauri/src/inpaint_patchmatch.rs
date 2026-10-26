//! Content-aware fill without a model (RFC-0032): multi-scale patch-based inpainting after Wexler et al. 2007, with the
//! nearest-patch search of PatchMatch (Barnes et al. 2009). The hole is filled at the coarsest scale by diffusion, then
//! at each finer scale the picture is refined by alternating "find, for every patch that touches the hole, the most
//! similar patch made only of known pixels" and "set each hole pixel to the weighted mean of what those patches say".
//!
//! Pure, deterministic for a given seed, no dependencies beyond `image`. It is the non-model baseline of the bake-off
//! and the fallback the RFC allows if no model clearly beats it.

use image::{GrayImage, Rgb, RgbImage};

#[derive(Debug, Clone, Copy)]
pub struct Params {
    /// Patches are (2 r + 1) squared.
    pub patch_radius: usize,
    /// The finest level is at most this many pixels on its long edge (a bigger picture is reduced, filled and
    /// enlarged back, and only the hole takes the result).
    pub max_side: u32,
    /// Refinement rounds at the coarse levels (the finest level uses fewer).
    pub iterations: usize,
    pub seed: u64,
}

impl Default for Params {
    fn default() -> Self {
        Self { patch_radius: 3, max_side: 1536, iterations: 5, seed: 1 }
    }
}

type Px = [f32; 3];

struct Level {
    w: usize,
    h: usize,
    rgb: Vec<Px>,
    hole: Vec<bool>,
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    /// Uniform in `0..n` (n > 0).
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// Fills the pixels of `hole` (any non-zero value) from the rest of `img`. Pixels outside the hole are returned unchanged.
pub fn inpaint(img: &RgbImage, hole: &GrayImage, params: &Params) -> RgbImage {
    assert_eq!(img.dimensions(), hole.dimensions(), "the mask must be as big as the picture");
    let (w0, h0) = img.dimensions();
    if hole.pixels().all(|p| p[0] == 0) {
        return img.clone();
    }
    // Work on a reduced copy when the picture is bigger than `max_side`.
    let long = w0.max(h0);
    let scale = if long > params.max_side { params.max_side as f32 / long as f32 } else { 1.0 };
    let (ww, wh) = (((w0 as f32 * scale).round() as usize).max(8), ((h0 as f32 * scale).round() as usize).max(8));
    let base = level_from(img, hole, ww, wh);

    let filled = fill(base, params);

    // Back to the input size: enlarge, and let only the hole take the result.
    let mut out = img.clone();
    for y in 0..h0 as usize {
        for x in 0..w0 as usize {
            if hole.get_pixel(x as u32, y as u32)[0] == 0 {
                continue;
            }
            let p = sample_bilinear(&filled, (x as f32 + 0.5) * ww as f32 / w0 as f32 - 0.5, (y as f32 + 0.5) * wh as f32 / h0 as f32 - 0.5, ww, wh);
            out.put_pixel(x as u32, y as u32, Rgb([p[0].round().clamp(0.0, 255.0) as u8, p[1].round().clamp(0.0, 255.0) as u8, p[2].round().clamp(0.0, 255.0) as u8]));
        }
    }
    out
}

/// Box-resamples the picture and mask to `w x h` (area average of the known pixels; a pixel is hole when any source
/// pixel under it is).
fn level_from(img: &RgbImage, hole: &GrayImage, w: usize, h: usize) -> Level {
    let (sw, sh) = img.dimensions();
    let mut rgb = vec![[0.0; 3]; w * h];
    let mut hl = vec![false; w * h];
    for y in 0..h {
        let (y0, y1) = (y * sh as usize / h, ((y + 1) * sh as usize / h).max(y * sh as usize / h + 1).min(sh as usize));
        for x in 0..w {
            let (x0, x1) = (x * sw as usize / w, ((x + 1) * sw as usize / w).max(x * sw as usize / w + 1).min(sw as usize));
            let (mut sum, mut n, mut any_hole) = ([0.0f32; 3], 0.0f32, false);
            for yy in y0..y1 {
                for xx in x0..x1 {
                    if hole.get_pixel(xx as u32, yy as u32)[0] != 0 {
                        any_hole = true;
                    } else {
                        let p = img.get_pixel(xx as u32, yy as u32).0;
                        sum[0] += p[0] as f32;
                        sum[1] += p[1] as f32;
                        sum[2] += p[2] as f32;
                        n += 1.0;
                    }
                }
            }
            hl[y * w + x] = any_hole;
            if n > 0.0 {
                rgb[y * w + x] = [sum[0] / n, sum[1] / n, sum[2] / n];
            }
        }
    }
    Level { w, h, rgb, hole: hl }
}

fn halve(l: &Level) -> Level {
    let (w, h) = ((l.w / 2).max(1), (l.h / 2).max(1));
    let mut rgb = vec![[0.0; 3]; w * h];
    let mut hole = vec![false; w * h];
    for y in 0..h {
        for x in 0..w {
            let (mut sum, mut n, mut any_hole) = ([0.0f32; 3], 0.0f32, false);
            for yy in (2 * y)..(2 * y + 2).min(l.h) {
                for xx in (2 * x)..(2 * x + 2).min(l.w) {
                    let i = yy * l.w + xx;
                    if l.hole[i] {
                        any_hole = true;
                    } else {
                        for (acc, v) in sum.iter_mut().zip(l.rgb[i]) {
                            *acc += v;
                        }
                        n += 1.0;
                    }
                }
            }
            hole[y * w + x] = any_hole;
            if n > 0.0 {
                rgb[y * w + x] = [sum[0] / n, sum[1] / n, sum[2] / n];
            }
        }
    }
    Level { w, h, rgb, hole }
}

fn sample_bilinear(px: &[Px], x: f32, y: f32, w: usize, h: usize) -> Px {
    let (x, y) = (x.clamp(0.0, w as f32 - 1.0), y.clamp(0.0, h as f32 - 1.0));
    let (x0, y0) = (x.floor() as usize, y.floor() as usize);
    let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let mut out = [0.0; 3];
    for c in 0..3 {
        let top = px[y0 * w + x0][c] * (1.0 - fx) + px[y0 * w + x1][c] * fx;
        let bot = px[y1 * w + x0][c] * (1.0 - fx) + px[y1 * w + x1][c] * fx;
        out[c] = top * (1.0 - fy) + bot * fy;
    }
    out
}

/// Onion-peel diffusion: each pass gives the hole pixels that touch a known one the mean of their known neighbours.
fn diffuse(l: &mut Level) {
    let mut known: Vec<bool> = l.hole.iter().map(|h| !h).collect();
    if !known.iter().any(|&k| k) {
        return;
    }
    loop {
        let mut changed = Vec::new();
        for y in 0..l.h {
            for x in 0..l.w {
                let i = y * l.w + x;
                if known[i] {
                    continue;
                }
                let (mut sum, mut n) = ([0.0f32; 3], 0.0f32);
                for (dx, dy) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)] {
                    let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                    if nx < 0 || ny < 0 || nx >= l.w as i32 || ny >= l.h as i32 {
                        continue;
                    }
                    let j = ny as usize * l.w + nx as usize;
                    if known[j] {
                        for (acc, v) in sum.iter_mut().zip(l.rgb[j]) {
                            *acc += v;
                        }
                        n += 1.0;
                    }
                }
                if n > 0.0 {
                    changed.push((i, [sum[0] / n, sum[1] / n, sum[2] / n]));
                }
            }
        }
        if changed.is_empty() {
            break;
        }
        for (i, v) in changed {
            l.rgb[i] = v;
            known[i] = true;
        }
    }
}

/// The finest level's estimate of the hole, the rest of the level as it was.
fn fill(base: Level, params: &Params) -> Vec<Px> {
    let r = params.patch_radius;
    // Levels, finest first, down while the hole is still bigger than a couple of patches and the picture is not tiny.
    let mut levels = vec![base];
    loop {
        let top = levels.last().unwrap();
        let (bw, bh) = hole_extent(top);
        if top.w.min(top.h) < 64 || bw.min(bh) <= 4 * r {
            break;
        }
        let next = halve(top);
        levels.push(next);
    }
    let mut rng = Rng(params.seed.max(1));
    let coarsest = levels.len() - 1;
    let mut current: Vec<Px> = Vec::new();
    for li in (0..levels.len()).rev() {
        let level = &levels[li];
        // Start: the level's own known pixels, with the hole from the level above (enlarged) or by diffusion.
        let mut cur = level.rgb.clone();
        if li == coarsest {
            let mut tmp = Level { w: level.w, h: level.h, rgb: cur.clone(), hole: level.hole.clone() };
            diffuse(&mut tmp);
            cur = tmp.rgb;
        } else {
            let up = &levels[li + 1];
            for y in 0..level.h {
                for x in 0..level.w {
                    let i = y * level.w + x;
                    if level.hole[i] {
                        cur[i] = sample_bilinear(&current, (x as f32 + 0.5) * up.w as f32 / level.w as f32 - 0.5, (y as f32 + 0.5) * up.h as f32 / level.h as f32 - 0.5, up.w, up.h);
                    }
                }
            }
        }
        let iters = if li == 0 { (params.iterations / 2).max(2) } else { params.iterations };
        refine(level, &mut cur, r, iters, li == 0, &mut rng);
        current = cur;
    }
    // Known pixels exactly as given.
    let l0 = &levels[0];
    for ((out, known), &hole) in current.iter_mut().zip(&l0.rgb).zip(&l0.hole) {
        if !hole {
            *out = *known;
        }
    }
    current
}

fn hole_extent(l: &Level) -> (usize, usize) {
    let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
    for y in 0..l.h {
        for x in 0..l.w {
            if l.hole[y * l.w + x] {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
    }
    if x0 == usize::MAX {
        (0, 0)
    } else {
        (x1 - x0 + 1, y1 - y0 + 1)
    }
}

/// A few rounds of: nearest known patch for every patch touching the hole (PatchMatch), then the weighted vote.
fn refine(level: &Level, cur: &mut [Px], r: usize, iters: usize, sharp_last: bool, rng: &mut Rng) {
    let (w, h) = (level.w, level.h);
    if w < 2 * r + 1 || h < 2 * r + 1 {
        return;
    }
    // Integral image of hole pixels: a source centre is valid when its whole patch is inside the picture and known.
    let mut integral = vec![0u32; (w + 1) * (h + 1)];
    for y in 0..h {
        let mut row = 0;
        for x in 0..w {
            row += level.hole[y * w + x] as u32;
            integral[(y + 1) * (w + 1) + x + 1] = integral[y * (w + 1) + x + 1] + row;
        }
    }
    let side = 2 * r + 1;
    let mut valid = vec![false; w * h];
    let mut sources = Vec::new();
    for y in r..h - r {
        for x in r..w - r {
            let (x0, y0, x1, y1) = (x - r, y - r, x + r + 1, y + r + 1);
            let n = integral[y1 * (w + 1) + x1] + integral[y0 * (w + 1) + x0] - integral[y0 * (w + 1) + x1] - integral[y1 * (w + 1) + x0];
            if n == 0 {
                valid[y * w + x] = true;
                sources.push((x, y));
            }
        }
    }
    if sources.is_empty() {
        return; // nothing known to copy from: the diffused estimate stands
    }
    // Target centres: every pixel whose patch touches the hole (centres inside the picture only).
    let mut targets = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let (x0, y0) = (x.saturating_sub(r), y.saturating_sub(r));
            let (x1, y1) = ((x + r + 1).min(w), (y + r + 1).min(h));
            let n = integral[y1 * (w + 1) + x1] + integral[y0 * (w + 1) + x0] - integral[y0 * (w + 1) + x1] - integral[y1 * (w + 1) + x0];
            if n > 0 {
                targets.push((x, y));
            }
        }
    }
    let mut is_target = vec![false; w * h];
    for &(x, y) in &targets {
        is_target[y * w + x] = true;
    }
    let mut nnf: Vec<(u32, u32)> = vec![(0, 0); w * h];
    for &(x, y) in &targets {
        let s = sources[rng.below(sources.len())];
        nnf[y * w + x] = (s.0 as u32, s.1 as u32);
    }
    let known = &level.rgb; // source patches are all known, so the level's own values are the truth
    let clamp = |v: i32, hi: usize| v.clamp(0, hi as i32 - 1) as usize;
    // Distance between the target patch at (px, py) (current estimate) and the source patch at (qx, qy).
    let dist = |cur: &[Px], px: usize, py: usize, qx: usize, qy: usize, limit: f32| -> f32 {
        let mut d = 0.0;
        for dy in -(r as i32)..=r as i32 {
            let ty = clamp(py as i32 + dy, h);
            let sy = qy as i32 + dy;
            for dx in -(r as i32)..=r as i32 {
                let t = cur[ty * w + clamp(px as i32 + dx, w)];
                let s = known[sy as usize * w + (qx as i32 + dx) as usize];
                let (a, b, c) = (t[0] - s[0], t[1] - s[1], t[2] - s[2]);
                d += a * a + b * b + c * c;
            }
            if d > limit {
                return d;
            }
        }
        d
    };

    for it in 0..iters {
        // Distances for the current NNF.
        let mut best: Vec<f32> = vec![f32::MAX; w * h];
        for &(x, y) in &targets {
            let (qx, qy) = nnf[y * w + x];
            best[y * w + x] = dist(cur, x, y, qx as usize, qy as usize, f32::MAX);
        }
        // Propagation and random search, alternating scan direction.
        let forward = it % 2 == 0;
        let order: Vec<(usize, usize)> = if forward { targets.clone() } else { targets.iter().rev().cloned().collect() };
        let step: i32 = if forward { 1 } else { -1 };
        for &(x, y) in &order {
            let i = y * w + x;
            let try_cand = |qx: i32, qy: i32, nnf: &mut Vec<(u32, u32)>, best: &mut Vec<f32>| {
                if qx < 0 || qy < 0 || qx >= w as i32 || qy >= h as i32 {
                    return;
                }
                let q = qy as usize * w + qx as usize;
                if !valid[q] {
                    return;
                }
                let d = dist(cur, x, y, qx as usize, qy as usize, best[i]);
                if d < best[i] {
                    best[i] = d;
                    nnf[i] = (qx as u32, qy as u32);
                }
            };
            // from the neighbour before, in scan order
            for (nx, ny) in [(x as i32 - step, y as i32), (x as i32, y as i32 - step)] {
                if nx >= 0 && ny >= 0 && (nx as usize) < w && (ny as usize) < h && is_target[ny as usize * w + nx as usize] {
                    let (qx, qy) = nnf[ny as usize * w + nx as usize];
                    let (dx, dy) = (x as i32 - nx, y as i32 - ny);
                    try_cand(qx as i32 + dx, qy as i32 + dy, &mut nnf, &mut best);
                }
            }
            // random search around the current best, the radius halving each time
            let (bx, by) = nnf[i];
            let mut radius = w.max(h) as i32 / 2;
            while radius >= 1 {
                let qx = bx as i32 + rng.below(2 * radius as usize + 1) as i32 - radius;
                let qy = by as i32 + rng.below(2 * radius as usize + 1) as i32 - radius;
                try_cand(qx, qy, &mut nnf, &mut best);
                radius /= 2;
            }
        }
        // Last round of the finest level: each hole pixel copies the centre of its own best patch. A mean of many
        // patches is a blur; copying keeps the source's grain and edges (the seams between copied runs are the price).
        if sharp_last && it + 1 == iters {
            for &(x, y) in &targets {
                let i = y * w + x;
                if level.hole[i] {
                    let (qx, qy) = nnf[i];
                    cur[i] = known[qy as usize * w + qx as usize];
                }
            }
            break;
        }
        // Vote: every hole pixel takes the weighted mean of what the patches covering it say.
        let patch_n = (side * side * 3) as f32;
        let mut acc = vec![[0.0f32; 4]; w * h];
        for &(x, y) in &targets {
            let i = y * w + x;
            let (qx, qy) = nnf[i];
            let wt = (-(best[i] / patch_n) / (2.0 * 20.0 * 20.0)).exp();
            for dy in -(r as i32)..=r as i32 {
                for dx in -(r as i32)..=r as i32 {
                    let (tx, ty) = (x as i32 + dx, y as i32 + dy);
                    if tx < 0 || ty < 0 || tx >= w as i32 || ty >= h as i32 {
                        continue;
                    }
                    let t = ty as usize * w + tx as usize;
                    if !level.hole[t] {
                        continue;
                    }
                    let s = known[(qy as i32 + dy) as usize * w + (qx as i32 + dx) as usize];
                    let a = &mut acc[t];
                    a[0] += s[0] * wt;
                    a[1] += s[1] * wt;
                    a[2] += s[2] * wt;
                    a[3] += wt;
                }
            }
        }
        for i in 0..w * h {
            if level.hole[i] && acc[i][3] > 0.0 {
                cur[i] = [acc[i][0] / acc[i][3], acc[i][1] / acc[i][3], acc[i][2] / acc[i][3]];
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Luma;

    /// A picture made of 8 px blocks of deterministic pseudo-random colours: the same block pattern repeats, so a hole
    /// inside it has a perfect source elsewhere.
    fn tiles(w: u32, h: u32) -> RgbImage {
        RgbImage::from_fn(w, h, |x, y| {
            let (bx, by) = ((x / 8) % 4, (y / 8) % 4);
            let v = (bx * 4 + by) * 15 + 20;
            Rgb([v as u8, (255 - v) as u8, ((v * 3) % 255) as u8])
        })
    }

    fn square_hole(w: u32, h: u32, x0: u32, y0: u32, s: u32) -> GrayImage {
        GrayImage::from_fn(w, h, |x, y| Luma([if x >= x0 && x < x0 + s && y >= y0 && y < y0 + s { 255 } else { 0 }]))
    }

    fn mean_abs_error(a: &RgbImage, b: &RgbImage, hole: &GrayImage) -> f32 {
        let (mut sum, mut n) = (0.0, 0.0);
        for (x, y, m) in hole.enumerate_pixels() {
            if m[0] != 0 {
                for c in 0..3 {
                    sum += (a.get_pixel(x, y)[c] as f32 - b.get_pixel(x, y)[c] as f32).abs();
                }
                n += 3.0;
            }
        }
        sum / n
    }

    #[test]
    fn a_hole_in_a_repeating_pattern_is_filled_with_the_pattern() {
        let truth = tiles(128, 128);
        let hole = square_hole(128, 128, 50, 50, 28);
        let mut damaged = truth.clone();
        for (x, y, m) in hole.enumerate_pixels() {
            if m[0] != 0 {
                damaged.put_pixel(x, y, Rgb([128, 128, 128]));
            }
        }
        let out = inpaint(&damaged, &hole, &Params::default());
        let err = mean_abs_error(&out, &truth, &hole);
        assert!(err < 25.0, "mean error {err} in the hole (a flat grey fill would be about 60)");
    }

    #[test]
    fn pixels_outside_the_hole_are_untouched() {
        let truth = tiles(96, 96);
        let hole = square_hole(96, 96, 30, 30, 20);
        let out = inpaint(&truth, &hole, &Params::default());
        for (x, y, m) in hole.enumerate_pixels() {
            if m[0] == 0 {
                assert_eq!(out.get_pixel(x, y), truth.get_pixel(x, y));
            }
        }
    }

    #[test]
    fn an_empty_mask_returns_the_picture() {
        let img = tiles(64, 64);
        assert_eq!(inpaint(&img, &GrayImage::new(64, 64), &Params::default()), img);
    }

    #[test]
    fn the_same_seed_gives_the_same_picture() {
        let img = tiles(96, 96);
        let hole = square_hole(96, 96, 30, 30, 24);
        assert_eq!(inpaint(&img, &hole, &Params::default()), inpaint(&img, &hole, &Params::default()));
    }

    #[test]
    fn holes_at_the_border_and_a_fully_masked_picture_do_not_panic() {
        let img = tiles(80, 80);
        let corner = square_hole(80, 80, 0, 0, 25);
        let out = inpaint(&img, &corner, &Params::default());
        assert_eq!(out.dimensions(), (80, 80));
        let all = GrayImage::from_pixel(80, 80, Luma([255]));
        assert_eq!(inpaint(&img, &all, &Params::default()).dimensions(), (80, 80));
        let edge = square_hole(80, 80, 60, 30, 20);
        assert_eq!(inpaint(&img, &edge, &Params::default()).dimensions(), (80, 80));
    }

    #[test]
    fn a_big_picture_is_worked_on_reduced_and_comes_back_full_size() {
        let img = tiles(400, 300);
        let hole = square_hole(400, 300, 150, 100, 60);
        let p = Params { max_side: 128, ..Params::default() };
        let out = inpaint(&img, &hole, &p);
        assert_eq!(out.dimensions(), (400, 300));
        for (x, y, m) in hole.enumerate_pixels() {
            if m[0] == 0 {
                assert_eq!(out.get_pixel(x, y), img.get_pixel(x, y));
            }
        }
    }
}
