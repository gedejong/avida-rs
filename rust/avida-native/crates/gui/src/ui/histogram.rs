//! The histogram view of the charts tab: how the values of the map's layer
//! are distributed now, with the organisms the filter selects drawn over
//! the whole population in the same bins.
//!
//! Dragging sideways over a histogram of a property (fitness, merit,
//! gestation time, genome length, age, generation, tasks performed,
//! threads) makes the dragged range a condition of the filter; a click
//! removes it. In the bar charts of genotypes and families a click on a bar
//! makes that genotype or family a condition.

use avida_session::LayerId;
use avida_viewmodel::{Histogram, Palette, Prop, Scale};
use egui::{pos2, vec2, Id, PointerButton, Rect, RichText, Ui};
use egui_material_icons::icons;
use egui_plot::{Bar, BarChart};

use crate::model::format;
use crate::model::layers::{genotype_name, layer_title};
use crate::model::AppState;
use crate::ui::charts::static_plot;
use crate::ui::theme::{semibold, Tokens};
use crate::ui::widgets::{self, rgba};

/// Histogram bins.
const BINS: usize = 24;
/// Bars of the genotype chart.
const GENOTYPE_BARS: usize = 20;

/// The value at `t` (0 to 1) along the bins.
fn value_at(h: &Histogram, t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    match h.scale {
        Scale::Linear => h.min + t * (h.max - h.min),
        Scale::Log => (h.min.ln() + t * (h.max.ln() - h.min.ln())).exp(),
    }
}

/// Where along the bins (0 to 1, clamped) a value lies.
fn position_of(h: &Histogram, v: f64) -> f64 {
    let t = match h.scale {
        Scale::Linear => (v - h.min) / (h.max - h.min),
        Scale::Log => {
            if v <= 0.0 {
                0.0
            } else {
                (v.ln() - h.min.ln()) / (h.max.ln() - h.min.ln())
            }
        }
    };
    if t.is_nan() {
        0.0
    } else {
        t.clamp(0.0, 1.0)
    }
}

/// The range a drag between plot positions `a` and `b` (in bins) selects
/// as a condition: whole numbers for whole properties.
pub fn dragged_range(h: &Histogram, prop: Prop, a: f64, b: f64) -> (f64, f64) {
    let bins = h.counts.len() as f64;
    let (lo, hi) = (a.min(b) / bins, a.max(b) / bins);
    let (mut lo, mut hi) = (value_at(h, lo), value_at(h, hi));
    if prop.is_whole() {
        let (l, u) = (lo.ceil(), hi.floor());
        if l > u {
            let mid = ((lo + hi) / 2.0).round();
            lo = mid;
            hi = mid;
        } else {
            lo = l;
            hi = u;
        }
    }
    (lo, hi)
}

fn brush_id(layer: LayerId) -> Id {
    Id::unique(("histogram-brush", layer.key()))
}

pub fn show(ui: &mut Ui, state: &mut AppState) {
    let t = Tokens::of(ui);
    let Some(frame) = state.frame.clone() else {
        widgets::empty_state(
            ui,
            icons::ICON_SHOW_CHART,
            "No frame yet",
            "The world is being built.",
        );
        return;
    };
    let layer = state.map.layer;
    let title = layer_title(&frame.env, layer);
    let sel = state.brush.selection().filter(|s| s.active).cloned();
    let prop = Prop::from_layer(layer);
    let area = ui
        .scope(|ui| {
            let Some(map) = frame.map.as_ref() else {
                ui.label(RichText::new(&title).family(semibold()).color(t.text));
                ui.label(RichText::new("No map yet.").color(t.text_weak));
                return;
            };
            let height = (ui.available_height() - 40.0).max(200.0);
            let heading = |ui: &mut Ui, explanation: &str, matching: Option<(u64, u64)>| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&title).family(semibold()).color(t.text))
                        .on_hover_text(explanation);
                    ui.label(icons::ICON_INFO.rich_text().size(13.0).color(t.text_faint))
                        .on_hover_text(explanation);
                    if let Some((n, of)) = matching {
                        let share = if of == 0 { 0.0 } else { n as f64 / of as f64 };
                        ui.label(
                            RichText::new(format!(
                                "{} matching ({})",
                                format::count(n as i64),
                                super::charts::percent_of(share)
                            ))
                            .color(t.text_weak),
                        );
                    }
                });
            };

            if layer == LayerId::Genotype {
                let rows: Vec<_> = frame.genotypes.iter().take(GENOTYPE_BARS).collect();
                let sub: Vec<u32> = rows
                    .iter()
                    .map(|g| match &sel {
                        Some(s) => map
                            .genotype_id
                            .iter()
                            .zip(&s.mask)
                            .filter(|(id, &m)| m && **id == g.id)
                            .count() as u32,
                        None => 0,
                    })
                    .collect();
                heading(
                    ui,
                    "The organisms of the most abundant genotypes. Click a bar to keep only that genotype.",
                    sel.as_ref().map(|s| (s.matched as u64, s.occupied as u64)),
                );
                let names: Vec<String> = rows.iter().map(|g| genotype_name(&frame, g.id)).collect();
                let bars: Vec<Bar> = rows
                    .iter()
                    .enumerate()
                    .map(|(i, g)| {
                        Bar::new(i as f64, f64::from(g.abundance))
                            .width(0.8)
                            .name(names[i].clone())
                            .fill(rgba(Palette::OkabeIto.category(map.genotype_slot(g.id))))
                    })
                    .collect();
                let overlay = overlay_bars(&sub, &names, t.filter);
                let labels = names.clone();
                let stride = label_stride(&names, ui.available_width());
                let r = static_plot(ui, "genotype-bars")
                    .height(height)
                    .show_grid([false, true])
                    .y_axis_min_width(40.0)
                    .x_grid_spacer(bar_marks(names.len(), stride))
                    .x_axis_formatter(move |mark, _| index_label(&labels, mark.value, stride))
                    .show(ui, |p| {
                        p.bar_chart(BarChart::new("abundance", bars));
                        if sel.is_some() {
                            p.bar_chart(BarChart::new("matching organisms", overlay));
                        }
                    });
                if r.response.clicked() {
                    if let Some(i) = clicked_bar(&r, rows.len()) {
                        let id = rows[i].id;
                        state.edit_filter(|f| f.toggle_genotype(id));
                    }
                }
                return;
            }
            if layer == LayerId::Ancestry {
                let values = map.layer(LayerId::Ancestry);
                let sub: Vec<u32> = map
                    .families
                    .iter()
                    .map(|fam| match (&sel, values) {
                        (Some(s), Some(v)) => v
                            .iter()
                            .zip(&s.mask)
                            .filter(|(v, &m)| {
                                m && map.family_of_value(**v).is_some_and(|f| f.slot == fam.slot)
                            })
                            .count() as u32,
                        _ => 0,
                    })
                    .collect();
                heading(
                    ui,
                    "The organisms in each family, as the Ancestry layer groups them. Click a bar to keep only that family.",
                    sel.as_ref().map(|s| (s.matched as u64, s.occupied as u64)),
                );
                let names: Vec<String> =
                    map.families.iter().map(|f| format!("#{}", f.root)).collect();
                let bars: Vec<Bar> = map
                    .families
                    .iter()
                    .enumerate()
                    .map(|(i, f)| {
                        Bar::new(i as f64, f64::from(f.organisms))
                            .width(0.8)
                            .name(format!("Family of {}", genotype_name(&frame, f.root)))
                            .fill(rgba(Palette::OkabeIto.category(Some(f.slot))))
                    })
                    .collect();
                let overlay = overlay_bars(&sub, &names, t.filter);
                let labels = names.clone();
                let stride = label_stride(&names, ui.available_width());
                let r = static_plot(ui, "family-bars")
                    .height(height)
                    .show_grid([false, true])
                    .y_axis_min_width(40.0)
                    .x_grid_spacer(bar_marks(names.len(), stride))
                    .x_axis_formatter(move |mark, _| index_label(&labels, mark.value, stride))
                    .show(ui, |p| {
                        p.bar_chart(BarChart::new("organisms", bars));
                        if sel.is_some() {
                            p.bar_chart(BarChart::new("matching organisms", overlay));
                        }
                    });
                if r.response.clicked() {
                    if let Some(i) = clicked_bar(&r, map.families.len()) {
                        let root = map.families[i].root;
                        state.edit_filter(|f| f.toggle_family(root));
                    }
                }
                return;
            }
            let Some(values) = map.layer(layer) else {
                heading(ui, "", None);
                ui.label(RichText::new("Waiting for this layer.").color(t.text_weak));
                return;
            };
            let scale = state.map.scale(layer);
            let hist = Histogram::new(values, BINS, scale);
            let sub = sel.as_ref().map(|s| hist.counts_where(values, &s.mask));
            let counted_sub = sub.as_ref().map(|c| c.iter().map(|&n| u64::from(n)).sum::<u64>());
            let explanation = format!(
                "{} cells counted, {} empty or out of range.{}{}",
                format::count(hist.total() as i64),
                format::count(hist.skipped as i64),
                if hist.scale == Scale::Log {
                    " The bins are logarithmic."
                } else {
                    ""
                },
                if prop.is_some() {
                    " Drag sideways over the chart to keep only organisms in that range, click to remove the range."
                } else {
                    " Add conditions with the filter bar."
                }
            );
            heading(
                ui,
                &explanation,
                counted_sub.map(|n| (n, hist.total())),
            );
            let edges = hist.bin_edges();
            // Whole-number properties label their bin edges with whole
            // numbers, as the chips and the form do.
            let whole = prop.is_some_and(Prop::is_whole);
            let edge = move |v: f64| {
                if whole {
                    format::count(v.round() as i64)
                } else {
                    format::real(v)
                }
            };
            let bars: Vec<Bar> = hist
                .counts
                .iter()
                .enumerate()
                .map(|(i, &c)| {
                    Bar::new(i as f64 + 0.5, f64::from(c))
                        .width(0.92)
                        .name(format!(
                            "{} to {}",
                            edge(edges[i]),
                            edge(edges[i + 1])
                        ))
                        .fill(t.accent.gamma_multiply(0.85))
                })
                .collect();
            let overlay: Vec<Bar> = sub
                .iter()
                .flatten()
                .enumerate()
                .map(|(i, &c)| {
                    Bar::new(i as f64 + 0.5, f64::from(c))
                        .width(0.5)
                        .name(format!(
                            "Matching: {} to {}",
                            edge(edges[i]),
                            edge(edges[i + 1])
                        ))
                        .fill(t.filter)
                })
                .collect();
            let edge_labels = edges.clone();
            let r = static_plot(ui, "layer-histogram")
                .height(height)
                .show_grid([false, true])
                .y_axis_min_width(40.0)
                .x_axis_formatter(move |mark, _| {
                    let i = mark.value.round();
                    if (mark.value - i).abs() < 1e-6 && i >= 0.0 && (i as usize) < edge_labels.len()
                    {
                        edge(edge_labels[i as usize])
                    } else {
                        String::new()
                    }
                })
                .show(ui, |p| {
                    p.bar_chart(BarChart::new("cells", bars));
                    if sub.is_some() {
                        p.bar_chart(BarChart::new("matching organisms", overlay));
                    }
                });
            let Some(prop) = prop else {
                return;
            };
            // Brushing: the range of the property, as the plot's x axis
            // counts bins.
            let bins = hist.counts.len() as f64;
            let frame_rect = *r.transform.frame();
            let painter = ui.painter().with_clip_rect(frame_rect);
            let band = |a: f64, b: f64, strong: bool| {
                let x0 = r.transform.position_from_point_x(a.min(b));
                let x1 = r.transform.position_from_point_x(a.max(b));
                let rect = Rect::from_min_max(
                    pos2(x0, frame_rect.top()),
                    pos2(x1.max(x0 + 1.0), frame_rect.bottom()),
                );
                painter.rect_filled(rect, 0.0, t.filter.gamma_multiply(if strong { 0.28 } else { 0.16 }));
                for x in [rect.left(), rect.right()] {
                    painter.line_segment(
                        [pos2(x, rect.top()), pos2(x, rect.bottom())],
                        egui::Stroke::new(1.5, t.filter),
                    );
                }
                rect
            };
            let id = brush_id(layer);
            if let Some((lo, hi)) = state.brush.filter.range_of(prop) {
                band(
                    position_of(&hist, lo) * bins,
                    position_of(&hist, hi) * bins,
                    false,
                );
            }
            let x_of = |pos: egui::Pos2| r.transform.value_from_position(pos).x.clamp(0.0, bins);
            if r.response.drag_started_by(PointerButton::Primary) {
                if let Some(p) = r.response.interact_pointer_pos() {
                    ui.data_mut(|d| d.insert_temp(id, (x_of(p), x_of(p))));
                }
            }
            if r.response.dragged_by(PointerButton::Primary) {
                if let (Some(p), Some((start, _))) = (
                    r.response.interact_pointer_pos(),
                    ui.data(|d| d.get_temp::<(f64, f64)>(id)),
                ) {
                    let cur = x_of(p);
                    ui.data_mut(|d| d.insert_temp(id, (start, cur)));
                    let rect = band(start, cur, true);
                    let (lo, hi) = dragged_range(&hist, prop, start, cur);
                    let text = if lo == hi {
                        edge(lo)
                    } else {
                        format!("{} to {}", edge(lo), edge(hi))
                    };
                    let galley = painter.layout_no_wrap(
                        text,
                        egui::FontId::proportional(12.5),
                        t.text,
                    );
                    let at = pos2(
                        (rect.center().x - galley.size().x / 2.0)
                            .clamp(frame_rect.left() + 4.0, frame_rect.right() - galley.size().x - 4.0),
                        frame_rect.top() + 4.0,
                    );
                    painter.rect_filled(
                        Rect::from_min_size(at, galley.size()).expand2(vec2(4.0, 2.0)),
                        4.0,
                        t.surface,
                    );
                    painter.galley(at, galley, t.text);
                }
            }
            if r.response.drag_stopped() {
                if let Some((start, cur)) = ui.data(|d| d.get_temp::<(f64, f64)>(id)) {
                    ui.data_mut(|d| d.remove::<(f64, f64)>(id));
                    let width = (r.transform.position_from_point_x(start)
                        - r.transform.position_from_point_x(cur))
                    .abs();
                    if width >= 3.0 {
                        let (lo, hi) = dragged_range(&hist, prop, start, cur);
                        state.edit_filter(|f| f.set(avida_viewmodel::Condition::range(prop, lo, hi)));
                    }
                }
            } else if r.response.clicked() && state.brush.filter.range_of(prop).is_some() {
                state.edit_filter(|f| f.clear_range(prop));
            }
        })
        .response
        .rect;
    // Only what is on screen (the tab scrolls).
    state.chart_area = Some(area.intersect(ui.clip_rect()));
}

/// The bar chart of the subset, narrower than the whole's bars.
fn overlay_bars(counts: &[u32], names: &[String], color: egui::Color32) -> Vec<Bar> {
    counts
        .iter()
        .enumerate()
        .map(|(i, &c)| {
            Bar::new(i as f64, f64::from(c))
                .width(0.45)
                .name(format!("Matching: {}", names[i]))
                .fill(color)
        })
        .collect()
}

/// A grid mark under every `stride`-th bar. The plot paints a label only
/// when its mark's step is wide on screen, so the marks that carry labels
/// say their step is huge (the plot draws no vertical grid lines here).
fn bar_marks(
    bars: usize,
    stride: usize,
) -> impl Fn(egui_plot::GridInput) -> Vec<egui_plot::GridMark> {
    move |_| {
        (0..bars)
            .step_by(stride.max(1))
            .map(|i| egui_plot::GridMark {
                value: i as f64,
                step_size: 1.0e4,
            })
            .collect()
    }
}

/// Label every `stride`-th bar, so that the labels do not run into each
/// other: as many as fit in `width` pixels.
fn label_stride(names: &[String], width: f32) -> usize {
    let longest = names.iter().map(|n| n.chars().count()).max().unwrap_or(1) as f32;
    let each = longest * 7.0 + 10.0;
    let room = (width - 50.0).max(each);
    let fit = ((room / each) as usize).max(1);
    names.len().div_ceil(fit).max(1)
}

fn index_label(names: &[String], v: f64, stride: usize) -> String {
    let i = v.round();
    if (v - i).abs() < 1e-6 && i >= 0.0 && (i as usize).is_multiple_of(stride.max(1)) {
        names.get(i as usize).cloned().unwrap_or_default()
    } else {
        String::new()
    }
}

/// The bar under a click, if any.
fn clicked_bar(r: &egui_plot::PlotResponse<()>, bars: usize) -> Option<usize> {
    let pos = r.response.interact_pointer_pos()?;
    let x = r.transform.value_from_position(pos).x;
    let i = x.round();
    (i >= 0.0 && (i as usize) < bars && (x - i).abs() <= 0.4).then_some(i as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bar_labels_thin_out_only_when_they_would_collide() {
        let short: Vec<String> = (0..7).map(|i| format!("#{i}")).collect();
        assert_eq!(label_stride(&short, 370.0), 1);
        let long: Vec<String> = (0..20).map(|i| format!("{i:03}-aabcd")).collect();
        let stride = label_stride(&long, 370.0);
        assert!(stride > 1 && stride < 10, "{stride}");
        assert_eq!(index_label(&long, 0.0, stride), "000-aabcd");
        assert_eq!(index_label(&long, 1.0, stride), "");
        assert_eq!(index_label(&long, stride as f64, stride), long[stride]);
        assert_eq!(index_label(&short, 3.5, 1), "");
    }

    #[test]
    fn a_drag_over_bins_becomes_a_range_of_values() {
        let values: Vec<f32> = (0..=100).map(|v| v as f32).collect();
        let h = Histogram::new(&values, 10, Scale::Linear);
        // Bins are 10 wide: from 1.5 to 4.2 bins is 15 to 42.
        let (lo, hi) = dragged_range(&h, Prop::GenomeLen, 4.2, 1.5);
        assert_eq!((lo, hi), (15.0, 42.0));
        let (lo, hi) = dragged_range(&h, Prop::Fitness, 1.5, 4.2);
        assert!((lo - 15.0).abs() < 1e-9 && (hi - 42.0).abs() < 1e-9);
        // A narrow whole-number drag still selects one value.
        let (lo, hi) = dragged_range(&h, Prop::Age, 1.01, 1.04);
        assert_eq!(lo, hi);
        // Positions beyond the plot clamp to the ends.
        let (lo, hi) = dragged_range(&h, Prop::Fitness, -3.0, 99.0);
        assert!((lo - 0.0).abs() < 1e-9 && (hi - 100.0).abs() < 1e-9);
    }

    #[test]
    fn log_bins_map_back_to_values() {
        let h = Histogram::with_range(&[1.0, 100.0], 10, 1.0, 100.0, Scale::Log);
        assert!((value_at(&h, 0.5) - 10.0).abs() < 1e-9);
        assert!((position_of(&h, 10.0) - 0.5).abs() < 1e-9);
        assert_eq!(position_of(&h, 0.0), 0.0);
        assert_eq!(position_of(&h, 1e9), 1.0);
    }
}
