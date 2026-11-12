//! Colour depth handling: truecolor where the terminal says it can, the
//! xterm 256-colour cube otherwise.

use ratatui::style::Color;

/// How many colours the terminal shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorMode {
    /// 24-bit colour.
    TrueColor,
    /// The xterm 256-colour palette (6x6x6 cube plus 24 greys).
    Indexed256,
    /// No colour (`NO_COLOR`, or `--colors none`): attributes and shading
    /// characters carry the information instead.
    Mono,
}

impl ColorMode {
    /// Reads `COLORTERM` and `TERM`: truecolor when `COLORTERM` says
    /// `truecolor` or `24bit`, or `TERM` names a direct-colour terminal.
    pub fn detect() -> ColorMode {
        let get = |k: &str| std::env::var(k).unwrap_or_default().to_ascii_lowercase();
        // https://no-color.org: any non-empty value asks for no colour.
        if std::env::var("NO_COLOR").is_ok_and(|v| !v.is_empty()) {
            return ColorMode::Mono;
        }
        ColorMode::from_env(&get("COLORTERM"), &get("TERM"))
    }

    /// Whether the terminal's own colour names are to be avoided.
    pub fn is_mono(self) -> bool {
        self == ColorMode::Mono
    }

    /// The decision of [`ColorMode::detect`] on given values.
    pub fn from_env(colorterm: &str, term: &str) -> ColorMode {
        if colorterm.contains("truecolor")
            || colorterm.contains("24bit")
            || term.contains("direct")
            || term.contains("truecolor")
        {
            ColorMode::TrueColor
        } else {
            ColorMode::Indexed256
        }
    }

    /// The terminal colour closest to `rgb`.
    pub fn color(self, rgb: [u8; 3]) -> Color {
        match self {
            ColorMode::TrueColor => Color::Rgb(rgb[0], rgb[1], rgb[2]),
            ColorMode::Indexed256 => Color::Indexed(nearest_256(rgb)),
            ColorMode::Mono => Color::Reset,
        }
    }
}

const CUBE: [u8; 6] = [0, 95, 135, 175, 215, 255];

fn dist(a: [u8; 3], b: [u8; 3]) -> i32 {
    // Weighted towards green, as the eye is.
    let d = |i: usize| i32::from(a[i]) - i32::from(b[i]);
    2 * d(0) * d(0) + 4 * d(1) * d(1) + 3 * d(2) * d(2)
}

/// The index (16..=255) of the xterm colour closest to `rgb`.
pub fn nearest_256(rgb: [u8; 3]) -> u8 {
    let level = |v: u8| {
        (0..6usize)
            .min_by_key(|&i| (i32::from(CUBE[i]) - i32::from(v)).abs())
            .unwrap_or(0)
    };
    let (r, g, b) = (level(rgb[0]), level(rgb[1]), level(rgb[2]));
    let cube = [CUBE[r], CUBE[g], CUBE[b]];
    let cube_idx = 16 + 36 * r + 6 * g + b;
    let avg = (u32::from(rgb[0]) + u32::from(rgb[1]) + u32::from(rgb[2])) / 3;
    let grey_i = ((avg.saturating_sub(8) + 5) / 10).min(23) as usize;
    let gv = (8 + 10 * grey_i) as u8;
    let grey = [gv, gv, gv];
    if dist(rgb, grey) < dist(rgb, cube) {
        (232 + grey_i) as u8
    } else {
        cube_idx as u8
    }
}

/// Relative luminance in `[0, 1]` (Rec. 709 weights on sRGB values).
pub fn luminance(rgb: [u8; 3]) -> f64 {
    (0.2126 * f64::from(rgb[0]) + 0.7152 * f64::from(rgb[1]) + 0.0722 * f64::from(rgb[2])) / 255.0
}

/// Black on light colours, white on dark ones.
pub fn contrast(rgb: [u8; 3]) -> [u8; 3] {
    if luminance(rgb) > 0.55 {
        [0, 0, 0]
    } else {
        [255, 255, 255]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detection() {
        assert_eq!(
            ColorMode::from_env("truecolor", "xterm"),
            ColorMode::TrueColor
        );
        assert_eq!(ColorMode::from_env("24bit", ""), ColorMode::TrueColor);
        assert_eq!(
            ColorMode::from_env("", "xterm-256color"),
            ColorMode::Indexed256
        );
        assert_eq!(
            ColorMode::from_env("", "xterm-direct"),
            ColorMode::TrueColor
        );
    }

    #[test]
    fn mono_has_no_colours() {
        assert_eq!(ColorMode::Mono.color([200, 10, 10]), Color::Reset);
        assert!(ColorMode::Mono.is_mono());
        assert!(!ColorMode::TrueColor.is_mono());
    }

    #[test]
    fn nearest_hits_cube_corners_and_greys() {
        assert_eq!(nearest_256([0, 0, 0]), 16);
        assert_eq!(nearest_256([255, 255, 255]), 231);
        assert_eq!(nearest_256([255, 0, 0]), 196);
        let g = nearest_256([128, 128, 128]);
        assert!((232..=255).contains(&g), "{g}");
        let g = nearest_256([28, 28, 34]);
        assert!((232..=255).contains(&g), "{g}");
    }

    #[test]
    fn contrast_flips_with_luminance() {
        assert_eq!(contrast([250, 250, 100]), [0, 0, 0]);
        assert_eq!(contrast([68, 1, 84]), [255, 255, 255]);
    }
}
