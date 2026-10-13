//! Chart calibration: measure a film (with its lab and scanner) from a photographed colour chart.
//!
//! Shoot a 24-patch ColorChecker-layout chart on the roll, scan it like the other frames, and give
//! the four corner patch centres. [`calibrate`] fits, per output channel, log exposure as an affine
//! function of the three base-relative densities (12 numbers, least squares over the patches): the
//! film's slopes, its dye crosstalk and the scanner's channel mix in one [`Measured`] profile.
//!
//! The reference colours are the chart's published nominal sRGB values (D65), linearized: the fit
//! runs in scan channels (linear sRGB primaries, see [`crate::to_scan`]). They are nominal: an individual chart differs a little, and so does its lighting.

use serde::Serialize;

use crate::{Measured, T_MIN};

/// Nominal 8-bit sRGB values of the 24 patches of the classic colour checker layout, row by row
/// from dark skin (top left) to black (bottom right).
pub const CHART_SRGB8: [[u8; 3]; 24] = [
    [115, 82, 68],
    [194, 150, 130],
    [98, 122, 157],
    [87, 108, 67],
    [133, 128, 177],
    [103, 189, 170],
    [214, 126, 44],
    [80, 91, 166],
    [193, 90, 99],
    [94, 60, 108],
    [157, 188, 64],
    [224, 163, 46],
    [56, 61, 150],
    [70, 148, 73],
    [175, 54, 60],
    [231, 199, 31],
    [187, 86, 149],
    [8, 133, 161],
    [243, 243, 242],
    [200, 200, 200],
    [160, 160, 160],
    [122, 122, 121],
    [85, 85, 85],
    [52, 52, 52],
];

/// Patch names, same order.
pub const CHART_NAMES: [&str; 24] = [
    "dark skin",
    "light skin",
    "blue sky",
    "foliage",
    "blue flower",
    "bluish green",
    "orange",
    "purplish blue",
    "moderate red",
    "purple",
    "yellow green",
    "orange yellow",
    "blue",
    "green",
    "red",
    "yellow",
    "magenta",
    "cyan",
    "white",
    "neutral 8",
    "neutral 6.5",
    "neutral 5",
    "neutral 3.5",
    "black",
];

/// The chart's reference colours, linear sRGB (scan channels).
pub fn chart_reference() -> [[f32; 3]; 24] {
    CHART_SRGB8.map(|p| {
        p.map(|v| {
            let v = v as f32 / 255.0;
            if v <= 0.040_45 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
        })
    })
}

/// Why a calibration failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CalibrationError {
    /// Fewer usable patches than the fit needs.
    TooFewPatches(usize),
    /// The patches don't separate the channels (all the same colour, or an unreadable image).
    Degenerate,
}

impl std::fmt::Display for CalibrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CalibrationError::TooFewPatches(n) => write!(f, "only {n} usable chart patches (at least 8 are needed)"),
            CalibrationError::Degenerate => write!(f, "the chart patches don't vary enough to measure the film: check the corner positions"),
        }
    }
}

impl std::error::Error for CalibrationError {}

/// A fitted profile and how well it fits.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Calibration {
    pub measured: Measured,
    /// Root-mean-square error of the fit over the patches, in stops.
    pub rms_stops: f32,
    /// Patches used (the very dark ones are left out: they sit on the film's toe).
    pub patches: usize,
}

/// Fit a [`Measured`] profile: `negative[i]` is patch `i`'s linear transmittance on the scan,
/// `reference[i]` its true scene-linear colour, `base` the film base transmittance.
pub fn calibrate(negative: &[[f32; 3]], reference: &[[f32; 3]], base: [f32; 3]) -> Result<Calibration, CalibrationError> {
    let base = base.map(|b| if b.is_finite() { b.max(T_MIN) } else { 1.0 });
    let mut rows: Vec<([f64; 4], [f64; 3])> = Vec::new();
    for (n, r) in negative.iter().zip(reference) {
        // the darkest patches sit on the toe, where density no longer tracks exposure
        if r.iter().any(|v| !v.is_finite() || *v < 0.01) || n.iter().any(|v| !v.is_finite() || *v <= 0.0) {
            continue;
        }
        let dn: [f64; 3] = std::array::from_fn(|c| f64::from((base[c] / n[c].max(T_MIN)).log10()));
        let x: [f64; 3] = std::array::from_fn(|c| f64::from((r[c] / crate::GREY).log10()));
        rows.push(([dn[0], dn[1], dn[2], 1.0], x));
    }
    if rows.len() < 8 {
        return Err(CalibrationError::TooFewPatches(rows.len()));
    }
    // every density channel must vary across the patches, or the slopes are unmeasurable
    for k in 0..3 {
        let (lo, hi) = rows.iter().fold((f64::MAX, f64::MIN), |(l, h), (a, _)| (l.min(a[k]), h.max(a[k])));
        if hi - lo < 0.1 {
            return Err(CalibrationError::Degenerate);
        }
    }
    // normal equations, shared by the three output channels
    let mut ata = [[0f64; 4]; 4];
    let mut atb = [[0f64; 3]; 4];
    for (a, x) in &rows {
        for i in 0..4 {
            for j in 0..4 {
                ata[i][j] += a[i] * a[j];
            }
            for c in 0..3 {
                atb[i][c] += a[i] * x[c];
            }
        }
    }
    // a touch of ridge keeps a near-degenerate set of patches from producing wild slopes
    for (i, row) in ata.iter_mut().enumerate().take(3) {
        row[i] += 1e-6 * rows.len() as f64;
    }
    let sol = solve4(ata, atb).ok_or(CalibrationError::Degenerate)?;
    let mut matrix = [[0f32; 3]; 3];
    let mut offset = [0f32; 3];
    for c in 0..3 {
        for k in 0..3 {
            matrix[c][k] = sol[k][c] as f32;
        }
        offset[c] = sol[3][c] as f32;
    }
    if !matrix.iter().flatten().chain(offset.iter()).all(|v| v.is_finite()) {
        return Err(CalibrationError::Degenerate);
    }
    let mut se = 0f64;
    for (a, x) in &rows {
        for c in 0..3 {
            let pred: f64 = (0..3).map(|k| f64::from(matrix[c][k]) * a[k]).sum::<f64>() + f64::from(offset[c]);
            se += (pred - x[c]).powi(2);
        }
    }
    // log10 → stops
    let rms_stops = ((se / (rows.len() * 3) as f64).sqrt() / std::f64::consts::LOG10_2) as f32;
    Ok(Calibration { measured: Measured { matrix, offset }, rms_stops, patches: rows.len() })
}

/// Solve `a · x = b` (4 × 4, three right-hand sides) by Gaussian elimination with partial pivoting.
fn solve4(mut a: [[f64; 4]; 4], mut b: [[f64; 3]; 4]) -> Option<[[f64; 3]; 4]> {
    for col in 0..4 {
        let piv = (col..4).max_by(|&i, &j| a[i][col].abs().total_cmp(&a[j][col].abs()))?;
        if a[piv][col].abs() < 1e-12 {
            return None;
        }
        a.swap(col, piv);
        b.swap(col, piv);
        for r in 0..4 {
            if r == col {
                continue;
            }
            let f = a[r][col] / a[col][col];
            for k in col..4 {
                a[r][k] -= f * a[col][k];
            }
            for k in 0..3 {
                b[r][k] -= f * b[col][k];
            }
        }
    }
    let mut x = [[0f64; 3]; 4];
    for r in 0..4 {
        for k in 0..3 {
            x[r][k] = b[r][k] / a[r][r];
        }
    }
    Some(x)
}

/// Sample the 24 patches of a chart in a `w × h` image: `corners` are the normalized centres of the
/// top-left, top-right, bottom-right and bottom-left patches as the chart appears (any rotation or
/// perspective close to a parallelogram). Each patch averages a box a quarter of the patch pitch
/// wide. `None` when a corner is outside the image.
pub fn sample_chart(px: &[[f32; 3]], w: usize, h: usize, corners: [[f32; 2]; 4]) -> Option<[[f32; 3]; 24]> {
    if w == 0 || h == 0 || px.len() < w.saturating_mul(h) || corners.iter().flatten().any(|v| !v.is_finite() || !(0.0..=1.0).contains(v)) {
        return None;
    }
    let [tl, tr, br, bl] = corners.map(|[x, y]| [x * w as f32, y * h as f32]);
    let at = |u: f32, v: f32| -> [f32; 2] {
        std::array::from_fn(|k| {
            let top = tl[k] + (tr[k] - tl[k]) * u;
            let bot = bl[k] + (br[k] - bl[k]) * u;
            top + (bot - top) * v
        })
    };
    let pitch = ((tr[0] - tl[0]).hypot(tr[1] - tl[1]) / 5.0).min((bl[0] - tl[0]).hypot(bl[1] - tl[1]) / 3.0);
    let r = (pitch / 8.0).max(1.0) as i64;
    let mut out = [[0f32; 3]; 24];
    for (i, o) in out.iter_mut().enumerate() {
        let (col, row) = ((i % 6) as f32 / 5.0, (i / 6) as f32 / 3.0);
        let [cx, cy] = at(col, row);
        let (cx, cy) = (cx as i64, cy as i64);
        let mut acc = [0f64; 3];
        let mut n = 0u32;
        for y in (cy - r)..=(cy + r) {
            for x in (cx - r)..=(cx + r) {
                if x < 0 || y < 0 || x >= w as i64 || y >= h as i64 {
                    continue;
                }
                if let Some(p) = px.get(y as usize * w + x as usize) {
                    for c in 0..3 {
                        acc[c] += f64::from(p[c]);
                    }
                    n += 1;
                }
            }
        }
        if n == 0 {
            return None;
        }
        *o = acc.map(|v| (v / f64::from(n)) as f32);
    }
    Some(out)
}
