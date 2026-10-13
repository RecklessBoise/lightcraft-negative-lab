//! Negative Lab: the colour negative conversion stage (see `lightcraft_film`).
//!
//! Runs first in the scene-linear stage, on the resampled source, so white balance, exposure, tone
//! and colour work on the positive exactly as on any photo. The conversion is anchored to
//! statistics of the whole source (film base, thin and dense ends), never of the rendered window,
//! so a zoomed view, a preview and an export of one photo convert identically.

use std::sync::Mutex;

use lightcraft_develop::DevelopSettings;
use lightcraft_film::{Analysis, Converter};
use lightcraft_raster::Rgb32f;

/// Whether `s` converts a negative.
pub fn enabled(s: &DevelopSettings) -> bool {
    s.film.enabled && s.section_enabled("film")
}

/// The conversion `s` asks for on `src`, or `None` (no conversion, or an unreadable source).
pub fn converter(src: &Rgb32f, s: &DevelopSettings) -> Option<Converter> {
    if !enabled(s) {
        return None;
    }
    let a = analysis(src, s)?;
    Some(Converter::new(&a, &s.film.params()))
}

/// The analysis the conversion uses: the one stored with the edit, else a measurement of `src`.
pub fn analysis(src: &Rgb32f, s: &DevelopSettings) -> Option<Analysis> {
    if let Some(a) = s.film.stored_analysis() {
        return Some(a);
    }
    measure(src, s.film.border_fraction(), s.film.base)
}

/// Measure `src` (memoized: every render of a photo while its film settings stay put reuses it).
pub fn measure(src: &Rgb32f, border: f32, base: Option<[f32; 3]>) -> Option<Analysis> {
    static MEMO: Mutex<Vec<(u64, Analysis)>> = Mutex::new(Vec::new());
    let key = fingerprint(src, border, base);
    if let Some(a) = MEMO.lock().unwrap_or_else(|e| e.into_inner()).iter().find(|(k, _)| *k == key).map(|e| e.1) {
        return Some(a);
    }
    let a = crate::timed("film analysis", || lightcraft_film::analyze_working(&src.data, src.width, src.height, border, base))?;
    let mut m = MEMO.lock().unwrap_or_else(|e| e.into_inner());
    m.retain(|(k, _)| *k != key);
    m.push((key, a));
    if m.len() > 8 {
        m.remove(0);
    }
    Some(a)
}

/// Identity of a source buffer and the analysis inputs: its address and size, plus a spread of its
/// pixels (an address can be reused by another image once the first is freed).
fn fingerprint(src: &Rgb32f, border: f32, base: Option<[f32; 3]>) -> u64 {
    let n = src.data.len();
    let probes: Vec<u32> = (0..64usize).filter_map(|i| src.data.get(i.wrapping_mul(n) / 64)).flat_map(|p| p.map(f32::to_bits)).collect();
    crate::hash_of((src.data.as_ptr() as usize, src.width, src.height, border.to_bits(), base.map(|b| b.map(f32::to_bits)), probes))
}

/// Convert `img` (linear transmittance) to the positive in place.
pub fn apply(img: &mut Rgb32f, conv: Option<&Converter>) {
    let Some(c) = conv else { return };
    let w = img.width.max(1);
    crate::for_rows(&mut img.data, w, |_, row| c.convert_all_working(row));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RenderRequest, SourceInfo, render};

    /// A colour negative of a horizontal grey ramp (dark → light), with an orange rebate border.
    fn negative_ramp(w: usize, h: usize) -> Rgb32f {
        let base = [0.8f32, 0.45, 0.22];
        let em = lightcraft_film::emulsions::find("kodak.portra400");
        let mut img = Rgb32f::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let border = x < w / 20 || x >= w - w / 20 || y < h / 20 || y >= h - h / 20;
                img.data[y * w + x] = if border {
                    base
                } else {
                    let e = 0.01 * 100f32.powf(x as f32 / w as f32);
                    std::array::from_fn(|c| base[c] * 10f32.powf(-(em.grey[c] + em.gamma[c] * (e / 0.18).log10())))
                };
            }
        }
        img
    }

    fn converted() -> DevelopSettings {
        let mut s = DevelopSettings::default();
        s.film.enabled = true;
        s.film.emulsion = "kodak.portra400".into();
        s
    }

    #[test]
    fn a_negative_renders_as_a_neutral_positive() {
        let src = negative_ramp(240, 120);
        let out = render(&src, &SourceInfo::default(), &converted(), &RenderRequest::fit(240, 120)).image;
        let px = |x: usize| out.data[60 * out.width + x];
        let (dark, mid, light) = (px(40), px(120), px(200));
        // the ramp runs dark → light in the scene: the positive must too (the negative ran light → dark)
        assert!(dark[1] < mid[1] && mid[1] < light[1], "{dark:?} {mid:?} {light:?}");
        for p in [dark, mid, light] {
            let spread = p[0].abs_diff(p[1]).max(p[2].abs_diff(p[1]));
            assert!(spread <= 6, "grey must stay grey: {p:?}");
        }
    }

    #[test]
    fn previews_and_exports_convert_alike() {
        // the analysis reads the whole source, not the render: a small and a large render agree
        let src = negative_ramp(480, 240);
        let s = converted();
        let small = render(&src, &SourceInfo::default(), &s, &RenderRequest::fit(120, 60)).image;
        let large = render(&src, &SourceInfo::default(), &s, &RenderRequest::fit(480, 240)).image;
        let a = small.data[30 * small.width + 60];
        let b = large.data[120 * large.width + 241];
        assert!(a.iter().zip(b).all(|(x, y)| x.abs_diff(y) <= 4), "{a:?} vs {b:?}");
    }

    #[test]
    fn switching_the_film_section_off_shows_the_negative() {
        let src = negative_ramp(120, 60);
        let mut s = converted();
        s.set_section_enabled("film", false);
        let out = render(&src, &SourceInfo::default(), &s, &RenderRequest::fit(120, 60)).image;
        let (l, r) = (out.data[30 * out.width + 20], out.data[30 * out.width + 100]);
        assert!(l[1] > r[1], "still a negative: {l:?} {r:?}");
    }
}
