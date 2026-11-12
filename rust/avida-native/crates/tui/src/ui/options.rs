//! The options screen: what the run is made of and how the view is set.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use super::util::{block, clip_lines, kv, trunc};
use crate::app::{layer_title, App};
use crate::color::ColorMode;

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let (left, right) = if area.width >= 90 {
        let [l, r] = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
            .areas(area);
        (l, Some(r))
    } else {
        (area, None)
    };
    let w = usize::from(left.width.saturating_sub(4)).saturating_sub(15);

    let mut run: Vec<Line> = Vec::new();
    run.push(kv(
        "Directory",
        15,
        trunc(&app.work_dir.display().to_string(), w),
    ));
    let args = if app.args.is_empty() {
        "(none)".to_string()
    } else {
        app.args.join(" ")
    };
    run.push(kv("Arguments", 15, trunc(&args, w)));
    if let Some(fr) = app.frame.as_ref() {
        let h = &fr.header;
        run.push(kv("World size", 15, format!("{}x{}", h.world_x, h.world_y)));
        run.push(kv("Random seed", 15, h.random_seed.to_string()));
        run.push(kv("Inst set", 15, h.inst_set.clone()));
        run.push(kv("Tasks", 15, fr.env.tasks.len().to_string()));
        run.push(kv("Resources", 15, fr.env.resources.len().to_string()));
        run.push(kv("Reactions", 15, fr.env.reactions.len().to_string()));
    }
    run.push(kv("State", 15, format!("{:?}", app.run_state())));
    run.push(kv(
        "Update",
        15,
        app.current_update().map_or("-".into(), |u| u.to_string()),
    ));
    run.push(kv("Log lines", 15, app.log.len().to_string()));
    run.push(kv("History rows", 15, app.history.len().to_string()));

    let mut view: Vec<Line> = Vec::new();
    view.push(kv("Speed", 15, format!("{}  (+ / -)", app.speed_label())));
    view.push(kv(
        "Layer",
        15,
        format!("{}  ([ / ])", layer_title(app.env(), app.layer)),
    ));
    view.push(kv("Palette", 15, format!("{:?}  (p)", app.palette)));
    view.push(kv(
        "Scale",
        15,
        format!(
            "{:?}  (L){}",
            app.scale(),
            if App::scale_applies(app.layer) {
                ""
            } else {
                " n/a here"
            }
        ),
    ));
    view.push(kv(
        "Cell size",
        15,
        format!(
            "{}  (i)",
            if app.mono_letters() {
                "two columns (letters, as there is no colour)"
            } else if app.big_cells {
                "two columns"
            } else {
                "half block"
            }
        ),
    ));
    view.push(kv(
        "Colours",
        15,
        match app.color_mode {
            ColorMode::TrueColor => "24-bit",
            ColorMode::Indexed256 => "256",
            ColorMode::Mono => "none",
        },
    ));
    view.push(kv(
        "Cursor",
        15,
        format!("({},{})", app.cursor.0, app.cursor.1),
    ));
    view.push(kv(
        "Inspecting",
        15,
        app.selection
            .map_or("nothing".to_string(), |c| format!("cell {c}")),
    ));
    view.push(Line::raw(""));
    view.push(Line::raw("W writes detail-<update>.spop to data/."));
    view.push(Line::raw("X writes the inspected genome to data/."));

    match right {
        Some(r) => {
            f.render_widget(
                Paragraph::new(clip_lines(run, usize::from(left.width.saturating_sub(2))))
                    .block(block("Run")),
                left,
            );
            f.render_widget(
                Paragraph::new(clip_lines(view, usize::from(r.width.saturating_sub(2))))
                    .block(block("View")),
                r,
            );
        }
        None => {
            run.push(Line::raw(""));
            run.extend(view);
            f.render_widget(
                Paragraph::new(clip_lines(run, usize::from(left.width.saturating_sub(2))))
                    .block(block("Options")),
                left,
            );
        }
    }
}
