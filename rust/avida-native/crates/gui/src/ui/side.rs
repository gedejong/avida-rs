//! The side panel: a tab strip over the stats, charts, genotypes,
//! organism, environment and config views.

use egui::{vec2, Role, Sense, Stroke, Ui};

use crate::model::metrics::SeriesCache;
use crate::model::{AppState, SideTab};
use crate::ui::theme::{semibold, Tokens};
use crate::ui::widgets::{self, name_widget};
use crate::ui::{charts, config, environment, genotypes, inspector, stats};

pub fn show(ui: &mut Ui, state: &mut AppState, cache: &mut SeriesCache) {
    contained(ui, |ui| content(ui, state, cache));
}

/// Runs `add` in the space left, clipped to it, and claims exactly that
/// space: content that comes out a few points too wide must not widen a
/// resizable panel (it would grow a little every frame).
pub fn contained(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    let rect = ui.available_rect_before_wrap();
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    child.shrink_clip_rect(rect);
    add(&mut child);
    ui.advance_cursor_after_rect(rect);
}

fn content(ui: &mut Ui, state: &mut AppState, cache: &mut SeriesCache) {
    tab_strip(ui, state);
    ui.add_space(8.0);
    match state.side_tab {
        // Charts size their plots to the panel and scroll only if needed.
        SideTab::Charts => {
            egui::ScrollArea::vertical()
                .id_salt("side-charts")
                .auto_shrink([false, false])
                .show(ui, |ui| charts::show(ui, state, cache));
        }
        // These fill the panel and scroll their own long lists.
        SideTab::Organism if state.is_playback() => widgets::empty_state(
            ui,
            egui_material_icons::icons::ICON_BIOTECH,
            "Organisms are not recorded",
            "A playback shows the map, the charts and the genotype table. \
             Open the experiment itself to inspect an organism or step its CPU.",
        ),
        SideTab::Config if state.is_playback() => widgets::empty_state(
            ui,
            egui_material_icons::icons::ICON_TUNE,
            "A playback has no settings",
            "The recorded run's settings are not part of the recording, and nothing \
             here can change them.",
        ),
        SideTab::Organism => inspector::show(ui, state),
        SideTab::Config => config::show(ui, state),
        tab => {
            egui::ScrollArea::vertical()
                .id_salt(("side", tab))
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.add_space(2.0);
                    match tab {
                        SideTab::Stats => stats::show(ui, state),
                        SideTab::Genotypes => genotypes::show(ui, state),
                        SideTab::Environment => environment::show(ui, state),
                        SideTab::Charts | SideTab::Organism | SideTab::Config => {}
                    }
                    ui.add_space(8.0);
                });
        }
    }
}

/// A tab's icon, shown alone for the other tabs when the strip is too
/// narrow for every title.
fn tab_icon(tab: SideTab) -> egui_material_icons::MaterialIcon {
    use egui_material_icons::icons;
    match tab {
        SideTab::Stats => icons::ICON_INSIGHTS,
        SideTab::Charts => icons::ICON_SHOW_CHART,
        SideTab::Genotypes => icons::ICON_HUB,
        SideTab::Organism => icons::ICON_BIOTECH,
        SideTab::Environment => icons::ICON_PUBLIC,
        SideTab::Config => icons::ICON_TUNE,
    }
}

/// How the tab strip fits: every title (with roomy or tight padding), or
/// only the selected tab's title and icons for the rest. Titles are never
/// abbreviated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Strip {
    Titles { pad: u8 },
    Icons,
}

const ICON_W: f32 = 18.0;

fn tab_strip(ui: &mut Ui, state: &mut AppState) {
    let t = Tokens::of(ui);
    let font = egui::FontId::new(13.5, semibold());
    let title_w = |ui: &Ui, tab: SideTab| -> f32 {
        ui.painter()
            .layout_no_wrap(tab.title().to_string(), font.clone(), t.text)
            .size()
            .x
    };
    let titles: f32 = SideTab::ALL.iter().map(|&tab| title_w(ui, tab)).sum();
    let n = SideTab::ALL.len() as f32;
    let avail = ui.available_width();
    let mode = if titles + n * 20.0 <= avail {
        Strip::Titles { pad: 18 }
    } else if titles + n * 10.0 <= avail {
        Strip::Titles { pad: 8 }
    } else {
        Strip::Icons
    };
    let (strip, _) = ui.allocate_exact_size(vec2(avail, 34.0), Sense::hover());
    ui.painter().hline(
        strip.x_range(),
        strip.bottom() - 0.5,
        Stroke::new(1.0, t.border),
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(strip)
            .layout(egui::Layout::left_to_right(egui::Align::Max)),
    );
    child.spacing_mut().item_spacing.x = 2.0;
    for tab in SideTab::ALL {
        let selected = state.side_tab == tab;
        let show_title = selected || matches!(mode, Strip::Titles { .. });
        let show_icon = mode == Strip::Icons;
        let pad = match mode {
            Strip::Titles { pad } => f32::from(pad),
            Strip::Icons => 14.0,
        };
        let mut w = pad;
        if show_title {
            w += title_w(&child, tab);
        }
        if show_icon {
            w += ICON_W + if show_title { 4.0 } else { 0.0 };
        }
        let (rect, response) = child.allocate_exact_size(vec2(w, 32.0), Sense::click());
        name_widget(&response, Role::Tab, tab.title());
        let color = if selected { t.text } else { t.text_weak };
        let painter = child.painter();
        if response.hovered() && !selected {
            painter.rect_filled(rect.shrink2(vec2(0.0, 3.0)), 6.0, t.control);
        }
        if response.has_focus() {
            painter.rect_stroke(
                rect.shrink2(vec2(0.0, 3.0)),
                6.0,
                Stroke::new(1.5, t.accent),
                egui::StrokeKind::Inside,
            );
        }
        let mut x = rect.left() + pad / 2.0;
        if show_icon {
            let icon = tab_icon(tab);
            painter.text(
                egui::pos2(x, rect.center().y),
                egui::Align2::LEFT_CENTER,
                icon.codepoint,
                egui::FontId::new(ICON_W, icon.font_family()),
                color,
            );
            x += ICON_W + 4.0;
        }
        if show_title {
            painter.text(
                egui::pos2(x, rect.center().y),
                egui::Align2::LEFT_CENTER,
                tab.title(),
                if selected {
                    font.clone()
                } else {
                    egui::FontId::proportional(13.5)
                },
                color,
            );
        }
        if selected {
            let r = egui::Rect::from_min_max(
                egui::pos2(rect.left() + 6.0, rect.bottom() - 2.5),
                egui::pos2(rect.right() - 6.0, rect.bottom() + 0.5),
            );
            painter.rect_filled(r, 1.5, t.accent);
        }
        let response = if show_title {
            response
        } else {
            response.on_hover_text(tab.title())
        };
        if response.clicked() {
            state.side_tab = tab;
        }
    }
}
