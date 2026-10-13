//! Make a colour negative "scan" from a positive image, through an emulsion profile: for trying the
//! converter without a film scanner, and for round-trip checks.
//!
//! cargo run -p lightcraft-film --example make_negative -- positive.jpg negative.tif [emulsion-id]
//!
//! The output is a 16-bit sRGB-encoded TIFF of the film's transmittance, orange mask included, as a
//! scanner with colour correction off writes it.

use lightcraft_film::emulsions;

const ORANGE: [f32; 3] = [0.86, 0.52, 0.30];
/// Softness of the toe (density).
const TOE: f32 = 0.08;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let (Some(input), Some(output)) = (args.get(1), args.get(2)) else {
        return Err("usage: make_negative <positive> <negative.tif> [emulsion-id]".into());
    };
    let em = emulsions::find(args.get(3).map(String::as_str).unwrap_or("kodak.portra400"));
    let img = image::open(input)?.to_rgb16();
    let (w, h) = img.dimensions();
    let lin = |v: u16| {
        let v = f32::from(v) / 65535.0;
        if v <= 0.040_45 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    };
    let enc = |v: f32| {
        let v = v.clamp(0.0, 1.0);
        let e = if v <= 0.003_130_8 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 };
        (e * 65535.0).round() as u16
    };
    // a 4 % unexposed rebate around the frame, as a holder shows it
    let (bx, by) = (w / 25, h / 25);
    let mut out = image::ImageBuffer::<image::Rgb<u16>, Vec<u16>>::new(w + 2 * bx, h + 2 * by);
    for (x, y, p) in out.enumerate_pixels_mut() {
        let t: [f32; 3] = if x < bx || y < by || x >= w + bx || y >= h + by {
            ORANGE
        } else {
            let s = img.get_pixel(x - bx, y - by).0.map(lin);
            // scene → layer densities (straight line with a soft toe: film never gets clearer than
            // its base) → crosstalk → transmittance
            let a: [f32; 3] = std::array::from_fn(|c| {
                let d = em.grey[c] + em.gamma[c] * (s[c].max(1e-4) / 0.18).log10();
                TOE * (1.0 + (d / TOE).exp()).ln()
            });
            let x = em.crosstalk;
            std::array::from_fn(|c| ORANGE[c] * 10f32.powf(-(x[c][0] * a[0] + x[c][1] * a[1] + x[c][2] * a[2])))
        };
        *p = image::Rgb(t.map(enc));
    }
    out.save(output)?;
    println!("{output}: {} × {} negative through {}", out.width(), out.height(), em.full_name());
    Ok(())
}
