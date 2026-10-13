//! Negative Lab commands: convert colour negatives (one photo or a whole roll), choose the
//! emulsion, pick the film base, re-measure, and calibrate from a photographed chart.
//!
//! Every photo is measured on its own preview-size source, and the measurement is stored with the
//! edit ([`lightcraft_develop::Film::analysis`]) so its thumbnail, loupe and export agree.

use lightcraft_catalog::PhotoId;
use lightcraft_develop::{DevelopSettings, Film};
use lightcraft_film::{EMULSIONS, emulsions};
use serde_json::{Value, json};

use super::{CommandSpec, always, bad, bool_or, cmd, f64_req, has_active, has_selection, str_param};
use crate::{Result, Session, media::SourceLevel};

/// Measure photo `id` under film settings `f` (its border and picked base).
fn measure(s: &mut Session, c: &str, id: PhotoId, f: &Film) -> Result<lightcraft_film::Analysis> {
    let src = s.source_now(id, SourceLevel::Preview).map_err(|e| bad(c, e))?;
    lightcraft_film::analyze(&src.data, src.width, src.height, f.border_fraction(), f.base)
        .ok_or_else(|| bad(c, "the photo has no readable pixels to measure"))
}

/// Apply `f` to photo `id`'s settings, re-measure it, and commit as `label`.
fn update(s: &mut Session, c: &str, id: PhotoId, label: &str, f: impl FnOnce(&mut DevelopSettings) -> Result<()>) -> Result<Value> {
    let mut d = (*s.develop_of(id).unwrap_or_default()).clone();
    f(&mut d)?;
    let a = measure(s, c, id, &d.film)?;
    d.film.analysis = Some(a);
    let film = d.film.clone();
    s.set_develop(id, d, label)?;
    Ok(summary(id, &film))
}

fn summary(id: PhotoId, f: &Film) -> Value {
    let e = emulsions::find(&f.emulsion);
    json!({
        "id": id.0,
        "enabled": f.enabled,
        "emulsion": e.id,
        "name": e.full_name(),
        "calibrated": f.measured.is_some(),
        "analysis": f.analysis,
    })
}

fn emulsion_param(p: &Value, c: &str) -> Result<Option<String>> {
    match str_param(p, "emulsion") {
        None => Ok(None),
        Some(id) if emulsions::exists(id) => Ok(Some(id.to_string())),
        Some(id) => Err(bad(c, format!("unknown emulsion `{id}` (see film.emulsions)"))),
    }
}

fn active(s: &Session, c: &str) -> Result<PhotoId> {
    s.active().ok_or_else(|| bad(c, "no active photo"))
}

fn corners(p: &Value, c: &str) -> Result<[[f32; 2]; 4]> {
    let err = || bad(c, "`corners` must be four [x, y] pairs (normalized: top-left, top-right, bottom-right, bottom-left patch centres)");
    let a = p.get("corners").and_then(Value::as_array).filter(|a| a.len() == 4).ok_or_else(err)?;
    let mut out = [[0f32; 2]; 4];
    for (o, v) in out.iter_mut().zip(a) {
        let xy: Vec<f64> = v.as_array().ok_or_else(err)?.iter().filter_map(Value::as_f64).collect();
        if xy.len() != 2 || xy.iter().any(|v| !v.is_finite() || !(0.0..=1.0).contains(v)) {
            return Err(err());
        }
        *o = [xy[0] as f32, xy[1] as f32];
    }
    Ok(out)
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            query "film.emulsions",
            "Film Emulsions",
            [],
            None,
            "{} → [{id, name, maker, iso, gamma, grey, crosstalk, saturation, source, notes}] the built-in colour negative profiles",
            always,
            |_, _| Ok(serde_json::to_value(EMULSIONS).unwrap_or_default())
        ),
        cmd!(
            "film.convert",
            "Convert Negative",
            ["Photo"],
            Some("Cmd+Alt+N"),
            "{emulsion?: id (default: keep, or generic), enabled?: bool (default true), ids?: [id] (default: the selection)} — convert colour negatives to positives: each photo is measured on its own (film base, shadows, highlights) → [{id, enabled, emulsion, name, calibrated, analysis}]",
            has_selection,
            |s, p| {
                const C: &str = "film.convert";
                let emulsion = emulsion_param(p, C)?;
                let enabled = bool_or(p, "enabled", true);
                let ids = s.targets(p);
                if ids.is_empty() {
                    return Err(bad(C, "no photos selected"));
                }
                let label = if enabled { "Convert Negative" } else { "Negative Off" };
                let mut out = Vec::new();
                for id in ids {
                    let e = emulsion.clone();
                    out.push(update(s, C, id, label, |d| {
                        d.film.enabled = enabled;
                        if let Some(e) = e {
                            d.film.emulsion = e;
                        }
                        Ok(())
                    })?);
                }
                Ok(Value::Array(out))
            }
        ),
        cmd!(
            "film.set",
            "Negative Lab Settings",
            [],
            None,
            "{emulsion?: id, autoExposure?: bool, ids?: [id] (default: the active photo)} — change the emulsion profile or exposure keying of converted negatives (sliders: film.neutralize, film.separation, film.border via develop.set)",
            has_active,
            |s, p| {
                const C: &str = "film.set";
                let emulsion = emulsion_param(p, C)?;
                let auto = p.get("autoExposure").and_then(Value::as_bool);
                let ids = if p.get("ids").is_some() { s.targets(p) } else { vec![active(s, C)?] };
                let mut out = Vec::new();
                for id in ids {
                    let e = emulsion.clone();
                    out.push(update(s, C, id, "Film Emulsion", |d| {
                        if let Some(e) = e {
                            d.film.emulsion = e;
                        }
                        if let Some(a) = auto {
                            d.film.auto_exposure = a;
                        }
                        Ok(())
                    })?);
                }
                Ok(Value::Array(out))
            }
        ),
        cmd!(
            "film.analyze",
            "Measure Negative",
            [],
            None,
            "{ids?: [id] (default: the active photo)} — measure the negative again (after changing the border buffer or the film base)",
            has_active,
            |s, p| {
                const C: &str = "film.analyze";
                let ids = if p.get("ids").is_some() { s.targets(p) } else { vec![active(s, C)?] };
                let mut out = Vec::new();
                for id in ids {
                    out.push(update(s, C, id, "Measure Negative", |_| Ok(()))?);
                }
                Ok(Value::Array(out))
            }
        ),
        cmd!(
            "film.pickBase",
            "Film Base Selector",
            [],
            None,
            "{x, y} normalized image coords of unexposed film (the rebate between frames) — sets the film base colour and measures again",
            has_active,
            |s, p| {
                const C: &str = "film.pickBase";
                let (x, y) = (f64_req(p, "x", C)?, f64_req(p, "y", C)?);
                if !(x.is_finite() && y.is_finite()) {
                    return Err(bad(C, "`x` and `y` must be numbers"));
                }
                let id = active(s, C)?;
                let src = s.source_now(id, SourceLevel::Preview).map_err(|e| bad(C, e))?;
                let d = s.develop_of(id).unwrap_or_default();
                let o = src.oriented(d.orientation);
                let (cx, cy) = ((x.clamp(0.0, 1.0) * o.width as f64) as isize, (y.clamp(0.0, 1.0) * o.height as f64) as isize);
                let r = (o.width.max(o.height) / 200).clamp(1, 12) as isize;
                let mut acc = [0f64; 3];
                let mut n = 0.0;
                for dy in -r..=r {
                    for dx in -r..=r {
                        let c = o.get_clamped(cx + dx, cy + dy);
                        for i in 0..3 {
                            acc[i] += f64::from(c[i]);
                        }
                        n += 1.0;
                    }
                }
                let base = acc.map(|v| (v / n) as f32);
                if base.iter().any(|v| !v.is_finite() || *v <= 0.0) {
                    return Err(bad(C, "that point is black: pick the clear film between frames"));
                }
                update(s, C, id, "Film Base", |d| {
                    d.film.enabled = true;
                    d.film.base = Some(base);
                    Ok(())
                })
            }
        ),
        cmd!("film.resetBase", "Reset Film Base", [], None, "{} — estimate the film base automatically again", has_active, |s, _| {
            const C: &str = "film.resetBase";
            let id = active(s, C)?;
            update(s, C, id, "Film Base", |d| {
                d.film.base = None;
                Ok(())
            })
        }),
        cmd!(
            "film.calibrate",
            "Calibrate Film from Chart",
            [],
            None,
            "{corners: [[x,y] ×4] normalized centres of the top-left (dark skin), top-right (bluish green), bottom-right (black) and bottom-left (white) patches of a 24-patch colour chart shot on the roll; ids?: [id] photos to give the profile (default: the active one)} — measures the film, lab and scanner together → {rmsStops, patches}",
            has_active,
            |s, p| {
                const C: &str = "film.calibrate";
                let corners = corners(p, C)?;
                let id = active(s, C)?;
                let src = s.source_now(id, SourceLevel::Preview).map_err(|e| bad(C, e))?;
                let d = s.develop_of(id).unwrap_or_default();
                let o = src.oriented(d.orientation);
                let patches = lightcraft_film::calibrate::sample_chart(&o.data, o.width, o.height, corners)
                    .ok_or_else(|| bad(C, "the chart corners are outside the photo"))?;
                let base = match d.film.stored_analysis() {
                    Some(a) => a.base,
                    None => measure(s, C, id, &d.film)?.base,
                };
                let cal =
                    lightcraft_film::calibrate(&patches, &lightcraft_film::calibrate::chart_reference(), base).map_err(|e| bad(C, e.to_string()))?;
                let ids = if p.get("ids").is_some() { s.targets(p) } else { vec![id] };
                for t in ids {
                    update(s, C, t, "Film Calibration", |d| {
                        d.film.enabled = true;
                        d.film.measured = Some(cal.measured);
                        Ok(())
                    })?;
                }
                Ok(json!({"rmsStops": cal.rms_stops, "patches": cal.patches, "measured": cal.measured}))
            }
        ),
        cmd!("film.clearCalibration", "Clear Film Calibration", [], None, "{} — go back to the emulsion profile", has_active, |s, _| {
            const C: &str = "film.clearCalibration";
            let id = active(s, C)?;
            update(s, C, id, "Film Calibration", |d| {
                d.film.measured = None;
                Ok(())
            })
        }),
    ]
}
