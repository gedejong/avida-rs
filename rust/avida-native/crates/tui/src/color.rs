//! Colour depth handling: truecolor where the terminal says it can, the
//! xterm 256-colour cube or the sixteen named colours otherwise, and the
//! hues of the interface for dark and for light terminal backgrounds.

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use ratatui::style::Color;

/// How many colours the terminal shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorMode {
    /// 24-bit colour.
    TrueColor,
    /// The xterm 256-colour palette (6x6x6 cube plus 24 greys).
    Indexed256,
    /// The sixteen named colours of a basic terminal (`TERM=xterm`, `linux`).
    Ansi16,
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

    fn depth_code(self) -> u8 {
        match self {
            ColorMode::TrueColor => 0,
            ColorMode::Indexed256 => 1,
            ColorMode::Ansi16 => 2,
            ColorMode::Mono => 3,
        }
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
        } else if term.contains("256") || colorterm.contains("256") {
            ColorMode::Indexed256
        } else if matches!(
            term,
            "xterm" | "linux" | "vt100" | "vt220" | "ansi" | "cons25"
        ) || term.starts_with("screen") && !term.contains("256")
        {
            ColorMode::Ansi16
        } else {
            // Unknown terminals (`alacritty`, `kitty`, `wezterm`, ...) show
            // 256 colours nowadays.
            ColorMode::Indexed256
        }
    }

    /// The colour the terminal actually shows for `rgb` in this mode.
    pub fn quantised(self, rgb: [u8; 3]) -> [u8; 3] {
        match self {
            ColorMode::TrueColor | ColorMode::Mono => rgb,
            ColorMode::Indexed256 => indexed_rgb(nearest_256(rgb)),
            ColorMode::Ansi16 => NAMED_16
                .iter()
                .min_by_key(|(_, c)| dist(rgb, *c))
                .map_or(rgb, |(_, c)| *c),
        }
    }

    /// Text colour for glyphs drawn on a background of `rgb`: black or
    /// white by the contrast against what the terminal really shows (in 16
    /// or 256 colours that is not `rgb` itself).
    pub fn ink_on(self, rgb: [u8; 3]) -> Color {
        self.color(contrast(self.quantised(rgb)))
    }

    /// The terminal colour closest to `rgb`.
    pub fn color(self, rgb: [u8; 3]) -> Color {
        match self {
            ColorMode::TrueColor => Color::Rgb(rgb[0], rgb[1], rgb[2]),
            ColorMode::Indexed256 => Color::Indexed(nearest_256(rgb)),
            ColorMode::Ansi16 => nearest_16(rgb),
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

const NAMED_16: [(Color, [u8; 3]); 16] = [
    (Color::Black, [0, 0, 0]),
    (Color::Red, [205, 0, 0]),
    (Color::Green, [0, 205, 0]),
    (Color::Yellow, [205, 205, 0]),
    (Color::Blue, [0, 0, 238]),
    (Color::Magenta, [205, 0, 205]),
    (Color::Cyan, [0, 205, 205]),
    (Color::Gray, [229, 229, 229]),
    (Color::DarkGray, [127, 127, 127]),
    (Color::LightRed, [255, 0, 0]),
    (Color::LightGreen, [0, 255, 0]),
    (Color::LightYellow, [255, 255, 0]),
    (Color::LightBlue, [92, 92, 255]),
    (Color::LightMagenta, [255, 0, 255]),
    (Color::LightCyan, [0, 255, 255]),
    (Color::White, [255, 255, 255]),
];

/// The 24-bit value of an xterm 256-colour index of the cube or grey ramp.
fn indexed_rgb(i: u8) -> [u8; 3] {
    match i {
        16..=231 => {
            let k = usize::from(i) - 16;
            [CUBE[k / 36], CUBE[k / 6 % 6], CUBE[k % 6]]
        }
        232..=255 => {
            let v = 8 + 10 * (i - 232);
            [v, v, v]
        }
        _ => [0, 0, 0],
    }
}

/// The named terminal colour closest to `rgb`.
pub fn nearest_16(rgb: [u8; 3]) -> Color {
    NAMED_16
        .iter()
        .min_by_key(|(_, c)| dist(rgb, *c))
        .map_or(Color::Reset, |(n, _)| *n)
}

/// The background of the terminal, as far as it is known. The interface
/// draws on the terminal's own background and never paints it, so the
/// foreground hues have to suit it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Background {
    Dark,
    Light,
}

impl Background {
    /// `AVIDA_TUI_THEME` (`dark` or `light`), else `COLORFGBG` (the
    /// background is its last field: 7 and 15 are light), else dark.
    pub fn detect() -> Background {
        let theme = std::env::var("AVIDA_TUI_THEME").unwrap_or_default();
        let fgbg = std::env::var("COLORFGBG").unwrap_or_default();
        Background::from_env(&theme, &fgbg)
    }

    /// The decision of [`Background::detect`] on given values.
    pub fn from_env(theme: &str, colorfgbg: &str) -> Background {
        match theme.to_ascii_lowercase().as_str() {
            "light" => return Background::Light,
            "dark" => return Background::Dark,
            _ => {}
        }
        match colorfgbg
            .rsplit(';')
            .next()
            .and_then(|b| b.parse::<u8>().ok())
        {
            Some(7 | 15) => Background::Light,
            _ => Background::Dark,
        }
    }
}

static LIGHT: AtomicBool = AtomicBool::new(false);
static DEPTH: AtomicU8 = AtomicU8::new(0);

/// Fixes the theme the hues of [`hue`] are drawn for. Called once at start
/// (and by the screen sweep).
pub fn set_theme(background: Background, mode: ColorMode) {
    LIGHT.store(background == Background::Light, Ordering::Relaxed);
    DEPTH.store(mode.depth_code(), Ordering::Relaxed);
}

/// Whether the hues are chosen for a light background.
pub fn is_light() -> bool {
    LIGHT.load(Ordering::Relaxed)
}

/// The semantic hues of the interface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hue {
    Cyan,
    Green,
    Yellow,
    Magenta,
    Red,
    LightRed,
    LightBlue,
}

impl Hue {
    pub const ALL: [Hue; 7] = [
        Hue::Cyan,
        Hue::Green,
        Hue::Yellow,
        Hue::Magenta,
        Hue::Red,
        Hue::LightRed,
        Hue::LightBlue,
    ];
}

/// The terminal colour for `h`. On a dark background these are the
/// terminal's own named colours (themes tune them for dark). On a light
/// background the named cyan, green and yellow of most terminals are
/// below 4.5:1 against white, so darker colours are chosen: 24-bit values,
/// cube entries on 256-colour terminals, and on 16-colour ones the named
/// colours that are dark in every palette.
pub fn hue(h: Hue) -> Color {
    if !LIGHT.load(Ordering::Relaxed) {
        return match h {
            Hue::Cyan => Color::Cyan,
            Hue::Green => Color::Green,
            Hue::Yellow => Color::Yellow,
            // The plain red and magenta of an xterm palette are 3.6:1 and
            // 4.5:1 on black; the bright ones read.
            Hue::Magenta => Color::LightMagenta,
            Hue::Red => Color::LightRed,
            Hue::LightRed => Color::LightRed,
            Hue::LightBlue => Color::LightBlue,
        };
    }
    match DEPTH.load(Ordering::Relaxed) {
        0 => {
            let [r, g, b] = light_rgb(h);
            Color::Rgb(r, g, b)
        }
        1 => Color::Indexed(match h {
            Hue::Cyan => 23,
            Hue::Green => 28,
            Hue::Yellow => 94,
            Hue::Magenta => 90,
            Hue::Red => 124,
            Hue::LightRed => 160,
            Hue::LightBlue => 26,
        }),
        2 => match h {
            Hue::Cyan | Hue::LightBlue => Color::Blue,
            // The named green is 2.2:1 on white in the xterm palette; black
            // is the only safe choice and the words carry the meaning.
            Hue::Green => Color::Black,
            Hue::Yellow | Hue::Magenta => Color::Magenta,
            Hue::Red | Hue::LightRed => Color::Red,
        },
        _ => Color::Reset,
    }
}

/// The 24-bit light-background values (each at least 4.5:1 on white).
pub fn light_rgb(h: Hue) -> [u8; 3] {
    match h {
        Hue::Cyan => [0, 110, 130],
        Hue::Green => [0, 120, 40],
        Hue::Yellow => [140, 95, 0],
        Hue::Magenta => [155, 0, 155],
        Hue::Red => [190, 0, 0],
        Hue::LightRed => [210, 50, 50],
        Hue::LightBlue => [30, 90, 210],
    }
}

/// Relative luminance in `[0, 1]` (Rec. 709 weights on sRGB values).
pub fn luminance(rgb: [u8; 3]) -> f64 {
    (0.2126 * f64::from(rgb[0]) + 0.7152 * f64::from(rgb[1]) + 0.0722 * f64::from(rgb[2])) / 255.0
}

/// The WCAG relative luminance of `rgb`, in `[0, 1]`.
fn wcag_luminance(rgb: [u8; 3]) -> f64 {
    let lin = |v: u8| {
        let s = f64::from(v) / 255.0;
        if s <= 0.039_28 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(rgb[0]) + 0.7152 * lin(rgb[1]) + 0.0722 * lin(rgb[2])
}

/// The WCAG contrast ratio of two colours, from 1 to 21.
pub fn contrast_ratio(a: [u8; 3], b: [u8; 3]) -> f64 {
    let (x, y) = (wcag_luminance(a), wcag_luminance(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

/// Black or white, whichever reads better on `rgb` (the larger WCAG
/// contrast ratio; the cross-over is at a relative luminance of about 0.18).
pub fn contrast(rgb: [u8; 3]) -> [u8; 3] {
    let l = wcag_luminance(rgb);
    // Black gives (l + 0.05) / 0.05, white gives 1.05 / (l + 0.05).
    if (l + 0.05) * (l + 0.05) > 0.05 * 1.05 {
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
    fn basic_terminals_get_sixteen_colours() {
        assert_eq!(ColorMode::from_env("", "xterm"), ColorMode::Ansi16);
        assert_eq!(ColorMode::from_env("", "linux"), ColorMode::Ansi16);
        assert_eq!(ColorMode::from_env("", "screen"), ColorMode::Ansi16);
        assert_eq!(
            ColorMode::from_env("", "screen-256color"),
            ColorMode::Indexed256
        );
        assert_eq!(ColorMode::from_env("", "alacritty"), ColorMode::Indexed256);
        assert_eq!(nearest_16([200, 0, 0]), Color::Red);
    }

    #[test]
    fn background_detection() {
        assert_eq!(Background::from_env("", "0;15"), Background::Light);
        assert_eq!(Background::from_env("", "15;0"), Background::Dark);
        assert_eq!(
            Background::from_env("", "default;default;7"),
            Background::Light
        );
        assert_eq!(Background::from_env("", ""), Background::Dark);
        assert_eq!(Background::from_env("light", "15;0"), Background::Light);
        assert_eq!(Background::from_env("DARK", "0;15"), Background::Dark);
    }

    #[test]
    fn ink_follows_the_quantised_background() {
        // In 16 colours a mid cyan becomes the named cyan, which black
        // reads on and white does not.
        assert_eq!(ColorMode::Ansi16.ink_on([0, 160, 190]), Color::Black);
        assert_eq!(
            ColorMode::TrueColor.ink_on([0, 0, 0]),
            Color::Rgb(255, 255, 255)
        );
    }

    #[test]
    fn light_hues_read_on_white() {
        for h in Hue::ALL {
            let r = contrast_ratio(light_rgb(h), [255, 255, 255]);
            assert!(r >= 4.5, "{h:?} is {r:.2}:1 on white");
        }
        // The 256-colour entries are cube values.
        for (idx, h) in [
            (23u8, Hue::Cyan),
            (28, Hue::Green),
            (94, Hue::Yellow),
            (90, Hue::Magenta),
            (124, Hue::Red),
            (160, Hue::LightRed),
            (26, Hue::LightBlue),
        ] {
            let i = usize::from(idx) - 16;
            let rgb = [CUBE[i / 36], CUBE[i / 6 % 6], CUBE[i % 6]];
            let r = contrast_ratio(rgb, [255, 255, 255]);
            assert!(r >= 4.5, "{h:?} (index {idx}) is {r:.2}:1 on white");
        }
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
        // Neither black nor white reaches 4.5:1 on a mid orange; the better
        // of the two is chosen.
        assert_eq!(contrast([213, 94, 0]), [0, 0, 0]);
        assert!(
            contrast_ratio([0, 0, 0], [213, 94, 0]) > contrast_ratio([255, 255, 255], [213, 94, 0])
        );
    }
}
