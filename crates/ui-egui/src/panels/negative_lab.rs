//! The Negative Lab panel: convert colour negatives, choose the film stock, pick the film base,
//! fine-tune the conversion and the positive, and calibrate a film from a photographed chart.
//!
//! Like every right-hand panel it is resized by dragging its inner edge; the S / M / L / XL buttons
//! set the common widths in one click.

use egui::{Align2, Color32, RichText, Sense, Stroke, pos2, vec2};
use lightcraft_catalog::PhotoId;
use lightcraft_develop::{DevelopSettings, controls};
use lightcraft_film::{EMULSIONS, emulsions};
use serde_json::json;

use super::edit::{REL_TEMP, REL_TINT, apply_slider_out, control, k_to_rel, rel_to_k, section, sub_title};
use crate::LightcraftApp;
use crate::icons::Icon;
use crate::state::RIGHT_WIDTH;
use crate::theme::Tokens;
use crate::widgets::{divider, slider, text_button};

/// Panel widths behind the size buttons (clamped to [`RIGHT_WIDTH`]).
pub const SIZES: [(&str, f32); 4] = [("S", 250.0), ("M", 320.0), ("L", 420.0), ("XL", 560.0)];

/// The on-canvas tools of this panel.
pub const BASE_TOOL: &str = "filmBase";
pub const CHART_TOOL: &str = "filmChart";

/// The corner patches of the chart, in the order `film.calibrate` takes them.
pub const CHART_CORNERS: [&str; 4] =
    ["top-left patch (dark skin)", "top-right patch (bluish green)", "bottom-right patch (black)", "bottom-left patch (white)"];

fn padded(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::NONE.inner_margin(egui::Margin { left: 24, right: 22, top: 6, bottom: 6 }).show(ui, add);
}

fn note(ui: &mut egui::Ui, text: &str, color: Color32) {
    padded(ui, |ui| {
        ui.add(egui::Label::new(RichText::new(text).size(12.0).color(color)).wrap());
    });
}

/// Set the right panel's width (as dragging its edge does).
pub fn set_width(app: &mut LightcraftApp, w: f32) {
    let w = RIGHT_WIDTH.clamp(w);
    app.ui.right_width = w;
    let _ = crate::docking::set_widths(&mut app.ui, &json!({"rightWidth": w}));
}

pub fn show(app: &mut LightcraftApp, ui: &mut egui::Ui, id: PhotoId) {
    let t = Tokens::get(ui.ctx());
    let d = app.session.develop_of(id).unwrap_or_default();
    let f = d.film.clone();

    // title + size buttons
    let (hr, _) = ui.allocate_exact_size(vec2(ui.available_width(), 46.0), Sense::hover());
    ui.painter().text(pos2(hr.left() + 24.0, hr.center().y + 2.0), Align2::LEFT_CENTER, crate::i18n::tr("Negative Lab"), t.semibold(15.0), t.text);
    padded(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(crate::i18n::tr("Panel size")).size(12.0).color(t.text_dim));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for (label, w) in SIZES.iter().rev() {
                    let on = (app.ui.right_width - RIGHT_WIDTH.clamp(*w)).abs() < 1.0;
                    if text_button(ui, &format!("filmSize{label}"), label, on)
                        .on_hover_text(crate::i18n::tr("Panel width (or drag the panel's edge)"))
                        .clicked()
                    {
                        set_width(app, *w);
                    }
                }
            });
        });
    });
    divider(ui);

    // convert
    let n = app.session.selection.ids.len();
    padded(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(8.0, 6.0);
            let label = if f.enabled { crate::i18n::tr("Converted") } else { crate::i18n::tr("Convert Negative") };
            if text_button(ui, "filmConvert", label, f.enabled)
                .on_hover_text(crate::i18n::tr("Turn this colour negative into a positive (Alt+N); click again to show the negative"))
                .clicked()
            {
                let ids = [id.0];
                let r = app.run("film.convert", json!({"ids": ids, "enabled": !f.enabled}));
                if let Err(e) = r {
                    app.toast(ui.ctx(), e);
                }
            }
            if n > 1 {
                let all = crate::i18n::tr_format!("Convert {n} Selected", n = n);
                if text_button(ui, "filmConvertAll", &all, false)
                    .on_hover_text(crate::i18n::tr("Convert every selected photo with this film stock, each measured on its own"))
                    .clicked()
                {
                    let ids: Vec<u64> = app.session.selection.ids.iter().map(|i| i.0).collect();
                    match app.run("film.convert", json!({"ids": ids, "emulsion": f.emulsion})) {
                        Ok(_) => app.toast(ui.ctx(), crate::i18n::tr_format!("{n} negatives converted", n = n)),
                        Err(e) => app.toast(ui.ctx(), e),
                    }
                }
            }
        });
    });
    if !f.enabled {
        note(
            ui,
            crate::i18n::tr(
                "Import your scans (TIFF, DNG or raw camera copies), choose the film stock below and convert. Scan with a linear profile and no automatic colour correction for the best colour.",
            ),
            t.text_dim,
        );
    }
    divider(ui);

    // film stock
    sub_title(ui, "Film Stock");
    let e = emulsions::find(&f.emulsion);
    padded(ui, |ui| {
        let r = crate::widgets::dropdown(ui, "filmEmulsion", &e.full_name(), t.font(14.0), t.text_label);
        egui::Popup::menu(&r).show(|ui| {
            ui.set_min_width(220.0);
            let mut maker = None;
            for em in EMULSIONS {
                if maker != Some(em.maker) {
                    if maker.is_some() {
                        ui.separator();
                    }
                    let m = if em.maker.is_empty() { crate::i18n::tr("Generic") } else { em.maker };
                    ui.label(RichText::new(m).size(11.0).color(t.text_dim));
                    maker = Some(em.maker);
                }
                let text = if em.iso.is_empty() { em.name.to_string() } else { format!("{}  ·  ISO {}", em.name, em.iso) };
                if ui.selectable_label(em.id == e.id, text).clicked() {
                    let r = if f.enabled {
                        app.run("film.set", json!({"emulsion": em.id}))
                    } else {
                        app.run("film.convert", json!({"ids": [id.0], "emulsion": em.id}))
                    };
                    if let Err(err) = r {
                        app.toast(ui.ctx(), err);
                    }
                }
            }
        });
    });
    note(ui, crate::i18n::tr(e.notes), t.text_label);
    note(ui, &format!("{} {}", crate::i18n::tr("Profile data:"), crate::i18n::tr(e.source)), t.text_dim);
    if f.measured.is_some() {
        note(ui, crate::i18n::tr("A chart calibration replaces this profile (see Calibration below)."), t.accent);
    }
    divider(ui);

    // conversion
    sub_title(ui, "Conversion");
    film_base_row(app, ui, &f);
    control(app, ui, &d, "film.neutralize", f.enabled);
    if f.measured.is_none() {
        control(app, ui, &d, "film.separation", f.enabled);
    }
    border_slider(app, ui, &d, f.enabled);
    padded(ui, |ui| {
        let mut auto = f.auto_exposure;
        let r = ui.add_enabled(f.enabled, egui::Checkbox::new(&mut auto, crate::i18n::tr("Automatic exposure")));
        crate::widgets::register(ui.ctx(), "check:film.autoExposure", r.rect);
        if r.on_hover_text(crate::i18n::tr("Key each frame's exposure from its own content (off: keep the calibrated exposure)")).changed() {
            let _ = app.run("film.set", json!({"autoExposure": auto}));
        }
    });
    if f.enabled
        && text_button(ui, "filmMeasure", crate::i18n::tr("Measure Again"), false)
            .on_hover_text(crate::i18n::tr("Measure the negative again (after a new crop or border)"))
            .clicked()
    {
        let _ = app.run("film.analyze", json!({}));
    }
    ui.add_space(8.0);
    divider(ui);

    // the positive: white balance and tone, the Edit panel's controls
    if f.enabled {
        section(app, ui, &d, "filmPositive", "Positive", |app, ui, d| {
            let raw = app.session.catalog.photo(id).is_some_and(|p| p.develops_raw() && !p.relative_wb());
            sub_title(ui, "White Balance");
            if raw {
                control(app, ui, d, "wb.temp", true);
                control(app, ui, d, "wb.tint", true);
            } else {
                let out = slider(ui, &REL_TEMP, k_to_rel(d.wb.temp), true, None);
                apply_slider_out(app, &REL_TEMP, out, |app, v| app.run("develop.set", json!({"control": "wb.temp", "value": rel_to_k(v)})));
                let out = slider(ui, &REL_TINT, d.wb.tint.clamp(-100.0, 100.0), true, None);
                apply_slider_out(app, &REL_TINT, out, |app, v| app.run("develop.set", json!({"control": "wb.tint", "value": v})));
            }
            sub_title(ui, "Tone");
            for c in ["light.exposure", "light.contrast", "light.highlights", "light.shadows", "light.whites", "light.blacks"] {
                control(app, ui, d, c, true);
            }
            sub_title(ui, "Color");
            for c in ["color.vibrance", "color.saturation"] {
                control(app, ui, d, c, true);
            }
            ui.add_space(8.0);
        });
        calibration(app, ui, &f);
    }
    ui.add_space(40.0);
}

/// The film base swatch, its picker and Auto.
fn film_base_row(app: &mut LightcraftApp, ui: &mut egui::Ui, f: &lightcraft_develop::Film) {
    let t = Tokens::get(ui.ctx());
    let base = f.stored_analysis().map(|a| a.base).or(f.base);
    padded(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(crate::i18n::tr("Film base")).font(t.font(13.0)).color(t.text_dim));
            let (r, resp) = ui.allocate_exact_size(vec2(36.0, 18.0), Sense::hover());
            crate::widgets::register(ui.ctx(), "filmBaseSwatch", r);
            match base {
                Some(b) => {
                    let peak = b.iter().copied().fold(1e-6f32, f32::max);
                    let [r8, g8, b8] = b.map(|v| lightcraft_color::transfer::encode_srgb8((v / peak).clamp(0.0, 1.0)));
                    ui.painter().rect_filled(r, 3.0, Color32::from_rgb(r8, g8, b8));
                    resp.on_hover_text(if f.base.is_some() { crate::i18n::tr("Picked film base") } else { crate::i18n::tr("Estimated film base") });
                }
                None => {
                    ui.painter().rect_stroke(r, 3.0, Stroke::new(1.0, t.divider), egui::StrokeKind::Inside);
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if f.base.is_some() && text_button(ui, "filmBaseAuto", crate::i18n::tr("Auto"), false).clicked() {
                    let _ = app.run("film.resetBase", json!({}));
                }
                let active = app.ui.tool == BASE_TOOL;
                if crate::widgets::icon_button(
                    ui,
                    "filmBasePicker",
                    Icon::Picker,
                    vec2(28.0, 28.0),
                    active,
                    true,
                    crate::i18n::tr("Film Base Selector: click the clear film between frames"),
                )
                .clicked()
                {
                    app.ui.tool = if active { String::new() } else { BASE_TOOL.into() };
                    if !active {
                        app.ui.view = crate::state::ViewMode::Detail;
                    }
                }
            });
        });
    });
}

/// The Border Buffer slider: measuring again when a drag ends.
fn border_slider(app: &mut LightcraftApp, ui: &mut egui::Ui, d: &DevelopSettings, enabled: bool) {
    let Some(spec) = controls::find("film.border") else { return };
    let v = controls::get(d, spec.id).unwrap_or(spec.default);
    let out = slider(ui, spec, v, enabled, None);
    // a drag measures once, when it ends; a click or a typed value at once
    let dragging = app.ui.dragging_control.as_deref() == Some(spec.id);
    let done = out.drag_stopped || out.reset || (out.value.is_some() && !out.drag_started && !dragging);
    apply_slider_out(app, spec, out, |app, v| app.run("develop.set", json!({"control": "film.border", "value": v})));
    if done && enabled {
        let _ = app.run("film.analyze", json!({}));
    }
}

/// Chart calibration: status, start (four clicks on the chart's corner patches), clear.
fn calibration(app: &mut LightcraftApp, ui: &mut egui::Ui, f: &lightcraft_develop::Film) {
    let t = Tokens::get(ui.ctx());
    sub_title(ui, "Calibration");
    if f.measured.is_some() {
        note(ui, crate::i18n::tr("Calibrated from a colour chart: this film, lab and scanner are measured."), t.accent);
    } else {
        note(
            ui,
            crate::i18n::tr(
                "For the most faithful colour, shoot a 24-patch colour chart on the roll, open its frame and calibrate: then copy the settings to the other frames.",
            ),
            t.text_dim,
        );
    }
    if app.ui.tool == CHART_TOOL {
        let k = app.ui.film_chart.len().min(3);
        note(ui, &crate::i18n::tr_format!("Click the centre of the {corner}", corner = crate::i18n::tr(CHART_CORNERS[k])), t.accent);
    }
    padded(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(8.0, 6.0);
            let on = app.ui.tool == CHART_TOOL;
            let label = if on { crate::i18n::tr("Cancel") } else { crate::i18n::tr("Calibrate from Chart…") };
            if text_button(ui, "filmCalibrate", label, on).clicked() {
                app.ui.film_chart.clear();
                app.ui.tool = if on { String::new() } else { CHART_TOOL.into() };
                if !on {
                    app.ui.view = crate::state::ViewMode::Detail;
                }
            }
            if f.measured.is_some() && text_button(ui, "filmClearCalibration", crate::i18n::tr("Clear Calibration"), false).clicked() {
                let _ = app.run("film.clearCalibration", json!({}));
            }
        });
    });
}

/// A click on the photo while a Negative Lab tool is active (normalized image coords).
/// Returns whether the click was taken.
pub fn canvas_click(app: &mut LightcraftApp, ctx: &egui::Context, x: f64, y: f64) -> bool {
    if app.ui.tool == BASE_TOOL {
        match app.run("film.pickBase", json!({"x": x, "y": y})) {
            Ok(_) => app.toast(ctx, crate::i18n::tr("Film base set")),
            Err(e) => app.toast(ctx, e),
        }
        app.ui.tool.clear();
        return true;
    }
    if app.ui.tool == CHART_TOOL {
        app.ui.film_chart.push([x, y]);
        if app.ui.film_chart.len() == 4 {
            let corners = std::mem::take(&mut app.ui.film_chart);
            app.ui.tool.clear();
            match app.run("film.calibrate", json!({"corners": corners})) {
                Ok(r) => {
                    let err = r["rmsStops"].as_f64().unwrap_or(0.0);
                    app.toast(ctx, crate::i18n::tr_format!("Film calibrated (error {err} stops)", err = format!("{err:.2}")));
                }
                Err(e) => app.toast(ctx, e),
            }
        }
        return true;
    }
    false
}

/// Markers for the chart corners clicked so far, drawn over the photo.
pub fn paint_chart_marks(app: &LightcraftApp, painter: &egui::Painter, to_screen: impl Fn(f64, f64) -> egui::Pos2) {
    if app.ui.tool != CHART_TOOL {
        return;
    }
    for (i, [x, y]) in app.ui.film_chart.iter().enumerate() {
        let p = to_screen(*x, *y);
        painter.circle_stroke(p, 9.0, Stroke::new(2.0, Color32::WHITE));
        painter.circle_stroke(p, 10.5, Stroke::new(1.0, Color32::BLACK));
        painter.text(p + vec2(12.0, -12.0), Align2::LEFT_BOTTOM, format!("{}", i + 1), egui::FontId::proportional(13.0), Color32::WHITE);
    }
}
