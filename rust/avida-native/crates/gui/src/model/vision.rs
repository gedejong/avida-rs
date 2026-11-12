//! A preview of the map as viewers with a colour-vision deficiency see it.
//!
//! The dichromacies use the matrices of Machado, Oliveira and Fernandes,
//! "A Physiologically-based Model for Simulation of Color Vision
//! Deficiency" (IEEE TVCG 15(6), 2009) at severity 1, applied in linear
//! RGB; achromatopsia keeps only the relative luminance (Rec. 709).

/// How colours are shown.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum ColorVision {
    /// As they are.
    #[default]
    Normal,
    /// No long-wavelength (red) cones; about 1 % of men.
    Protanopia,
    /// No medium-wavelength (green) cones; about 1 % of men.
    Deuteranopia,
    /// No short-wavelength (blue) cones; rare.
    Tritanopia,
    /// No colour at all; very rare, but also how a greyscale print looks.
    Achromatopsia,
}

const PROTAN: [[f32; 3]; 3] = [
    [0.152_286, 1.052_583, -0.204_868],
    [0.114_503, 0.786_281, 0.099_216],
    [-0.003_882, -0.048_116, 1.051_998],
];
const DEUTAN: [[f32; 3]; 3] = [
    [0.367_322, 0.860_646, -0.227_968],
    [0.280_085, 0.672_501, 0.047_413],
    [-0.011_820, 0.042_940, 0.968_881],
];
const TRITAN: [[f32; 3]; 3] = [
    [1.255_528, -0.076_749, -0.178_779],
    [-0.078_411, 0.930_809, 0.147_602],
    [0.004_733, 0.691_367, 0.303_900],
];

fn to_linear(c: u8) -> f32 {
    let s = f32::from(c) / 255.0;
    if s <= 0.040_45 {
        s / 12.92
    } else {
        ((s + 0.055) / 1.055).powf(2.4)
    }
}

fn to_srgb(l: f32) -> u8 {
    let l = l.clamp(0.0, 1.0);
    let s = if l <= 0.003_130_8 {
        l * 12.92
    } else {
        1.055 * l.powf(1.0 / 2.4) - 0.055
    };
    (s * 255.0).round() as u8
}

impl ColorVision {
    pub const ALL: [ColorVision; 5] = [
        ColorVision::Normal,
        ColorVision::Protanopia,
        ColorVision::Deuteranopia,
        ColorVision::Tritanopia,
        ColorVision::Achromatopsia,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ColorVision::Normal => "Normal vision",
            ColorVision::Protanopia => "Protanopia",
            ColorVision::Deuteranopia => "Deuteranopia",
            ColorVision::Tritanopia => "Tritanopia",
            ColorVision::Achromatopsia => "Achromatopsia",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            ColorVision::Normal => "Colours as they are",
            ColorVision::Protanopia => "Red-blind (about 1 % of men)",
            ColorVision::Deuteranopia => "Green-blind (about 1 % of men)",
            ColorVision::Tritanopia => "Blue-blind (rare)",
            ColorVision::Achromatopsia => "No colour; also a greyscale print",
        }
    }

    /// One colour as this vision sees it.
    pub fn simulate(self, [r, g, b]: [u8; 3]) -> [u8; 3] {
        let m = match self {
            ColorVision::Normal => return [r, g, b],
            ColorVision::Protanopia => PROTAN,
            ColorVision::Deuteranopia => DEUTAN,
            ColorVision::Tritanopia => TRITAN,
            ColorVision::Achromatopsia => {
                let y = 0.2126 * to_linear(r) + 0.7152 * to_linear(g) + 0.0722 * to_linear(b);
                let v = to_srgb(y);
                return [v, v, v];
            }
        };
        let l = [to_linear(r), to_linear(g), to_linear(b)];
        let row = |i: usize| to_srgb(m[i][0] * l[0] + m[i][1] * l[1] + m[i][2] * l[2]);
        [row(0), row(1), row(2)]
    }

    /// An RGBA colour as this vision sees it (alpha unchanged).
    pub fn simulate_rgba(self, c: [u8; 4]) -> [u8; 4] {
        let [r, g, b] = self.simulate([c[0], c[1], c[2]]);
        [r, g, b, c[3]]
    }

    /// Converts an RGBA8 buffer in place. Colours repeat a lot on a map, so
    /// the last conversion is reused.
    pub fn apply(self, rgba: &mut [u8]) {
        if self == ColorVision::Normal {
            return;
        }
        let mut last: Option<([u8; 3], [u8; 3])> = None;
        for px in rgba.as_chunks_mut::<4>().0 {
            let src = [px[0], px[1], px[2]];
            let out = match last {
                Some((s, o)) if s == src => o,
                _ => {
                    let o = self.simulate(src);
                    last = Some((src, o));
                    o
                }
            };
            px[..3].copy_from_slice(&out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn greys_and_normal_vision_are_unchanged() {
        for v in ColorVision::ALL {
            for g in [0u8, 60, 128, 200, 255] {
                let out = v.simulate([g, g, g]);
                for c in out {
                    assert!(
                        (i32::from(c) - i32::from(g)).abs() <= 2,
                        "{v:?} {g} {out:?}"
                    );
                }
            }
        }
        assert_eq!(ColorVision::Normal.simulate([200, 10, 40]), [200, 10, 40]);
    }

    #[test]
    fn red_and_green_merge_for_the_red_green_deficiencies() {
        let red = [213, 94, 0];
        let green = [0, 158, 115];
        let dist = |a: [u8; 3], b: [u8; 3]| {
            a.iter()
                .zip(b)
                .map(|(x, y)| (i32::from(*x) - i32::from(y)).pow(2))
                .sum::<i32>()
        };
        let normal = dist(red, green);
        for v in [ColorVision::Protanopia, ColorVision::Deuteranopia] {
            let d = dist(v.simulate(red), v.simulate(green));
            assert!(d < normal / 2, "{v:?}: {d} vs {normal}");
        }
        let grey = ColorVision::Achromatopsia.simulate(red);
        assert_eq!(grey[0], grey[1]);
    }

    #[test]
    fn buffers_keep_alpha() {
        let mut buf = vec![255, 0, 0, 255, 255, 0, 0, 40, 0, 0, 0, 0];
        ColorVision::Deuteranopia.apply(&mut buf);
        assert_eq!(buf[3], 255);
        assert_eq!(buf[7], 40);
        assert_eq!(&buf[0..3], &buf[4..7]);
        assert_ne!(&buf[0..3], &[255, 0, 0]);
    }
}
