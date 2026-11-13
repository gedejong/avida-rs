//! The options screen: what the run is made of and how the view is set.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use super::util::{block, clip_lines, kv, trunc};
use crate::app::{layer_title, App};
use crate::color::ColorMode;

/// `s` cut to `w` characters from the front, so the end (the folder's own
/// name) stays.
fn trunc_start(s: &str, w: usize) -> String {
    let n = s.chars().count();
    if n <= w || w == 0 {
        s.to_string()
    } else {
        let tail: String = s.chars().skip(n - (w - 1)).collect();
        format!("…{tail}")
    }
}

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    // Side by side when wide, and also when the one tall column would not
    // fit (the last lines, including the save keys, were cut off at 24 rows).
    let columns = area.width >= 90 || (area.width >= 64 && area.height < 30);
    let (left, right) = if columns {
        let [l, r] = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
            .areas(area);
        (l, Some(r))
    } else {
        (area, None)
    };
    let w = usize::from(left.width.saturating_sub(4)).saturating_sub(17);

    let mut run: Vec<Line> = Vec::new();
    run.push(kv(
        "Directory",
        17,
        trunc_start(&app.work_dir.display().to_string(), w),
    ));
    let args = if app.args.is_empty() {
        "(none)".to_string()
    } else {
        app.args.join(" ")
    };
    run.push(kv("Arguments", 17, trunc(&args, w)));
    if let Some(fr) = app.frame.as_ref() {
        let h = &fr.header;
        run.push(kv("World size", 17, format!("{}x{}", h.world_x, h.world_y)));
        run.push(kv("Random seed", 17, h.random_seed.to_string()));
        run.push(kv("Instruction set", 17, h.inst_set.clone()));
        run.push(kv("Tasks", 17, fr.env.tasks.len().to_string()));
        run.push(kv("Resources", 17, fr.env.resources.len().to_string()));
        run.push(kv("Reactions", 17, fr.env.reactions.len().to_string()));
    }
    run.push(kv("State", 17, format!("{:?}", app.run_state())));
    run.push(kv(
        "Update",
        17,
        app.current_update().map_or("-".into(), |u| u.to_string()),
    ));
    run.push(kv("Log lines", 17, app.log.len().to_string()));
    run.push(kv("History rows", 17, app.history.len().to_string()));

    let mut view: Vec<Line> = Vec::new();
    view.push(kv("Speed", 17, format!("{}  (+ / -)", app.speed_label())));
    view.push(kv(
        "Layer",
        17,
        format!("{}  ([ / ])", layer_title(app.env(), app.layer)),
    ));
    view.push(kv("Palette", 17, format!("{:?}  (p)", app.palette)));
    view.push(kv(
        "Scale",
        17,
        format!(
            "{:?}  (L){}",
            app.scale(),
            if App::scale_applies(app.layer) {
                ""
            } else {
                " (not for this layer)"
            }
        ),
    ));
    view.push(kv(
        "Cell size",
        17,
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
        17,
        match app.color_mode {
            ColorMode::TrueColor => "24-bit",
            ColorMode::Indexed256 => "256",
            ColorMode::Ansi16 => "16",
            ColorMode::Mono => "none",
        },
    ));
    view.push(kv(
        "Background",
        17,
        if app.color_mode.is_mono() {
            "-"
        } else if crate::color::is_light() {
            "light  (--theme)"
        } else {
            "dark  (--theme)"
        },
    ));
    view.push(kv(
        "Cursor",
        17,
        format!("({},{})", app.cursor.0, app.cursor.1),
    ));
    view.push(kv(
        "Inspecting",
        17,
        app.selection
            .map_or("nothing".to_string(), |c| format!("cell {c}")),
    ));
    view.push(Line::raw(""));
    // Short enough for the 38 columns of the two-column layout at 80x24.
    view.push(Line::raw("W saves the population,"));
    view.push(Line::raw("X the inspected genome (data folder)."));

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
