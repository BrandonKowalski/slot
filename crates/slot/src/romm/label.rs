use slot_store::Platform;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgba<'a> {
    pub px: &'a [u8],
    pub w: usize,
    pub h: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Box4 {
    pub l: f32,
    pub t: f32,
    pub r: f32,
    pub b: f32,
}

impl Box4 {
    fn w(&self) -> f32 {
        self.r - self.l
    }

    fn h(&self) -> f32 {
        self.b - self.t
    }

    fn part(&self, f: [f32; 4]) -> Box4 {
        Box4 {
            l: self.l + self.w() * f[0],
            t: self.t + self.h() * f[1],
            r: self.l + self.w() * f[2],
            b: self.t + self.h() * f[3],
        }
    }

    fn inset(&self) -> Box4 {
        let (dx, dy) = (self.w() * 0.006, self.h() * 0.01);
        Box4 {
            l: self.l + dx,
            t: self.t + dy,
            r: self.r - dx,
            b: self.b - dy,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Found {
    WholeImage,
    Template,
    Detected,
    Standard,
}

fn label_box(platform: Platform) -> [f32; 4] {
    match platform.hardware() {
        Platform::Gba => [0.142, 0.228, 0.868, 0.863],
        _ => [0.10, 0.27, 0.90, 0.88],
    }
}

fn label_aspect(platform: Platform) -> f32 {
    match platform.hardware() {
        Platform::Gba => 1.94,
        _ => 1.12,
    }
}

impl Rgba<'_> {
    fn at(&self, x: usize, y: usize) -> [u8; 4] {
        let i = (y.min(self.h - 1) * self.w + x.min(self.w - 1)) * 4;
        [self.px[i], self.px[i + 1], self.px[i + 2], self.px[i + 3]]
    }
}

fn median(mut v: Vec<u8>) -> u8 {
    if v.is_empty() {
        return 0;
    }
    v.sort_unstable();
    v[v.len() / 2]
}

fn median_rgb(samples: &[[u8; 4]]) -> [u8; 3] {
    [0, 1, 2].map(|c| median(samples.iter().map(|s| s[c]).collect()))
}

fn max_diff(p: [u8; 4], c: [u8; 3]) -> u8 {
    (0..3).map(|i| p[i].abs_diff(c[i])).max().unwrap_or(0)
}

pub fn cartridge_bounds(img: Rgba) -> Option<Box4> {
    let see_through = img.px.chunks_exact(4).any(|p| p[3] < 250);
    let corners = [
        (0, 0),
        (img.w - 1, 0),
        (0, img.h - 1),
        (img.w - 1, img.h - 1),
    ]
    .map(|(x, y)| img.at(x, y));
    let ground = median_rgb(&corners);
    let inside = |p: [u8; 4]| match see_through {
        true => p[3] > 32,
        false => {
            let d = |i: usize| f32::from(p[i].abs_diff(ground[i]));
            0.299 * d(0) + 0.587 * d(1) + 0.114 * d(2) > 30.0
        }
    };
    let (mut l, mut t, mut r, mut b) = (usize::MAX, usize::MAX, 0, 0);
    for y in 0..img.h {
        for x in 0..img.w {
            if inside(img.at(x, y)) {
                (l, t, r, b) = (l.min(x), t.min(y), r.max(x + 1), b.max(y + 1));
            }
        }
    }
    (l < r && t < b).then_some(Box4 {
        l: l as f32,
        t: t as f32,
        r: r as f32,
        b: b as f32,
    })
}

pub fn find_box(img: Rgba, platform: Platform) -> (Box4, Found) {
    let whole = Box4 {
        l: 0.0,
        t: 0.0,
        r: img.w as f32,
        b: img.h as f32,
    };
    let Some(cart) = cartridge_bounds(img) else {
        return (whole, Found::WholeImage);
    };
    if cart.w() < 10.0 || cart.h() < 10.0 {
        return (whole, Found::WholeImage);
    }
    let aspect = cart.w() / cart.h();
    let want = label_aspect(platform);
    if (aspect / want - 1.0).abs() < 0.06 {
        return (cart, Found::WholeImage);
    }
    let rough = cart.part(label_box(platform));
    if platform.hardware() == Platform::Gba && (1.6..=1.78).contains(&aspect) {
        return (snap_to_edges(img, cart, rough).inset(), Found::Template);
    }
    match detect(img, cart, want) {
        Some(found) => (found.inset(), Found::Detected),
        None => (rough.inset(), Found::Standard),
    }
}

fn snap_to_edges(img: Rgba, cart: Box4, rough: Box4) -> Box4 {
    let (lw, lh) = (rough.w(), rough.h());
    let (y0, y1) = ((rough.t + lh * 0.3) as usize, (rough.t + lh * 0.7) as usize);
    let mut samples = Vec::new();
    for (from, to) in [(cart.l, rough.l), (rough.r, cart.r)] {
        let margin = to - from;
        for x in (from + margin * 0.25) as usize..(from + margin * 0.75) as usize {
            for y in y0..y1 {
                samples.push(img.at(x, y));
            }
        }
    }
    if samples.is_empty() {
        return rough;
    }
    let plastic = median_rgb(&samples);
    let plastic_share = |vertical: bool, at: usize| {
        let (from, to) = match vertical {
            true => ((rough.t + lh * 0.2) as usize, (rough.t + lh * 0.8) as usize),
            false => ((rough.l + lw * 0.2) as usize, (rough.l + lw * 0.8) as usize),
        };
        let n = to.saturating_sub(from).max(1);
        let hits = (from..to)
            .filter(|&i| {
                let p = if vertical {
                    img.at(at, i)
                } else {
                    img.at(i, at)
                };
                max_diff(p, plastic) < 24
            })
            .count();
        hits as f32 / n as f32
    };
    let walk = |start: f32, step: f32, limit: f32, vertical: bool| -> f32 {
        let mut at = start;
        let mut last_plastic = false;
        while (step > 0.0 && at < limit) || (step < 0.0 && at > limit) {
            let is_plastic = plastic_share(vertical, at.max(0.0) as usize) >= 0.85;
            if is_plastic && last_plastic {
                return at - step;
            }
            last_plastic = is_plastic;
            at += step;
        }
        limit
    };
    let (dx, dy) = (lw * 0.06, lh * 0.08);
    Box4 {
        l: walk(rough.l + dx, -1.0, (rough.l - dx).max(cart.l), true),
        r: walk(rough.r - dx, 1.0, (rough.r + dx).min(cart.r), true) + 1.0,
        t: walk(rough.t + dy, -1.0, (rough.t - dy).max(cart.t), false),
        b: walk(rough.b - dy, 1.0, (rough.b + dy).min(cart.b), false) + 1.0,
    }
}

fn longest_run(values: &[f32], at_least: f32) -> Option<(usize, usize)> {
    let mut best: Option<(usize, usize)> = None;
    let mut start = None;
    for (i, v) in values.iter().chain(std::iter::once(&-1.0)).enumerate() {
        match (*v >= at_least, start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                if best.is_none_or(|(a, b)| i - s > b - a) {
                    best = Some((s, i));
                }
                start = None;
            }
            _ => {}
        }
    }
    best
}

fn detect(img: Rgba, cart: Box4, want: f32) -> Option<Box4> {
    let scale = cart.w().max(cart.h()) / 300.0;
    let (sw, sh) = ((cart.w() / scale) as usize, (cart.h() / scale) as usize);
    if sw < 8 || sh < 8 {
        return None;
    }
    let sample = |x: usize, y: usize| {
        img.at(
            (cart.l + x as f32 * scale) as usize,
            (cart.t + y as f32 * scale) as usize,
        )
    };
    let span = |a: f32, b: f32, n: usize| (n as f32 * a) as usize..(n as f32 * b) as usize;
    let mut plastic = Vec::new();
    for y in span(0.35, 0.85, sh) {
        for x in span(0.015, 0.06, sw).chain(span(0.94, 0.985, sw)) {
            plastic.push(sample(x, y));
        }
    }
    let plastic = median_rgb(&plastic);
    let mask: Vec<bool> = (0..sh)
        .flat_map(|y| (0..sw).map(move |x| (x, y)))
        .map(|(x, y)| max_diff(sample(x, y), plastic) > 38)
        .collect();
    let inner = span(0.15, 0.85, sw);
    let rows: Vec<f32> = (0..sh)
        .map(|y| {
            inner.clone().filter(|&x| mask[y * sw + x]).count() as f32 / inner.len().max(1) as f32
        })
        .collect();
    let (top, bottom) = longest_run(&rows, 0.6)?;
    if ((bottom - top) as f32) < sh as f32 * 0.4 {
        return None;
    }
    let cols: Vec<f32> = (0..sw)
        .map(|x| (top..bottom).filter(|&y| mask[y * sw + x]).count() as f32 / (bottom - top) as f32)
        .collect();
    let (left, right) = longest_run(&cols, 0.6)?;
    let (w, h) = ((right - left) as f32, (bottom - top) as f32);
    if w < sw as f32 * 0.6 || (w / h / want - 1.0).abs() > 0.3 {
        return None;
    }
    Some(Box4 {
        l: cart.l + left as f32 * scale,
        t: cart.t + top as f32 * scale,
        r: cart.l + right as f32 * scale,
        b: cart.t + bottom as f32 * scale,
    })
}

pub fn crop(img: Rgba, b: Box4) -> (Vec<u8>, usize, usize) {
    let l = (b.l.max(0.0) as usize).min(img.w - 1);
    let t = (b.t.max(0.0) as usize).min(img.h - 1);
    let r = (b.r.ceil() as usize).clamp(l + 1, img.w);
    let bt = (b.b.ceil() as usize).clamp(t + 1, img.h);
    let (w, h) = (r - l, bt - t);
    let mut out = Vec::with_capacity(w * h * 4);
    for y in t..bt {
        out.extend_from_slice(&img.px[(y * img.w + l) * 4..(y * img.w + r) * 4]);
    }
    (out, w, h)
}
