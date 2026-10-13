//! Colour negative film conversion ("Negative Lab").
//!
//! A scan or camera copy of a colour negative records the film's transmittance `T` per channel.
//! The conversion works in density, where film is linear:
//!
//! 1. **Base**: `Dn = log10(T_base / T)` — density above the film base. Dividing by the base
//!    (orange mask + the light source) removes both, whatever scanner or light was used.
//! 2. **Separation**: `A = M⁻¹ · Dn` undoes the dyes' crosstalk ([`Emulsion::crosstalk`]),
//!    scaled by [`Params::separation`].
//! 3. **Inversion**: each layer's density goes back to log exposure, `x = a·A + b`, either through
//!    the emulsion's own slopes and grey densities ([`Params::neutralize`] = 0) or matched so the
//!    image's thinnest and densest areas come out neutral, as a lab scanner does (1). Linear in
//!    density, so the film's toe and shoulder stay in the image as in a print.
//! 4. **Light**: `E = 0.18 · 10^(x + offset)`, scene-linear, with the offset putting the image's
//!    median at a standard key; then the emulsion's rendition saturation.
//!
//! The image statistics the conversion needs (film base, thin/dense/median densities) come from
//! [`analyze`] and are stored with the edit ([`Analysis`]), so previews, crops and exports of the
//! same photo convert identically. [`calibrate`] fits a profile from a photographed colour chart.
//!
//! Densities are taken in the scan's own channels: a scanner or camera file reaches the pipeline
//! converted to its working space (linear Rec.2020), which mixes the channels; the `*_working`
//! entry points undo that ([`to_scan`], linear Rec.709 / sRGB primaries, which is how scanner
//! files are tagged) before measuring and converting, and return the positive in Rec.2020.
//! [`analyze`] and [`Converter::convert`] work in scan channels directly.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod calibrate;
pub mod emulsions;

pub use calibrate::{Calibration, CalibrationError, calibrate};
pub use emulsions::{EMULSIONS, Emulsion};

use serde::{Deserialize, Serialize};

/// Linear Rec.2020 → linear Rec.709 (sRGB primaries, D65).
const WORKING_TO_SCAN: [[f32; 3]; 3] =
    [[1.660_491, -0.587_641, -0.072_850], [-0.124_550, 1.132_900, -0.008_349], [-0.018_151, -0.100_579, 1.118_730]];
/// Linear Rec.709 → linear Rec.2020.
const SCAN_TO_WORKING: [[f32; 3]; 3] = [[0.627_404, 0.329_283, 0.043_313], [0.069_097, 0.919_540, 0.011_362], [0.016_391, 0.088_013, 0.895_595]];

/// A working-space (linear Rec.2020) pixel in scan channels (linear Rec.709).
#[inline]
pub fn to_scan(p: [f32; 3]) -> [f32; 3] {
    mul(&WORKING_TO_SCAN, &p)
}

/// A scan-channel pixel back in the working space.
#[inline]
pub fn from_scan(p: [f32; 3]) -> [f32; 3] {
    mul(&SCAN_TO_WORKING, &p)
}

/// Smallest transmittance read (a fully opaque pixel would be infinite density).
const T_MIN: f32 = 1e-5;
/// Scene-linear value an 18 % grey card maps to.
const GREY: f32 = 0.18;
/// Where the image's median lands (a little under grey: most scenes are darker than a grey card).
const KEY: f32 = 0.15;
/// Rec.2020 luminance weights.
const LUMA: [f32; 3] = [0.2627, 0.6780, 0.0593];
/// Most pixels [`analyze`] reads.
const MAX_SAMPLES: usize = 1 << 17;
/// Bands (fractions of pixels ranked by density) averaged for the thin (shadow) and dense
/// (highlight) ends; the outermost 0.1 % (dust, specular glints) is left out.
const LO_BAND: (f32, f32) = (0.001, 0.015);
const HI_BAND: (f32, f32) = (0.985, 0.999);

/// The image statistics a conversion is anchored to, in density above [`Analysis::base`].
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Analysis {
    /// Linear transmittance of the film base (unexposed film), per channel.
    pub base: [f32; 3],
    /// Thin end (deep shadows) of each channel's density.
    pub lo: [f32; 3],
    /// Dense end (highlights) of each channel's density.
    pub hi: [f32; 3],
    /// Median density per channel.
    pub mid: [f32; 3],
}

impl Analysis {
    /// The same statistics against another film base: densities shift by `log10(new / old)`.
    pub fn with_base(&self, base: [f32; 3]) -> Analysis {
        let mut a = *self;
        for c in 0..3 {
            let b = base[c].max(T_MIN);
            let d = (b / self.base[c].max(T_MIN)).log10();
            a.base[c] = b;
            a.lo[c] += d;
            a.hi[c] += d;
            a.mid[c] += d;
        }
        a
    }

    /// Every value finite and the base positive (a damaged settings file can hold anything).
    pub fn is_valid(&self) -> bool {
        let fin = |v: &[f32; 3]| v.iter().all(|x| x.is_finite());
        fin(&self.base) && self.base.iter().all(|&b| b > 0.0) && fin(&self.lo) && fin(&self.hi) && fin(&self.mid)
    }
}

/// A measured profile: log exposure as an affine function of base-relative density,
/// `x = matrix · Dn + offset` (see [`calibrate`]). Replaces the emulsion model when set.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Measured {
    pub matrix: [[f32; 3]; 3],
    pub offset: [f32; 3],
}

/// What the user chose for one photo.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Params<'a> {
    pub emulsion: &'a Emulsion,
    /// 0 = trust the emulsion profile's colour balance, 1 = neutral thin and dense ends (lab scanner).
    pub neutralize: f32,
    /// Crosstalk correction strength: 0 = none, 1 = the profile's, up to 2.
    pub separation: f32,
    /// Keep the measured exposure (calibrated profiles) instead of re-keying the median.
    pub auto_exposure: bool,
    pub measured: Option<&'a Measured>,
}

/// A conversion ready to run per pixel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Converter {
    /// `x = m · Dn + q` (separation, inversion and exposure folded together).
    m: [[f32; 3]; 3],
    q: [f32; 3],
    base: [f32; 3],
    saturation: f32,
}

impl Converter {
    /// The conversion of an image with statistics `a` under `p`.
    pub fn new(a: &Analysis, p: &Params<'_>) -> Converter {
        let base = a.base.map(|b| if b.is_finite() { b.max(T_MIN) } else { 1.0 });
        if let Some(meas) = p.measured {
            let mut q = meas.offset;
            if p.auto_exposure {
                let shift = key_shift(&meas.matrix, &q, &a.mid);
                q = q.map(|v| v + shift);
            }
            return Converter { m: meas.matrix, q, base, saturation: 1.0 };
        }
        let e = p.emulsion;
        let s = finite_or(p.separation, 1.0).clamp(0.0, 2.0);
        let k = finite_or(p.neutralize, 1.0).clamp(0.0, 1.0);
        let mut mix = [[0f32; 3]; 3];
        for (r, row) in mix.iter_mut().enumerate() {
            for (c, v) in row.iter_mut().enumerate() {
                let id = if r == c { 1.0 } else { 0.0 };
                *v = id + s * (e.crosstalk[r][c] - id);
            }
        }
        let minv = invert(&mix).unwrap_or(IDENTITY);
        let (lo, hi) = (mul(&minv, &a.lo), mul(&minv, &a.hi));
        let gamma = e.gamma.map(|g| g.max(0.05));
        // the emulsion's own inversion: x = (A − grey) / gamma
        let own: [(f32, f32); 3] = std::array::from_fn(|c| (1.0 / gamma[c], -e.grey[c] / gamma[c]));
        // lab-neutral: every channel's thin and dense ends land where green's do
        let g = own[1];
        let (xlo, xhi) = (g.0 * lo[1] + g.1, g.0 * hi[1] + g.1);
        let neutral: [(f32, f32); 3] = std::array::from_fn(|c| {
            let span = hi[c] - lo[c];
            if span.abs() < 0.05 || !span.is_finite() {
                return own[c];
            }
            let a = (xhi - xlo) / span;
            (a, xlo - a * lo[c])
        });
        let ab: [(f32, f32); 3] = std::array::from_fn(|c| (lerp(own[c].0, neutral[c].0, k), lerp(own[c].1, neutral[c].1, k)));
        // fold the per-channel affine map into the separation matrix: x = diag(a) · M⁻¹ · Dn + b
        let mut m = [[0f32; 3]; 3];
        for r in 0..3 {
            for c in 0..3 {
                m[r][c] = ab[r].0 * minv[r][c];
            }
        }
        let mut q = ab.map(|v| v.1);
        let shift = key_shift(&m, &q, &a.mid);
        q = q.map(|v| v + shift);
        Converter { m, q, base, saturation: e.saturation }
    }

    /// Convert one linear-transmittance pixel to scene-linear light.
    #[inline]
    pub fn convert(&self, t: [f32; 3]) -> [f32; 3] {
        let dn: [f32; 3] = std::array::from_fn(|c| {
            let v = t[c];
            let v = if v.is_finite() { v.max(T_MIN) } else { T_MIN };
            (self.base[c] / v).log10()
        });
        let mut e = [0f32; 3];
        for c in 0..3 {
            let x = self.m[c][0] * dn[0] + self.m[c][1] * dn[1] + self.m[c][2] * dn[2] + self.q[c];
            // ±12 stops around grey is far beyond any film's range
            e[c] = GREY * 10f32.powf(x.clamp(-3.6, 3.6));
        }
        if self.saturation != 1.0 {
            let y = LUMA[0] * e[0] + LUMA[1] * e[1] + LUMA[2] * e[2];
            for v in &mut e {
                *v = (y + (*v - y) * self.saturation).max(0.0);
            }
        }
        e
    }

    /// Convert a buffer in place.
    pub fn convert_all(&self, px: &mut [[f32; 3]]) {
        for p in px {
            *p = self.convert(*p);
        }
    }

    /// Convert a working-space (Rec.2020) buffer in place: scan channels in, Rec.2020 out.
    pub fn convert_all_working(&self, px: &mut [[f32; 3]]) {
        for p in px {
            *p = from_scan(self.convert(to_scan(*p))).map(|v| v.max(0.0));
        }
    }
}

/// Exposure shift (log10) putting the median's green at [`KEY`].
fn key_shift(m: &[[f32; 3]; 3], q: &[f32; 3], mid: &[f32; 3]) -> f32 {
    let xg = m[1][0] * mid[0] + m[1][1] * mid[1] + m[1][2] * mid[2] + q[1];
    let s = (KEY / GREY).log10() - xg;
    if s.is_finite() { s } else { 0.0 }
}

/// Measure an image for conversion: `px` is `w × h` linear transmittance (row-major).
///
/// `border` (0..0.45) is the fraction of each edge left out of the tonal statistics, so the film
/// rebate, sprocket holes and scanner holder don't count as image. The film base is `base` when
/// given (a picked rebate colour), otherwise estimated from the clearest orange pixels anywhere in
/// the frame. `None` for an empty or unreadable image.
pub fn analyze(px: &[[f32; 3]], w: usize, h: usize, border: f32, base: Option<[f32; 3]>) -> Option<Analysis> {
    analyze_with(px, w, h, border, base, |p| p)
}

/// [`analyze`] of a working-space (Rec.2020) image; `base` is in scan channels.
pub fn analyze_working(px: &[[f32; 3]], w: usize, h: usize, border: f32, base: Option<[f32; 3]>) -> Option<Analysis> {
    analyze_with(px, w, h, border, base, to_scan)
}

fn analyze_with(
    px: &[[f32; 3]],
    w: usize,
    h: usize,
    border: f32,
    base: Option<[f32; 3]>,
    map: impl Fn([f32; 3]) -> [f32; 3] + Copy,
) -> Option<Analysis> {
    if w == 0 || h == 0 || px.len() < w.saturating_mul(h) {
        return None;
    }
    let step = ((w * h) as f64 / MAX_SAMPLES as f64).sqrt().ceil().max(1.0) as usize;
    let all: Vec<[f32; 3]> = (0..h)
        .step_by(step)
        .flat_map(|y| (0..w).step_by(step).filter_map(move |x| px.get(y * w + x).copied().map(map)))
        .filter(|p| p.iter().all(|v| v.is_finite()))
        .map(|p| p.map(|v| v.max(T_MIN)))
        .collect();
    if all.is_empty() {
        return None;
    }
    let base = match base {
        Some(b) if b.iter().all(|v| v.is_finite() && *v > 0.0) => b,
        _ => estimate_base(&all)?,
    };
    let b = if border.is_finite() { border.clamp(0.0, 0.45) } else { 0.05 };
    let (x0, x1) = ((w as f32 * b) as usize, (w as f32 * (1.0 - b)).ceil() as usize);
    let (y0, y1) = ((h as f32 * b) as usize, (h as f32 * (1.0 - b)).ceil() as usize);
    let mut dens: Vec<[f32; 3]> = Vec::new();
    for y in (y0..y1.min(h)).step_by(step) {
        for x in (x0..x1.min(w)).step_by(step) {
            let Some(p) = px.get(y * w + x).copied().map(map) else { continue };
            if !p.iter().all(|v| v.is_finite()) {
                continue;
            }
            dens.push(std::array::from_fn(|c| (base[c] / p[c].max(T_MIN)).log10()));
        }
    }
    if dens.is_empty() {
        return None;
    }
    // Rank by total density and average whole pixels in each band: the thin end is the deepest
    // shadows, the dense end the brightest highlights, each with its own colour. (Per-channel
    // percentiles would take each channel's extreme from a different object: the red channel's from
    // a saturated red, and so on, which reads as a cast.)
    dens.sort_unstable_by(|a, b| (a[0] + a[1] + a[2]).total_cmp(&(b[0] + b[1] + b[2])));
    let band = |from: f32, to: f32| -> [f32; 3] {
        let n = dens.len();
        let i0 = ((n as f32 * from) as usize).min(n - 1);
        let i1 = ((n as f32 * to).ceil() as usize).clamp(i0 + 1, n);
        let part = dens.get(i0..i1).unwrap_or_default();
        let k = part.len().max(1) as f32;
        std::array::from_fn(|c| part.iter().map(|p| p[c]).sum::<f32>() / k)
    };
    let out = Analysis { base, lo: band(LO_BAND.0, LO_BAND.1), hi: band(HI_BAND.0, HI_BAND.1), mid: band(0.45, 0.55) };
    out.is_valid().then_some(out)
}

/// The film base from sampled pixels: the clearest pixels that look like an orange mask (red
/// passes most, blue least), so sprocket holes and the bare light source (neutral and brighter)
/// are not taken for film. Falls back to the clearest pixels when nothing is orange (a scan
/// white-balanced on the base already).
fn estimate_base(px: &[[f32; 3]]) -> Option<[f32; 3]> {
    let peak = px.iter().map(|p| p[1]).fold(0f32, f32::max);
    if peak <= 0.0 {
        return None;
    }
    let orange: Vec<[f32; 3]> = px.iter().copied().filter(|p| p[0] > p[1] && p[1] > p[2] && p[2] < 0.85 * p[0]).collect();
    let pool = if orange.len() >= 16 { orange } else { px.iter().copied().filter(|p| p[1] < 0.995 * peak).collect::<Vec<_>>() };
    let pool = if pool.is_empty() { px.to_vec() } else { pool };
    // clearest 0.5 % by green, averaged
    let mut g: Vec<f32> = pool.iter().map(|p| p[1]).collect();
    let cut = percentile(&mut g, 0.995);
    let top: Vec<&[f32; 3]> = pool.iter().filter(|p| p[1] >= cut).collect();
    let n = top.len().max(1) as f32;
    let mut b = [0f32; 3];
    for p in &top {
        for c in 0..3 {
            b[c] += p[c] / n;
        }
    }
    b.iter().all(|v| v.is_finite() && *v > 0.0).then_some(b)
}

/// The `p` quantile of `v` (reorders `v`).
fn percentile(v: &mut [f32], p: f32) -> f32 {
    if v.is_empty() {
        return 0.0;
    }
    let i = ((v.len() - 1) as f32 * p.clamp(0.0, 1.0)).round() as usize;
    let i = i.min(v.len() - 1);
    let (_, m, _) = v.select_nth_unstable_by(i, f32::total_cmp);
    *m
}

const IDENTITY: [[f32; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn finite_or(v: f32, d: f32) -> f32 {
    if v.is_finite() { v } else { d }
}

pub(crate) fn mul(m: &[[f32; 3]; 3], v: &[f32; 3]) -> [f32; 3] {
    std::array::from_fn(|r| m[r][0] * v[0] + m[r][1] * v[1] + m[r][2] * v[2])
}

pub(crate) fn invert(m: &[[f32; 3]; 3]) -> Option<[[f32; 3]; 3]> {
    let [[a, b, c], [d, e, f], [g, h, i]] = m.map(|r| r.map(f64::from));
    let det = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g);
    if !det.is_finite() || det.abs() < 1e-9 {
        return None;
    }
    let inv = [
        [(e * i - f * h) / det, (c * h - b * i) / det, (b * f - c * e) / det],
        [(f * g - d * i) / det, (a * i - c * g) / det, (c * d - a * f) / det],
        [(d * h - e * g) / det, (b * g - a * h) / det, (a * e - b * d) / det],
    ];
    Some(inv.map(|r| r.map(|v| v as f32)))
}

#[cfg(test)]
mod tests;
