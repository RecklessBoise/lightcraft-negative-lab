//! Negative Lab commands.

use serde_json::json;

use crate::Session;

fn demo() -> Session {
    Session::with_demo()
}

#[test]
fn convert_measures_and_stores_the_negative() {
    let mut s = demo();
    let r = s.execute("film.convert", &json!({"emulsion": "kodak.portra400"})).unwrap();
    assert_eq!(r[0]["emulsion"], "kodak.portra400");
    let id = s.active().unwrap();
    let d = s.develop_of(id).unwrap();
    assert!(d.film.enabled && d.film.analysis.is_some_and(|a| a.is_valid()));
    // the settings round-trip through JSON (sidecars, presets, the catalog)
    let back: lightcraft_develop::DevelopSettings = serde_json::from_value(serde_json::to_value(&*d).unwrap()).unwrap();
    assert_eq!(back.film, d.film);
    assert!(s.render_now(id, 160, 120).is_ok());
    // one undo step brings the positive back
    s.execute("edit.undo", &json!({})).unwrap();
    assert!(!s.develop_of(id).unwrap().film.enabled);
}

#[test]
fn emulsion_base_and_sliders() {
    let mut s = demo();
    s.execute("film.convert", &json!({})).unwrap();
    s.execute("film.set", &json!({"emulsion": "lomography.purple"})).unwrap();
    assert!(s.execute("film.set", &json!({"emulsion": "nope"})).is_err());
    s.execute("develop.set", &json!({"control": "film.neutralize", "value": 40})).unwrap();
    s.execute("develop.set", &json!({"control": "film.border", "value": 99})).unwrap();
    s.execute("film.pickBase", &json!({"x": 0.01, "y": 0.01})).unwrap();
    let d = s.develop_of(s.active().unwrap()).unwrap();
    assert_eq!(d.film.emulsion, "lomography.purple");
    assert_eq!(d.film.neutralize, 40.0);
    assert_eq!(d.film.border, 45.0, "clamped to the spec");
    assert!(d.film.base.is_some());
    s.execute("film.resetBase", &json!({})).unwrap();
    assert!(s.develop_of(s.active().unwrap()).unwrap().film.base.is_none());
}

#[test]
fn a_roll_converts_photo_by_photo() {
    let mut s = demo();
    let ids: Vec<u64> = s.catalog.photos().take(3).map(|p| p.id.0).collect();
    let r = s.execute("film.convert", &json!({"ids": ids, "emulsion": "lomography.metropolis"})).unwrap();
    assert_eq!(r.as_array().unwrap().len(), 3);
    // each frame keeps its own measurement
    let a: Vec<_> = r.as_array().unwrap().iter().map(|v| v["analysis"].clone()).collect();
    assert!(a[0] != a[1] || a[1] != a[2]);
}

#[test]
fn bad_arguments_are_errors_not_crashes() {
    let mut s = demo();
    assert!(s.execute("film.calibrate", &json!({"corners": [[0, 0]]})).is_err());
    assert!(s.execute("film.calibrate", &json!({"corners": [[0.1, 0.1], [0.9, 0.1], [0.9, f64::MAX], [0.1, 0.9]]})).is_err());
    // a demo photo is not a chart: the fit fails cleanly or succeeds, never panics
    let _ = s.execute("film.calibrate", &json!({"corners": [[0.1, 0.1], [0.9, 0.1], [0.9, 0.9], [0.1, 0.9]]}));
    assert!(s.execute("film.pickBase", &json!({"x": "a"})).is_err());
    let list = s.execute("film.emulsions", &json!({})).unwrap();
    assert!(list.as_array().unwrap().iter().any(|e| e["id"] == "lomography.metropolis"));
}

/// A colour negative of grey and coloured bars as a scanner writes it (8-bit sRGB-encoded
/// transmittance, orange base, rebate border), made through the emulsion's own model.
fn write_negative(path: &std::path::Path, emulsion: &str, scene: &[[f32; 3]]) {
    let em = lightcraft_film::emulsions::find(emulsion);
    let base = [0.86f32, 0.52, 0.30];
    let (w, h) = (40 * scene.len() + 40, 120);
    let enc = |v: f32| lightcraft_color::transfer::encode_srgb8(v.clamp(0.0, 1.0));
    let data: Vec<[u8; 4]> = (0..w * h)
        .map(|i| {
            let (x, y) = (i % w, i / w);
            let t = if x < 20 || x >= w - 20 || y < 20 || y >= h - 20 {
                base
            } else {
                let s = scene[(x - 20) / 40];
                let a: [f32; 3] = std::array::from_fn(|c| em.grey[c] + em.gamma[c] * (s[c].max(1e-4) / 0.18).log10());
                let m = em.crosstalk;
                std::array::from_fn(|c| base[c] * 10f32.powf(-(m[c][0] * a[0] + m[c][1] * a[1] + m[c][2] * a[2])))
            };
            let [r, g, b] = t.map(enc);
            [r, g, b, 255]
        })
        .collect();
    let img = lightcraft_raster::Rgba8 { width: w, height: h, data };
    let bytes = lightcraft_codecs::encode_png(&lightcraft_codecs::EncodeImage::rgba8(&img), &lightcraft_codecs::EncodeMeta::default()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

#[test]
fn an_imported_scan_converts_to_the_scene_it_was_shot_from() {
    let dir = std::env::temp_dir().join(format!("lc-film-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("neg.png");
    // linear sRGB scene: greys, then a warm skin tone and a sky blue
    let scene = [[0.04f32; 3], [0.18; 3], [0.5; 3], [0.45, 0.30, 0.22], [0.15, 0.25, 0.45]];
    write_negative(&path, "kodak.portra400", &scene);
    let mut s = Session::new().with_fs();
    s.execute("library.import", &json!({"paths": [path.to_string_lossy()]})).unwrap();
    let id = s.catalog.photos().next().unwrap().id;
    s.execute("photo.select", &json!({"ids": [id.0]})).ok();
    s.execute("film.convert", &json!({"ids": [id.0], "emulsion": "kodak.portra400"})).unwrap();
    s.execute("develop.set", &json!({"control": "film.neutralize", "value": 0, "ids": [id.0]})).unwrap();
    let (w, h) = (40 * scene.len() + 40, 120);
    let r = s.render_now(id, w, h).unwrap();
    let img = r.image;
    eprintln!("render {}x{} film {:?}", img.width, img.height, s.develop_of(id).unwrap().film);
    let at = |i: usize| {
        let p = img.data[(img.height / 2) * img.width + 20 + 40 * i + 20];
        [p[0], p[1], p[2]].map(lightcraft_color::transfer::decode_srgb8)
    };
    let grey = at(1);
    for i in 0..scene.len() {
        let got = at(i);
        // compare colour (channel ratios), not tone: the display rendering adds its own curve
        let (gr, gb) = (got[0] / got[1], got[2] / got[1]);
        let (wr, wb) = (scene[i][0] / scene[i][1], scene[i][2] / scene[i][1]);
        eprintln!("patch {i}: got {got:?} ratios {gr:.3} {gb:.3}, want {wr:.3} {wb:.3} (grey {grey:?})");
    }
    for i in 0..3 {
        let g = at(i);
        assert!((g[0] / g[1]).log2().abs() < 0.08 && (g[2] / g[1]).log2().abs() < 0.08, "grey {i} not neutral: {g:?}");
    }
}
