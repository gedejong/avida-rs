//! The visual design: colour tokens for both themes, egui styles built
//! from them, and fonts.
//!
//! The palette is deliberately quiet (cool greys with a single indigo
//! accent) so the map's viridis/cividis and Okabe-Ito colours carry the
//! data. Status colours are used only for state badges and messages.

use std::sync::Arc;

use egui::epaint::text::VariationCoords;
use egui::{
    Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Shadow, Stroke, Style,
    TextStyle, Theme, Visuals,
};

/// Colours of one theme.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub dark: bool,
    /// Window and panel background.
    pub bg: Color32,
    /// Cards, popups, the top bar.
    pub surface: Color32,
    /// Controls at rest, table stripes.
    pub control: Color32,
    pub control_hover: Color32,
    pub control_active: Color32,
    pub border: Color32,
    pub border_strong: Color32,
    pub text: Color32,
    pub text_weak: Color32,
    pub text_faint: Color32,
    pub accent: Color32,
    /// Text and icons on an accent fill.
    pub on_accent: Color32,
    /// Accent at low opacity (selected rows, chips).
    pub accent_soft: Color32,
    /// Behind the world on the map (empty cells).
    pub map_backdrop: Color32,
    /// Around the map canvas.
    pub canvas: Color32,
    pub success: Color32,
    pub warning: Color32,
    pub danger: Color32,
    pub info: Color32,
    pub shadow: Color32,
}

pub const DARK: Tokens = Tokens {
    dark: true,
    bg: Color32::from_rgb(0x14, 0x16, 0x1b),
    surface: Color32::from_rgb(0x1b, 0x1e, 0x25),
    control: Color32::from_rgb(0x25, 0x29, 0x32),
    control_hover: Color32::from_rgb(0x2e, 0x33, 0x3e),
    control_active: Color32::from_rgb(0x36, 0x3c, 0x49),
    border: Color32::from_rgb(0x2b, 0x2f, 0x39),
    border_strong: Color32::from_rgb(0x3d, 0x43, 0x50),
    text: Color32::from_rgb(0xe4, 0xe7, 0xec),
    text_weak: Color32::from_rgb(0x9b, 0xa3, 0xb0),
    text_faint: Color32::from_rgb(0x80, 0x88, 0x96),
    accent: Color32::from_rgb(0x7b, 0x8c, 0xff),
    on_accent: Color32::from_rgb(0x0d, 0x10, 0x26),
    accent_soft: Color32::from_rgba_premultiplied(0x1e, 0x23, 0x40, 0x60),
    map_backdrop: Color32::from_rgb(0x0e, 0x10, 0x14),
    canvas: Color32::from_rgb(0x11, 0x13, 0x17),
    success: Color32::from_rgb(0x4c, 0xc3, 0x8a),
    warning: Color32::from_rgb(0xf0, 0xb4, 0x4c),
    danger: Color32::from_rgb(0xf2, 0x6d, 0x6d),
    info: Color32::from_rgb(0x6c, 0xb6, 0xf2),
    shadow: Color32::from_black_alpha(110),
};

pub const LIGHT: Tokens = Tokens {
    dark: false,
    bg: Color32::from_rgb(0xf4, 0xf5, 0xf8),
    surface: Color32::from_rgb(0xff, 0xff, 0xff),
    control: Color32::from_rgb(0xec, 0xee, 0xf2),
    control_hover: Color32::from_rgb(0xe2, 0xe5, 0xeb),
    control_active: Color32::from_rgb(0xd6, 0xda, 0xe2),
    border: Color32::from_rgb(0xdc, 0xe0, 0xe7),
    border_strong: Color32::from_rgb(0xc3, 0xc9, 0xd3),
    text: Color32::from_rgb(0x1a, 0x1e, 0x26),
    text_weak: Color32::from_rgb(0x5c, 0x64, 0x72),
    text_faint: Color32::from_rgb(0x68, 0x70, 0x80),
    accent: Color32::from_rgb(0x43, 0x56, 0xe0),
    on_accent: Color32::from_rgb(0xff, 0xff, 0xff),
    accent_soft: Color32::from_rgba_premultiplied(0x2a, 0x36, 0x8c, 0x22),
    map_backdrop: Color32::from_rgb(0xe6, 0xe9, 0xef),
    canvas: Color32::from_rgb(0xfa, 0xfb, 0xfc),
    success: Color32::from_rgb(0x0f, 0x7a, 0x4c),
    warning: Color32::from_rgb(0x94, 0x59, 0x0a),
    danger: Color32::from_rgb(0xc2, 0x34, 0x34),
    info: Color32::from_rgb(0x1a, 0x69, 0xb0),
    shadow: Color32::from_black_alpha(36),
};

impl Tokens {
    pub fn for_dark_mode(dark: bool) -> Tokens {
        if dark {
            DARK
        } else {
            LIGHT
        }
    }

    /// The tokens of the theme a `Ui` is drawn in.
    pub fn of(ui: &egui::Ui) -> Tokens {
        Tokens::for_dark_mode(ui.visuals().dark_mode)
    }

    pub fn of_ctx(ctx: &egui::Context) -> Tokens {
        Tokens::for_dark_mode(ctx.global_style().visuals.dark_mode)
    }
}

/// The font family for headings and emphasis.
pub fn semibold() -> FontFamily {
    FontFamily::Name("semibold".into())
}

/// Small uppercase section labels.
pub fn caption_style() -> TextStyle {
    TextStyle::Name("caption".into())
}

/// Large numbers on stat cards.
pub fn metric_style() -> TextStyle {
    TextStyle::Name("metric".into())
}

/// The style of one theme.
pub fn style(t: &Tokens) -> Style {
    let mut style = Style {
        visuals: visuals(t),
        ..Style::default()
    };
    style.text_styles = [
        (
            TextStyle::Small,
            FontId::new(11.0, FontFamily::Proportional),
        ),
        (TextStyle::Body, FontId::new(13.5, FontFamily::Proportional)),
        (
            TextStyle::Button,
            FontId::new(13.5, FontFamily::Proportional),
        ),
        (TextStyle::Heading, FontId::new(17.0, semibold())),
        (
            TextStyle::Monospace,
            FontId::new(12.5, FontFamily::Monospace),
        ),
        (caption_style(), FontId::new(10.5, semibold())),
        (metric_style(), FontId::new(20.0, semibold())),
    ]
    .into();
    let s = &mut style.spacing;
    s.item_spacing = egui::vec2(8.0, 6.0);
    s.button_padding = egui::vec2(10.0, 4.0);
    s.interact_size = egui::vec2(36.0, 26.0);
    s.menu_margin = egui::Margin::same(6);
    s.window_margin = egui::Margin::same(14);
    s.indent = 16.0;
    s.slider_width = 140.0;
    s.combo_width = 120.0;
    s.text_edit_width = 160.0;
    s.icon_width = 15.0;
    s.icon_width_inner = 9.0;
    s.icon_spacing = 6.0;
    style.interaction.tooltip_delay = 0.35;
    style
}

fn visuals(t: &Tokens) -> Visuals {
    let mut v = if t.dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    let radius = CornerRadius::same(6);
    v.panel_fill = t.bg;
    v.window_fill = t.surface;
    v.window_stroke = Stroke::new(1.0, t.border);
    v.window_corner_radius = CornerRadius::same(12);
    v.menu_corner_radius = CornerRadius::same(10);
    v.window_shadow = Shadow {
        offset: [0, 8],
        blur: 28,
        spread: 0,
        color: t.shadow,
    };
    v.popup_shadow = Shadow {
        offset: [0, 4],
        blur: 16,
        spread: 0,
        color: t.shadow,
    };
    v.faint_bg_color = t.control;
    // Fields are inset in light mode (a white field on a white bar would
    // vanish). In dark mode the canvas colour is 1.04:1 against the panel,
    // so a field would have no visible edge; it takes the control fill.
    v.extreme_bg_color = if t.dark { t.canvas } else { t.control };
    v.text_edit_bg_color = Some(t.control);
    v.code_bg_color = t.control;
    v.hyperlink_color = t.accent;
    v.warn_fg_color = t.warning;
    v.error_fg_color = t.danger;
    v.weak_text_color = Some(t.text_weak);
    v.selection.bg_fill = t.accent.gamma_multiply(if t.dark { 0.45 } else { 0.25 });
    v.selection.stroke = Stroke::new(1.0, t.accent);
    v.striped = false;
    v.collapsing_header_frame = false;
    v.indent_has_left_vline = false;
    v.slider_trailing_fill = true;
    v.button_frame = true;

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = t.surface;
    w.noninteractive.weak_bg_fill = t.surface;
    w.noninteractive.bg_stroke = Stroke::new(1.0, t.border);
    w.noninteractive.fg_stroke = Stroke::new(1.0, t.text);
    w.noninteractive.corner_radius = radius;

    w.inactive.bg_fill = t.control;
    w.inactive.weak_bg_fill = t.control;
    w.inactive.bg_stroke = Stroke::new(1.0, Color32::TRANSPARENT);
    w.inactive.fg_stroke = Stroke::new(1.0, t.text);
    w.inactive.corner_radius = radius;
    w.inactive.expansion = 0.0;

    w.hovered.bg_fill = t.control_hover;
    w.hovered.weak_bg_fill = t.control_hover;
    w.hovered.bg_stroke = Stroke::new(1.0, t.border_strong);
    w.hovered.fg_stroke = Stroke::new(1.5, t.text);
    w.hovered.corner_radius = radius;
    w.hovered.expansion = 0.0;

    w.active.bg_fill = t.control_active;
    w.active.weak_bg_fill = t.control_active;
    w.active.bg_stroke = Stroke::new(1.0, t.accent);
    w.active.fg_stroke = Stroke::new(1.5, t.text);
    w.active.corner_radius = radius;
    w.active.expansion = 0.0;

    w.open.bg_fill = t.control_hover;
    w.open.weak_bg_fill = t.control_hover;
    w.open.bg_stroke = Stroke::new(1.0, t.border_strong);
    w.open.fg_stroke = Stroke::new(1.0, t.text);
    w.open.corner_radius = radius;
    v
}

/// Installs both themes' styles.
pub fn install_styles(ctx: &egui::Context) {
    ctx.set_style_of(Theme::Dark, style(&DARK));
    ctx.set_style_of(Theme::Light, style(&LIGHT));
}

/// Candidate system UI fonts, best first: (path, collection index,
/// variable weight for body text or 0 for a static face).
const UI_FONTS: &[(&str, u32, f32)] = &[
    ("/System/Library/Fonts/SFNS.ttf", 0, 0.0),
    ("/System/Library/Fonts/HelveticaNeue.ttc", 0, 0.0),
    ("C:\\Windows\\Fonts\\segoeui.ttf", 0, 0.0),
    ("/usr/share/fonts/truetype/inter/Inter-Regular.ttf", 0, 0.0),
    ("/usr/share/fonts/opentype/inter/Inter-Regular.otf", 0, 0.0),
    ("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf", 0, 0.0),
];

const SEMIBOLD_FONTS: &[(&str, u32)] = &[
    ("C:\\Windows\\Fonts\\seguisb.ttf", 0),
    ("/usr/share/fonts/truetype/inter/Inter-SemiBold.ttf", 0),
    ("/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf", 0),
];

const MONO_FONTS: &[(&str, u32)] = &[
    ("/System/Library/Fonts/SFNSMono.ttf", 0),
    ("/System/Library/Fonts/Menlo.ttc", 0),
    ("C:\\Windows\\Fonts\\consola.ttf", 0),
    ("/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf", 0),
];

fn first_readable<'a, T: Copy>(candidates: &'a [(&'a str, T)]) -> Option<(&'a str, T, Vec<u8>)> {
    candidates
        .iter()
        .find_map(|&(p, x)| std::fs::read(p).ok().map(|b| (p, x, b)))
}

/// Uses the platform's UI and monospace fonts when they can be found (San
/// Francisco on macOS, Segoe UI on Windows, Inter or DejaVu on Linux),
/// egui's bundled fonts otherwise, and adds the Material Symbols icon
/// font. Headings use a semibold face: a weight instance of a variable
/// font, a separate face, or the regular one.
pub fn install_fonts(ctx: &egui::Context) {
    let mut defs = FontDefinitions::default();
    let mut semibold_family: Vec<String> = Vec::new();

    let ui_font = UI_FONTS
        .iter()
        .find_map(|&(p, idx, _)| std::fs::read(p).ok().map(|b| (p, idx, b)));
    if let Some((path, index, bytes)) = ui_font {
        let blob: egui::epaint::text::Blob = Arc::new(bytes);
        let regular = FontData::from_blob(blob.clone(), index);
        defs.font_data.insert("ui".into(), Arc::new(regular));
        prepend(&mut defs, FontFamily::Proportional, "ui");
        if path.ends_with("SFNS.ttf") {
            // A variable font: the semibold instance of the same file.
            let mut semi = FontData::from_blob(blob, index);
            semi.tweak.coords = VariationCoords::new([("wght", 600.0)]);
            defs.font_data.insert("ui-semibold".into(), Arc::new(semi));
            semibold_family.push("ui-semibold".into());
        } else if let Some((_, idx, b)) = first_readable(SEMIBOLD_FONTS) {
            defs.font_data.insert(
                "ui-semibold".into(),
                Arc::new(FontData::from_blob(Arc::new(b), idx)),
            );
            semibold_family.push("ui-semibold".into());
        }
    }
    if let Some((_, idx, bytes)) = first_readable(MONO_FONTS) {
        defs.font_data.insert(
            "mono".into(),
            Arc::new(FontData::from_blob(Arc::new(bytes), idx)),
        );
        prepend(&mut defs, FontFamily::Monospace, "mono");
    }
    let proportional = defs
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    semibold_family.extend(proportional);
    defs.families.insert(semibold(), semibold_family);
    ctx.set_fonts(defs);
    egui_material_icons::initialize(ctx);
}

fn prepend(defs: &mut FontDefinitions, family: FontFamily, name: &str) {
    defs.families
        .entry(family)
        .or_default()
        .insert(0, name.to_string());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_has_enough_contrast_in_both_themes() {
        // WCAG relative luminance contrast.
        fn lum(c: Color32) -> f64 {
            let f = |u: u8| {
                let s = f64::from(u) / 255.0;
                if s <= 0.04045 {
                    s / 12.92
                } else {
                    ((s + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * f(c.r()) + 0.7152 * f(c.g()) + 0.0722 * f(c.b())
        }
        fn contrast(a: Color32, b: Color32) -> f64 {
            let (x, y) = (lum(a), lum(b));
            (x.max(y) + 0.05) / (x.min(y) + 0.05)
        }
        for t in [DARK, LIGHT] {
            assert!(contrast(t.text, t.bg) >= 7.0, "{t:?}");
            assert!(contrast(t.text_weak, t.surface) >= 4.5, "{t:?}");
            assert!(contrast(t.on_accent, t.accent) >= 4.5, "{t:?}");
            assert!(contrast(t.text, t.control) >= 7.0, "{t:?}");
            // Small print and status colours are text too (WCAG AA).
            for (name, fg) in [
                ("text_faint", t.text_faint),
                ("success", t.success),
                ("warning", t.warning),
                ("danger", t.danger),
                ("info", t.info),
            ] {
                for (on, bg) in [("bg", t.bg), ("surface", t.surface)] {
                    let c = contrast(fg, bg);
                    assert!(c >= 4.5, "{name} on {on}: {c:.2} (dark: {})", t.dark);
                }
            }
        }
    }
}
