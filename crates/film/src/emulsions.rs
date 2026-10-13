//! Emulsion profiles: how each colour negative film turns dye density back into light.
//!
//! A profile holds what the inversion needs to know about a film once its base (orange mask) is
//! divided out:
//! - `gamma`: slope of each layer's characteristic curve (density per log10 exposure) on its
//!   straight-line portion. The green layer's slope sets the overall contrast of a conversion; the
//!   ratios between layers are the film's own colour balance across the tonal range, used as such
//!   when [`crate::Params::neutralize`] is 0.
//! - `grey`: density above base of each layer for an 18 % grey card exposed at box speed (where the
//!   neutral sits when the conversion trusts the profile rather than the image).
//! - `crosstalk`: unwanted absorption of each dye in the other channels (rows: measured channel
//!   R, G, B; columns: cyan, magenta, yellow dye). Undoing it restores the colour separation a
//!   broadband camera or scanner loses against a print paper's narrow sensitivities.
//! - `saturation`: the colourfulness of the lab rendition the film is known for (1 = as measured).
//!
//! The figures are our own readings of published characteristic curves where a manufacturer
//! publishes them (`source`), and estimates from the manufacturer's description and typical scans
//! where none is published. They are starting points: a scanner's channels are not Status M, so the
//! defaults neutralize the image ([`crate::Params::neutralize`]), and a chart calibration
//! ([`crate::calibrate`]) measures the film, the lab and the scanner together.
//!
//! The toe and shoulder of each film are not modelled: the inversion is linear in density, so they
//! stay in the converted image as they would in a print (the soft shadows of Portra, the crushed
//! blacks of Metropolis are the film's, not the converter's).

use serde::Serialize;

/// One film stock.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Emulsion {
    /// Stable identifier stored in develop settings (`kodak.portra400`).
    pub id: &'static str,
    /// Display name.
    pub name: &'static str,
    pub maker: &'static str,
    /// Nominal speed (shown in the picker).
    pub iso: &'static str,
    /// Straight-line slope of the R, G, B layers (density per log10 H).
    pub gamma: [f32; 3],
    /// Density above base of R, G, B for an 18 % grey at box speed.
    pub grey: [f32; 3],
    /// Dye crosstalk, rows = channel R/G/B, columns = cyan/magenta/yellow dye; diagonal 1.
    pub crosstalk: [[f32; 3]; 3],
    /// Lab rendition saturation (1 = neutral).
    pub saturation: f32,
    /// Where the numbers come from.
    pub source: &'static str,
    /// What to expect, in the user's words.
    pub notes: &'static str,
}

/// Identifier of the generic profile (used when a stored id is unknown).
pub const GENERIC: &str = "generic.c41";

const fn xt(rg: f32, rb: f32, gr: f32, gb: f32, br: f32, bg: f32) -> [[f32; 3]; 3] {
    [[1.0, rg, rb], [gr, 1.0, gb], [br, bg, 1.0]]
}

/// Every built-in emulsion, in picker order.
pub const EMULSIONS: &[Emulsion] = &[
    Emulsion {
        id: GENERIC,
        name: "Generic C-41",
        maker: "",
        iso: "",
        gamma: [0.60, 0.62, 0.64],
        grey: [0.75, 0.80, 0.85],
        crosstalk: xt(0.06, 0.02, 0.08, 0.06, 0.03, 0.10),
        saturation: 1.0,
        source: "Typical C-41 colour negative (no specific stock)",
        notes: "Neutral starting point for any C-41 film without its own profile.",
    },
    Emulsion {
        id: "kodak.portra400",
        name: "Portra 400",
        maker: "Kodak",
        iso: "400",
        gamma: [0.56, 0.58, 0.62],
        grey: [0.70, 0.76, 0.82],
        crosstalk: xt(0.05, 0.02, 0.07, 0.05, 0.03, 0.10),
        saturation: 0.96,
        source: "Kodak publication E-4050 (Status M characteristic curves), read approximately",
        notes: "Low contrast, very wide latitude, soft warm skin tones and restrained saturation.",
    },
    Emulsion {
        id: "kodak.portra160",
        name: "Portra 160",
        maker: "Kodak",
        iso: "160",
        gamma: [0.55, 0.57, 0.60],
        grey: [0.68, 0.74, 0.80],
        crosstalk: xt(0.05, 0.02, 0.07, 0.05, 0.03, 0.09),
        saturation: 0.94,
        source: "Kodak publication E-4051 (Status M characteristic curves), read approximately",
        notes: "Finest grain of the Portra line, the most muted colour.",
    },
    Emulsion {
        id: "kodak.portra800",
        name: "Portra 800",
        maker: "Kodak",
        iso: "800",
        gamma: [0.58, 0.60, 0.64],
        grey: [0.72, 0.78, 0.84],
        crosstalk: xt(0.06, 0.02, 0.08, 0.06, 0.03, 0.11),
        saturation: 1.0,
        source: "Kodak publication E-4040 (Status M characteristic curves), read approximately",
        notes: "Slightly more contrast and saturation than Portra 400.",
    },
    Emulsion {
        id: "kodak.ektar100",
        name: "Ektar 100",
        maker: "Kodak",
        iso: "100",
        gamma: [0.68, 0.70, 0.73],
        grey: [0.80, 0.86, 0.92],
        crosstalk: xt(0.10, 0.03, 0.12, 0.08, 0.04, 0.16),
        saturation: 1.10,
        source: "Kodak publication E-4046 (Status M characteristic curves), read approximately",
        notes: "Vivid, saturated colour and higher contrast; reds and blues run strong.",
    },
    Emulsion {
        id: "kodak.gold200",
        name: "Gold 200",
        maker: "Kodak",
        iso: "200",
        gamma: [0.62, 0.64, 0.66],
        grey: [0.76, 0.81, 0.86],
        crosstalk: xt(0.07, 0.02, 0.09, 0.06, 0.03, 0.12),
        saturation: 1.04,
        source: "Kodak publication E-7022 (Status M characteristic curves), read approximately",
        notes: "Warm, golden consumer film.",
    },
    Emulsion {
        id: "kodak.ultramax400",
        name: "UltraMax 400",
        maker: "Kodak",
        iso: "400",
        gamma: [0.63, 0.65, 0.68],
        grey: [0.77, 0.82, 0.88],
        crosstalk: xt(0.08, 0.02, 0.10, 0.07, 0.03, 0.13),
        saturation: 1.06,
        source: "Kodak publication E-7023 (Status M characteristic curves), read approximately",
        notes: "Punchy consumer colour, slightly warm.",
    },
    Emulsion {
        id: "fuji.c200",
        name: "Fujicolor 200",
        maker: "Fujifilm",
        iso: "200",
        gamma: [0.60, 0.62, 0.63],
        grey: [0.74, 0.79, 0.84],
        crosstalk: xt(0.06, 0.02, 0.09, 0.06, 0.03, 0.11),
        saturation: 1.0,
        source: "Typical Fujicolor consumer negative, estimated",
        notes: "Cooler greens than the Kodak consumer films.",
    },
    Emulsion {
        id: "cinestill.800t",
        name: "CineStill 800T",
        maker: "CineStill",
        iso: "800 (tungsten)",
        gamma: [0.58, 0.60, 0.62],
        grey: [0.72, 0.78, 0.84],
        crosstalk: xt(0.06, 0.02, 0.08, 0.06, 0.03, 0.10),
        saturation: 1.0,
        source: "Motion picture stock without rem-jet, estimated",
        notes: "Balanced for 3200 K tungsten light: daylight scenes come out blue unless warmed with Temp.",
    },
    Emulsion {
        id: "lomography.cn400",
        name: "Color Negative 400",
        maker: "Lomography",
        iso: "400",
        gamma: [0.62, 0.64, 0.66],
        grey: [0.76, 0.81, 0.86],
        crosstalk: xt(0.07, 0.02, 0.09, 0.06, 0.03, 0.12),
        saturation: 1.04,
        source: "Lomography does not publish curves; estimated from typical scans",
        notes: "Saturated, slightly warm everyday film.",
    },
    Emulsion {
        id: "lomography.metropolis",
        name: "LomoChrome Metropolis",
        maker: "Lomography",
        iso: "100–400",
        gamma: [0.70, 0.68, 0.66],
        grey: [0.82, 0.84, 0.86],
        crosstalk: xt(0.03, 0.01, 0.04, 0.03, 0.01, 0.05),
        saturation: 0.80,
        source: "Lomography does not publish curves; estimated from the maker's description and typical lab scans",
        notes: "Muted, desaturated colour with strong contrast and deep blacks; yellows lean olive, reds stay dense. \
                Calibrate with a chart for the closest match to your lab.",
    },
    Emulsion {
        id: "lomography.purple",
        name: "LomoChrome Purple",
        maker: "Lomography",
        iso: "100–400",
        gamma: [0.60, 0.60, 0.58],
        grey: [0.74, 0.76, 0.78],
        crosstalk: xt(0.05, 0.02, 0.06, 0.05, 0.02, 0.08),
        saturation: 1.0,
        source: "Lomography does not publish curves; estimated from typical lab scans",
        notes: "The colour shift is built into the film's layers: a normal inversion turns greens purple, blues \
                teal-green and yellows pink. ISO 100 gives the strongest shift, 400 the softest. Keep Neutralize \
                below 100 if the purple fades.",
    },
];

/// The emulsion with `id`, or the generic profile.
pub fn find(id: &str) -> &'static Emulsion {
    EMULSIONS.iter().find(|e| e.id == id).unwrap_or(&GENERIC_EMULSION)
}

/// Whether `id` names a built-in emulsion.
pub fn exists(id: &str) -> bool {
    EMULSIONS.iter().any(|e| e.id == id)
}

const GENERIC_EMULSION: Emulsion = EMULSIONS[0];

impl Emulsion {
    /// "Kodak Portra 400" (maker first when there is one).
    pub fn full_name(&self) -> String {
        if self.maker.is_empty() { self.name.to_string() } else { format!("{} {}", self.maker, self.name) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_generic_is_first() {
        assert_eq!(EMULSIONS[0].id, GENERIC);
        for (i, a) in EMULSIONS.iter().enumerate() {
            assert!(EMULSIONS[i + 1..].iter().all(|b| b.id != a.id), "duplicate id {}", a.id);
        }
    }

    #[test]
    fn profiles_are_physically_plausible() {
        for e in EMULSIONS {
            for c in 0..3 {
                assert!((0.3..1.2).contains(&e.gamma[c]), "{} gamma", e.id);
                assert!((0.3..1.5).contains(&e.grey[c]), "{} grey", e.id);
                assert_eq!(e.crosstalk[c][c], 1.0, "{} diagonal", e.id);
                for d in 0..3 {
                    assert!((0.0..0.3).contains(&e.crosstalk[c][d]) || c == d, "{} crosstalk", e.id);
                }
            }
            assert!((0.5..1.5).contains(&e.saturation), "{}", e.id);
        }
    }

    #[test]
    fn unknown_ids_fall_back_to_generic() {
        assert_eq!(find("no.such.film").id, GENERIC);
        assert_eq!(find("lomography.purple").name, "LomoChrome Purple");
        assert!(exists("kodak.portra400") && !exists("x"));
    }
}
