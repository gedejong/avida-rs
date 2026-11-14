//! The filter bar at the top of the Charts tab: the active conditions as
//! removable chips, the "Add condition" form, "Clear", and how many
//! organisms match.

use avida_viewmodel::{Condition, Prop};
use egui::{pos2, vec2, Align, Margin, Rect, RichText, Role, Sense, Stroke, Ui};
use egui_material_icons::icons;

use crate::model::filtering::{
    condition_label, condition_sentence, task_name, Comparison, Draft, DraftKind,
};
use crate::model::layers::genotype_name;
use crate::model::{format, AppState};
use crate::ui::theme::{semibold, Tokens};
use crate::ui::widgets::{self, name_widget};

/// What hovering the bar's icon says.
const HELP: &str = "Show only some organisms. Conditions are joined by AND: an organism must meet all of them. \
The map dims the others, the histogram and task shares compare the matching organisms with everyone, \
and the scatter plot and time series emphasise them. \
Drag over a histogram or the scatter plot, click a task, or drag a region on the map to add conditions.";

/// "N of M organisms match (P%)".
pub fn summary(matched: usize, occupied: usize) -> String {
    let share = if occupied == 0 {
        0.0
    } else {
        matched as f64 / occupied as f64 * 100.0
    };
    let p = if share >= 10.0 || share == 0.0 {
        format!("{share:.0}")
    } else {
        format!("{share:.1}")
    };
    format!(
        "{} of {} organisms match ({p}%)",
        format::count(matched as i64),
        format::count(occupied as i64)
    )
}

pub fn show(ui: &mut Ui, state: &mut AppState) {
    let t = Tokens::of(ui);
    state.brush.add_open = false;
    let frame = state.frame.clone();
    let conds: Vec<Condition> = state.brush.filter.conditions().to_vec();
    let sel = state.brush.selection().cloned();
    let active = !conds.is_empty();
    let mut remove = None;
    let mut clear = false;
    state.brush.add_open = state.brush.form_open;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(6.0, 4.0);
        ui.label(
            icons::ICON_FILTER_ALT
                .rich_text()
                .size(17.0)
                .color(if active { t.filter } else { t.text_faint }),
        )
        .on_hover_text(HELP);
        for (i, c) in conds.iter().enumerate() {
            let ignored = sel.as_ref().is_some_and(|s| s.ignored.contains(&i));
            let label = condition_label(c, frame.as_deref());
            // A chip is one block: start a new line when it does not fit.
            let width = widgets::text_width(ui, RichText::new(&label).size(12.5))
                + 40.0
                + if ignored { 18.0 } else { 0.0 };
            // (`available_width` counts the next line in a wrapped layout.)
            if ui.max_rect().right() - ui.cursor().left() < width {
                ui.end_row();
            }
            if chip(
                ui,
                &t,
                &label,
                &condition_sentence(c, frame.as_deref()),
                ignored,
            ) {
                remove = Some(i);
            }
        }
        add_menu(ui, state, &t);
    });
    if state.brush.form_open {
        add_form(ui, state, &t);
    }
    // The count and "Clear" share a row: in the chip row "Clear" fell onto
    // a line of its own whenever the chips filled it.
    if active {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            if let Some(s) = sel.as_ref() {
                ui.label(RichText::new(summary(s.matched, s.occupied)).color(t.text_weak));
            }
            if ui
                .add(egui::Button::new(RichText::new("Clear").color(t.text_weak)).frame(false))
                .on_hover_text("Remove every condition")
                .clicked()
            {
                clear = true;
            }
        });
        if sel
            .as_ref()
            .is_some_and(|s| s.matched == 0 && s.occupied > 0)
        {
            ui.label(
                RichText::new(
                    "No organism meets all the conditions, so the charts have nothing to compare. Remove a condition to see matches.",
                )
                .small()
                .color(t.text_weak),
            );
        }
    }
    if clear {
        state.clear_filter();
    } else if let Some(i) = remove {
        state.edit_filter(|f| f.remove(i));
    }
}

/// One condition. Returns whether its remove button was clicked.
fn chip(ui: &mut Ui, t: &Tokens, label: &str, sentence: &str, ignored: bool) -> bool {
    let mut removed = false;
    let r = egui::Frame::new()
        .fill(t.filter_soft)
        .stroke(Stroke::new(1.0, t.filter.gamma_multiply(0.7)))
        .corner_radius(11.0)
        .inner_margin(Margin {
            left: 9,
            right: 2,
            top: 0,
            bottom: 0,
        })
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            ui.spacing_mut().interact_size.y = 20.0;
            ui.horizontal(|ui| {
                if ignored {
                    ui.label(icons::ICON_WARNING.rich_text().size(13.0).color(t.warning));
                }
                ui.label(RichText::new(label).size(12.5).color(t.text));
                removed = widgets::icon_button_sized(
                    ui,
                    icons::ICON_CLOSE,
                    &format!("Remove condition: {label}"),
                    "Remove this condition",
                    13.0,
                    false,
                )
                .clicked();
            });
        })
        .response;
    let tip = if ignored {
        format!(
            "{sentence} It is not applied: this frame does not carry the values it reads (a recording keeps only some layers)."
        )
    } else {
        sentence.to_string()
    };
    r.on_hover_text(tip);
    removed
}

/// The genotype the filter's "Genotype" option refers to: the highlighted
/// one, else the occupant of the inspected cell.
fn selected_genotype(state: &AppState) -> Option<i32> {
    state.highlight.or_else(|| {
        let cell = usize::try_from(state.selection?).ok()?;
        let g = *state.frame.as_ref()?.map.as_ref()?.genotype_id.get(cell)?;
        (g >= 0).then_some(g)
    })
}

fn kind_label(kind: DraftKind) -> &'static str {
    match kind {
        DraftKind::Performs => "Performs task",
        DraftKind::NotPerforms => "Does not perform task",
        DraftKind::Prop(p) => p.label(),
        DraftKind::Genotype => "Genotype",
        DraftKind::Family => "Family",
    }
}

/// The "Add condition" button. It opens and closes the form, which is drawn
/// inline under the chips: a form inside a pop-up would close as soon as one
/// of its own lists opened.
fn add_menu(ui: &mut Ui, state: &mut AppState, t: &Tokens) {
    let open = state.brush.form_open;
    let mut button = egui::Button::new((
        if open {
            icons::ICON_CLOSE
        } else {
            icons::ICON_ADD
        }
        .rich_text()
        .size(16.0)
        .color(t.text_weak),
        RichText::new("Add condition").color(t.text),
    ));
    if open {
        button = button.fill(t.control_active);
    }
    let response = ui.add(button);
    name_widget(&response, Role::Button, "Add condition");
    if response.clicked() {
        state.brush.form_open = !open;
    }
    response.on_hover_text(
        "Add a condition on a task, a property, a genotype or a family, or on a region of the map",
    );
}

/// The form, in a card of its own under the chips.
fn add_form(ui: &mut Ui, state: &mut AppState, t: &Tokens) {
    let mut to_add: Option<Condition> = None;
    let mut pick_region = false;
    let mut close = false;
    ui.add_space(2.0);
    widgets::card_frame(ui)
        .inner_margin(Margin::same(10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            form(ui, state, t, &mut to_add, &mut pick_region, &mut close);
        });
    ui.add_space(2.0);
    if let Some(c) = to_add {
        state.edit_filter(|f| f.set(c));
        state.brush.form_open = false;
    }
    if pick_region {
        state.brush.region_mode = true;
        state.brush.form_open = false;
    }
    if close {
        state.brush.form_open = false;
    }
}

/// A number for a text field: as many digits as its size needs.
fn field_number(v: f64) -> String {
    let a = v.abs();
    let s = if a >= 100.0 {
        format!("{v:.0}")
    } else if a >= 10.0 {
        format!("{v:.1}")
    } else if a >= 1.0 {
        format!("{v:.2}")
    } else {
        format!("{v:.3}")
    };
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s
    }
}

/// The range slider of a numeric condition: the property's distribution
/// over the organisms of this frame as small bars, and under it a track with
/// one handle ("at least", "at most") or two ("between"). Dragging writes
/// the draft's number fields; the bars inside the range take the filter
/// colour.
fn range_slider(ui: &mut Ui, t: &Tokens, values: &[f32], draft: &mut Draft, whole: bool) {
    // The ends are written as the chips write them: "1,096", not "1096".
    let end = |v: f64| {
        if whole {
            format::count(v.round() as i64)
        } else {
            format::real(v)
        }
    };
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for v in values.iter().filter(|v| v.is_finite()) {
        lo = lo.min(f64::from(*v));
        hi = hi.max(f64::from(*v));
    }
    if !(lo.is_finite() && hi.is_finite()) {
        return;
    }
    if hi <= lo {
        ui.label(
            RichText::new(format!(
                "Every organism has the value {}.",
                field_number(lo)
            ))
            .small()
            .color(t.text_faint),
        );
        return;
    }
    let log = lo > 0.0 && hi / lo > 50.0;
    let to01 = |v: f64| -> f64 {
        let v = v.clamp(lo, hi);
        if log {
            (v / lo).ln() / (hi / lo).ln()
        } else {
            (v - lo) / (hi - lo)
        }
    };
    let from01 = |x: f64| -> f64 {
        let x = x.clamp(0.0, 1.0);
        if log {
            lo * (hi / lo).powf(x)
        } else {
            lo + x * (hi - lo)
        }
    };
    let (typed_a, typed_b) = draft.numbers().unwrap_or((None, None));
    // The range the fields describe, with the open side at the data's end.
    let mut a = typed_a.unwrap_or(lo).clamp(lo, hi);
    let mut b = typed_b.unwrap_or(hi).clamp(lo, hi);
    if a > b {
        std::mem::swap(&mut a, &mut b);
    }

    const BARS: f32 = 30.0;
    const HANDLE: f32 = 7.0;
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(
        vec2(width, BARS + 2.0 * HANDLE + 4.0),
        Sense::click_and_drag(),
    );
    name_widget(&response, Role::Slider, "Range of values");
    let left = rect.left() + HANDLE;
    let right = rect.right() - HANDLE;
    let x_of = |v: f64| left + (right - left) * to01(v) as f32;

    if let Some(p) = response.interact_pointer_pos().filter(|_| {
        response.dragged() || response.clicked() || response.is_pointer_button_down_on()
    }) {
        let v = from01(f64::from((p.x - left) / (right - left)));
        match draft.comparison {
            Comparison::AtLeast | Comparison::AtMost => {
                draft.value = field_number(v);
                if draft.comparison == Comparison::AtLeast {
                    a = v;
                } else {
                    b = v;
                }
            }
            Comparison::Between => {
                // The nearer handle follows the pointer.
                if (p.x - x_of(a)).abs() <= (p.x - x_of(b)).abs() {
                    a = v;
                } else {
                    b = v;
                }
                draft.value = field_number(a.min(b));
                draft.value2 = field_number(a.max(b));
            }
        }
    }

    let painter = ui.painter_at(rect);
    const BINS: usize = 36;
    let mut counts = [0u32; BINS];
    for v in values.iter().filter(|v| v.is_finite()) {
        let k = ((to01(f64::from(*v)) * BINS as f64) as usize).min(BINS - 1);
        counts[k] += 1;
    }
    let top = counts.iter().copied().max().unwrap_or(1).max(1) as f32;
    let base = rect.top() + BARS;
    let bin_w = (right - left) / BINS as f32;
    let (xa, xb) = (x_of(a.min(b)), x_of(a.max(b)));
    for (k, &n) in counts.iter().enumerate() {
        if n == 0 {
            continue;
        }
        let x0 = left + bin_w * k as f32;
        let h = (BARS - 2.0) * (n as f32 / top).sqrt().max(0.06);
        let mid = x0 + bin_w / 2.0;
        let inside = mid >= xa && mid <= xb;
        painter.rect_filled(
            Rect::from_min_max(pos2(x0 + 0.5, base - h), pos2(x0 + bin_w - 0.5, base)),
            1.0,
            if inside { t.filter } else { t.border_strong },
        );
    }
    let y = base + HANDLE + 2.0;
    painter.line_segment(
        [pos2(left, y), pos2(right, y)],
        Stroke::new(3.0, t.control_active),
    );
    painter.line_segment([pos2(xa, y), pos2(xb, y)], Stroke::new(3.0, t.filter));
    let handle = |x: f32| {
        painter.circle(
            pos2(x, y),
            HANDLE - 1.0,
            t.surface,
            Stroke::new(2.0, t.filter),
        );
    };
    match draft.comparison {
        Comparison::AtLeast => handle(xa),
        Comparison::AtMost => handle(xb),
        Comparison::Between => {
            handle(xa);
            handle(xb);
        }
    }
    response.on_hover_text(
        "Drag to set the range. The bars show how the organisms of this frame are spread over the property; the coloured ones are inside the range.",
    );
    ui.horizontal(|ui| {
        ui.label(RichText::new(end(lo)).small().color(t.text_faint));
        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
            ui.label(RichText::new(end(hi)).small().color(t.text_faint));
            if log {
                ui.label(RichText::new("log scale").small().color(t.text_faint));
            }
        });
    });
}

fn form(
    ui: &mut Ui,
    state: &mut AppState,
    t: &Tokens,
    to_add: &mut Option<Condition>,
    pick_region: &mut bool,
    close: &mut bool,
) {
    let frame = state.frame.clone();
    let tasks: Vec<String> = (0..frame.as_ref().map_or(0, |f| f.env.tasks.len()))
        .map(|k| task_name(frame.as_deref(), k))
        .collect();
    let families: Vec<(i32, u32)> = frame
        .as_ref()
        .and_then(|f| f.map.as_ref())
        .map(|m| m.families.iter().map(|f| (f.root, f.organisms)).collect())
        .unwrap_or_default();
    let genotype = selected_genotype(state);
    let draft: &mut Draft = &mut state.brush.draft;

    widgets::caption(ui, "New condition");
    let kind = egui::ComboBox::from_id_salt("filter-kind")
        .selected_text(kind_label(draft.kind))
        .width(220.0)
        .height(360.0)
        .show_ui(ui, |ui| {
            let mut kinds = vec![DraftKind::Performs, DraftKind::NotPerforms];
            kinds.extend(Prop::ALL.into_iter().map(DraftKind::Prop));
            kinds.push(DraftKind::Genotype);
            kinds.push(DraftKind::Family);
            for k in kinds {
                ui.selectable_value(&mut draft.kind, k, kind_label(k));
            }
        })
        .response;
    name_widget(&kind, Role::ComboBox, "What the condition is about");
    kind.on_hover_text("What the condition is about");
    ui.add_space(2.0);

    let mut ready = false;
    match draft.kind {
        DraftKind::Performs | DraftKind::NotPerforms => {
            if tasks.is_empty() {
                ui.label(RichText::new("This environment rewards no tasks.").color(t.text_weak));
            } else {
                draft.task = draft.task.min(tasks.len() - 1);
                let r = egui::ComboBox::from_id_salt("filter-task")
                    .selected_text(tasks[draft.task].clone())
                    .width(220.0)
                    .height(360.0)
                    .show_ui(ui, |ui| {
                        for (k, name) in tasks.iter().enumerate() {
                            ui.selectable_value(&mut draft.task, k, name);
                        }
                    })
                    .response;
                name_widget(&r, Role::ComboBox, "Task");
                r.on_hover_text(
                    "The task the organisms must perform in their last gestation, or must not",
                );
                ready = true;
            }
        }
        DraftKind::Prop(p) => {
            let options: Vec<(Comparison, &str, &str)> = Comparison::ALL
                .iter()
                .map(|c| (*c, c.label(), ""))
                .collect();
            widgets::segmented(ui, "filter-comparison", &mut draft.comparison, &options);
            ui.add_space(2.0);
            let height = ui.spacing().interact_size.y;
            let field = |ui: &mut Ui, text: &mut String| {
                ui.add(
                    egui::TextEdit::singleline(text)
                        .desired_width(72.0)
                        .min_size(vec2(72.0, height))
                        .align(egui::Align2::LEFT_CENTER)
                        .hint_text("value"),
                )
            };
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                field(ui, &mut draft.value);
                if draft.comparison == Comparison::Between {
                    ui.label(RichText::new("and").color(t.text_weak));
                    field(ui, &mut draft.value2);
                }
            });
            match frame
                .as_deref()
                .and_then(|f| f.map.as_ref())
                .and_then(|m| m.layer(p.layer()))
            {
                Some(values) => range_slider(ui, t, values, draft, p.is_whole()),
                None => {
                    ui.label(
                        RichText::new("Reading the values…")
                            .small()
                            .color(t.text_faint),
                    );
                }
            }
            ready = draft.numbers().is_some();
            if !ready {
                ui.label(
                    RichText::new("Type a number or drag the slider.")
                        .small()
                        .color(t.text_faint),
                );
            }
        }
        DraftKind::Genotype => match (genotype, frame.as_deref()) {
            (Some(g), Some(f)) => {
                ui.label(RichText::new(genotype_name(f, g)).color(t.text));
                ready = true;
            }
            _ => {
                ui.label(
                    RichText::new("Pick a genotype first.").color(t.text_weak),
                )
                .on_hover_text(
                    "Click an organism on the map, or a genotype in the Genotypes tab: the condition then names that genotype.",
                );
            }
        },
        DraftKind::Family => {
            if families.is_empty() {
                ui.label(RichText::new("Looking up the families…").color(t.text_weak))
                    .on_hover_text(
                        "Families are the groups the Ancestry map layer colours. They appear once the frame carries that layer.",
                    );
            } else {
                let current = draft
                    .family
                    .filter(|r| families.iter().any(|f| f.0 == *r))
                    .unwrap_or(families[0].0);
                draft.family = Some(current);
                let name = |r: i32, n: u32| {
                    let who = frame
                        .as_deref()
                        .map_or_else(|| format!("#{r}"), |f| genotype_name(f, r));
                    format!(
                        "Family of {who} ({})",
                        format::plural(u64::from(n), "organism")
                    )
                };
                let shown = families
                    .iter()
                    .find(|f| f.0 == current)
                    .map_or_else(String::new, |f| name(f.0, f.1));
                let r = egui::ComboBox::from_id_salt("filter-family")
                    .selected_text(shown)
                    .width(220.0)
                    .height(360.0)
                    .show_ui(ui, |ui| {
                        for &(r, n) in &families {
                            ui.selectable_value(&mut draft.family, Some(r), name(r, n));
                        }
                    })
                    .response;
                name_widget(&r, Role::ComboBox, "Family");
                r.on_hover_text(
                    "The family, named after its root genotype, with the organisms it has now",
                );
                ready = true;
            }
        }
    }
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        let add = ui.add_enabled(
            ready,
            egui::Button::new(RichText::new("Add").family(semibold()).color(t.on_accent))
                .fill(t.accent),
        );
        name_widget(&add, Role::Button, "Add the condition");
        if add.clicked() {
            *to_add = match draft.kind {
                DraftKind::Performs | DraftKind::NotPerforms => Some(Condition::Task {
                    task: draft.task,
                    performs: draft.kind == DraftKind::Performs,
                }),
                DraftKind::Prop(p) => draft.numbers().map(|(lo, hi)| {
                    Condition::range(
                        p,
                        lo.unwrap_or(f64::NEG_INFINITY),
                        hi.unwrap_or(f64::INFINITY),
                    )
                }),
                DraftKind::Genotype => genotype.map(|id| Condition::Genotype { id }),
                DraftKind::Family => draft.family.map(|root| Condition::Family { root }),
            };
        }
        if ui
            .button("Select a region")
            .on_hover_text(
                "Then drag a rectangle on the map. Shift and drag does the same at any time.",
            )
            .clicked()
        {
            *pick_region = true;
        }
        if ui
            .add(egui::Button::new(RichText::new("Cancel").color(t.text_weak)).frame(false))
            .clicked()
        {
            *close = true;
        }
    });
}
