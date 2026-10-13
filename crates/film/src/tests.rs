use super::*;
use crate::calibrate::{chart_reference, sample_chart};

const ORANGE: [f32; 3] = [0.82, 0.48, 0.26];

/// Expose and develop a film: scene-linear `e` → transmittance on the scan, through the emulsion's
/// own slopes, grey densities and crosstalk, a scanner channel gain and the orange base.
fn negative(e: [f32; 3], em: &Emulsion, scan_gain: [f32; 3]) -> [f32; 3] {
    let a: [f32; 3] = std::array::from_fn(|c| em.grey[c] + em.gamma[c] * (e[c].max(1e-6) / GREY).log10());
    let dn = mul(&em.crosstalk, &a);
    std::array::from_fn(|c| ORANGE[c] * scan_gain[c] * 10f32.powf(-dn[c]))
}

fn flat(em: &'static Emulsion) -> Emulsion {
    Emulsion { saturation: 1.0, ..*em }
}

/// A frame: an orange rebate border around a scene of greys and colours.
fn frame(em: &Emulsion, gain: [f32; 3], scene: &[[f32; 3]]) -> (Vec<[f32; 3]>, usize, usize) {
    let (w, h) = (200, 120);
    let mut px = vec![[0f32; 3]; w * h];
    for y in 0..h {
        for x in 0..w {
            let inside = x >= 10 && x < w - 10 && y >= 10 && y < h - 10;
            px[y * w + x] = if inside {
                let i = (x / 7 + y / 9) % scene.len();
                negative(scene[i], em, gain)
            } else {
                std::array::from_fn(|c| ORANGE[c] * gain[c])
            };
        }
    }
    // sprocket holes: bare light, brighter and neutral, must not be taken for the base
    for y in 0..4 {
        for x in (0..w).step_by(20) {
            px[y * w + x] = [1.0, 1.0, 1.0];
        }
    }
    (px, w, h)
}

fn scene() -> Vec<[f32; 3]> {
    let mut s: Vec<[f32; 3]> = [0.02f32, 0.05, 0.1, 0.18, 0.35, 0.6, 0.9].iter().map(|&v| [v, v, v]).collect();
    s.extend(chart_reference());
    s
}

fn params(em: &Emulsion, neutralize: f32) -> Params<'_> {
    Params { emulsion: em, neutralize, separation: 1.0, auto_exposure: true, measured: None }
}

#[test]
fn base_estimate_finds_the_orange_rebate_not_the_sprocket_holes() {
    let em = flat(emulsions::find("kodak.portra400"));
    let (px, w, h) = frame(&em, [1.0; 3], &scene());
    let a = analyze(&px, w, h, 0.1, None).unwrap();
    for c in 0..3 {
        assert!((a.base[c] - ORANGE[c]).abs() < 0.01, "{:?}", a.base);
    }
}

#[test]
fn the_emulsions_own_inversion_recovers_scene_colour() {
    // neutralize 0, same scanner as the profile assumes: colour ratios come back exactly
    for em in EMULSIONS {
        let em = flat(em);
        let sc = scene();
        let (px, w, h) = frame(&em, [1.0; 3], &sc);
        let a = analyze(&px, w, h, 0.1, None).unwrap();
        let conv = Converter::new(&a, &params(&em, 0.0));
        let grey = conv.convert(negative([0.18; 3], &em, [1.0; 3]));
        for e in &sc {
            let out = conv.convert(negative(*e, &em, [1.0; 3]));
            for c in 0..3 {
                // exposure is re-keyed: compare against grey
                let want = e[c] / 0.18;
                let got = out[c] / grey[1];
                assert!((got / want).log2().abs() < 0.02, "{} {e:?} → {out:?}", em.id);
            }
        }
    }
}

#[test]
fn neutralize_removes_a_scanner_cast() {
    // a scanner whose channels see the film differently from the profile: a strong cast
    let em = flat(emulsions::find("kodak.portra400"));
    let gain = [1.0, 0.7, 0.45];
    let sc = scene();
    let (px, w, h) = frame(&em, gain, &sc);
    let a = analyze(&px, w, h, 0.1, None).unwrap();
    let conv = Converter::new(&a, &params(&em, 1.0));
    for v in [0.05f32, 0.18, 0.6] {
        let out = conv.convert(negative([v; 3], &em, gain));
        let spread = (out[0] / out[1]).log2().abs().max((out[2] / out[1]).log2().abs());
        assert!(spread < 0.05, "grey {v} → {out:?}");
    }
}

#[test]
fn separation_restores_saturation() {
    let em = flat(emulsions::find("kodak.ektar100"));
    let sc = scene();
    let (px, w, h) = frame(&em, [1.0; 3], &sc);
    let a = analyze(&px, w, h, 0.1, None).unwrap();
    let red = negative(chart_reference()[14], &em, [1.0; 3]);
    let sat = |s: f32| {
        let c = Converter::new(&a, &Params { separation: s, ..params(&em, 0.0) }).convert(red);
        c[0] / c[1].max(1e-6)
    };
    assert!(sat(1.0) > sat(0.0) * 1.2, "{} vs {}", sat(1.0), sat(0.0));
}

#[test]
fn profile_saturation_is_applied_around_luminance() {
    let em = emulsions::find("lomography.metropolis");
    let a = Analysis { base: ORANGE, lo: [0.1; 3], hi: [1.5; 3], mid: [0.8; 3] };
    let conv = Converter::new(&a, &params(em, 0.0));
    let flatc = Converter::new(&a, &params(&flat(em), 0.0));
    let t = negative([0.4, 0.1, 0.05], em, [1.0; 3]);
    let (m, f) = (conv.convert(t), flatc.convert(t));
    assert!(m[0] / m[1] < f[0] / f[1], "Metropolis must be more muted: {m:?} vs {f:?}");
}

#[test]
fn hostile_inputs_never_produce_nan() {
    let em = emulsions::find(emulsions::GENERIC);
    let bad = Analysis { base: [f32::NAN, 0.0, -1.0], lo: [f32::INFINITY; 3], hi: [0.0; 3], mid: [f32::NAN; 3] };
    let conv = Converter::new(&bad, &Params { neutralize: f32::NAN, separation: 1e9, ..params(em, 1.0) });
    for t in [[0.0; 3], [f32::NAN; 3], [f32::INFINITY, -1.0, 1e30], [0.5; 3]] {
        assert!(conv.convert(t).iter().all(|v| v.is_finite() && *v >= 0.0), "{t:?}");
    }
    assert!(analyze(&[], 0, 0, 0.05, None).is_none());
    assert!(analyze(&[[0.5; 3]; 4], 4, 4, 0.05, None).is_none(), "buffer shorter than w × h");
    assert!(analyze(&[[f32::NAN; 3]; 16], 4, 4, 0.05, None).is_none());
    assert!(!bad.is_valid());
}

#[test]
fn picking_another_base_shifts_the_statistics() {
    let a = Analysis { base: [0.8, 0.5, 0.25], lo: [0.1; 3], hi: [1.0; 3], mid: [0.5; 3] };
    let b = a.with_base([0.4, 0.5, 0.25]);
    assert!((b.lo[0] - (0.1 - 2f32.log10())).abs() < 1e-5);
    assert_eq!(b.lo[1], 0.1);
}

#[test]
fn chart_calibration_recovers_the_film_and_scanner() {
    let em = flat(emulsions::find("lomography.purple"));
    let gain = [0.9, 0.6, 0.5];
    let reference = chart_reference();
    // a chart shot one stop over: the fit absorbs that too
    let neg: Vec<[f32; 3]> = reference.iter().map(|r| negative(r.map(|v| v * 2.0), &em, gain)).collect();
    let base = std::array::from_fn(|c| ORANGE[c] * gain[c]);
    let cal = calibrate(&neg, &reference, base).unwrap();
    assert!(cal.rms_stops < 0.01, "{cal:?}");
    let a = Analysis { base, lo: [0.0; 3], hi: [1.0; 3], mid: [0.5; 3] };
    let conv = Converter::new(&a, &Params { measured: Some(&cal.measured), auto_exposure: false, ..params(&em, 1.0) });
    for (n, r) in neg.iter().zip(&reference).filter(|(_, r)| r.iter().all(|v| *v > 0.01)) {
        let out = conv.convert(*n);
        for c in 0..3 {
            assert!((out[c] / r[c]).log2().abs() < 0.05, "{r:?} → {out:?}");
        }
    }
}

#[test]
fn calibration_rejects_too_few_or_flat_patches() {
    assert_eq!(calibrate(&[[0.5; 3]; 3], &[[0.2; 3]; 3], [1.0; 3]), Err(CalibrationError::TooFewPatches(3)));
    assert_eq!(calibrate(&[[0.5; 3]; 24], &[[0.2; 3]; 24], [1.0; 3]), Err(CalibrationError::Degenerate));
}

#[test]
fn chart_sampling_reads_patch_centres() {
    let (w, h) = (600, 400);
    let refs = chart_reference();
    let mut px = vec![[0f32; 3]; w * h];
    for y in 0..h {
        for x in 0..w {
            // patches 100 × 100 px, centres at 50 + 100 i
            px[y * w + x] = refs[(y / 100) * 6 + x / 100];
        }
    }
    let n = |x: f32, y: f32| [x / w as f32, y / h as f32];
    let got = sample_chart(&px, w, h, [n(50.0, 50.0), n(550.0, 50.0), n(550.0, 350.0), n(50.0, 350.0)]).unwrap();
    assert_eq!(got, refs);
    assert!(sample_chart(&px, w, h, [[2.0, 0.0]; 4]).is_none());
}
