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
