//! The map screen: the world as half-block (or two-column) cells, a
//! scrolling viewport, the cursor, a legend line and, on wide terminals,
//! a side panel.

use avida_session::{Frame as SimFrame, LayerId};
use avida_viewmodel::text::genotype_char;
use avida_viewmodel::{format_number, layer_legend, render_rgba, Palette, CATEGORICAL_SLOTS};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use super::util::{block, clip_lines, heading, kv, trunc, DIM};
use super::waiting;
use crate::app::{layer_title, App, MapCache, MapGeom};
use crate::color::{contrast, luminance};

/// Stands in for a cell under the cursor that nothing lives in: a mid
/// grey that shows on dark and light backgrounds alike. Empty cells
/// themselves use the terminal's own background.
const EMPTY_CURSOR: [u8; 3] = [140, 140, 150];

/// Terminal width from which the side panel appears.
pub const WIDE: u16 = 110;
const SIDE_W: u16 = 36;

pub fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    if app.frame.as_ref().and_then(|f| f.map.as_ref()).is_none() {
        waiting(f, area, super::FIRST_FRAME);
        return;
    }
    let (main, side) = if area.width >= WIDE {
        let [m, s] =
            Layout::horizontal([Constraint::Min(20), Constraint::Length(SIDE_W)]).areas(area);
        (m, Some(s))
    } else {
        (area, None)
    };
    let [grid, legend, info] = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(main);

    draw_grid(f, app, grid);
    f.render_widget(
        Paragraph::new(Line::from(map_legend_spans(app, usize::from(legend.width)))),
        legend,
    );
    // When the world is larger than the window, say which part is shown,
    // if the line has room for it next to the cell's description.
    let summary = cell_summary(app);
    let part = match (app.geom, app.world_size()) {
        (Some(g), Some((ww, wh))) if g.vis_w < ww || g.vis_h < wh => format!(
            "view x {}-{}, y {}-{} of {ww}x{wh} ",
            g.ox,
            g.ox + g.vis_w - 1,
            g.oy,
            g.oy + g.vis_h - 1
        ),
        _ => String::new(),
    };
    let width = usize::from(info.width);
    let part = if summary.chars().count() + part.chars().count() < width {
        part
    } else {
        String::new()
    };
    let summary = trunc(&summary, width.saturating_sub(part.chars().count()));
    let gap = width.saturating_sub(summary.chars().count() + part.chars().count());
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(summary, Style::new()),
            Span::raw(" ".repeat(gap)),
            Span::styled(part, DIM),
        ])),
        info,
    );
    if let Some(side) = side {
        draw_side(f, app, side);
    }
}

fn rebuild_cache(app: &mut App) {
    let Some(frame) = app.frame.clone() else {
        return;
    };
    let Some(map) = frame.map.as_ref() else {
        return;
    };
    let layer = app.layer;
    let palette = app.palette_for(layer);
    let scale = app.scale();
    let key = (app.frames_applied, layer, palette, scale);
    if app.map_cache.as_ref().is_some_and(|c| c.key == key) {
        return;
    }
    let rgba = render_rgba(map, layer, &palette, scale);
    let legend = layer_legend(map, layer, scale);
    app.map_cache = Some(MapCache { key, rgba, legend });
}

/// A shading character for a brightness in `[0, 1]`.
fn shade(lum: f64) -> char {
    match lum {
        l if l < 0.25 => '░',
        l if l < 0.5 => '▒',
        l if l < 0.75 => '▓',
        _ => '█',
    }
}

/// The colour of world cell `(x, y)` (`None` for an empty cell).
fn cell_rgb(cache: &MapCache, w: usize, x: usize, y: usize) -> Option<[u8; 3]> {
    let i = (y * w + x) * 4;
    let p = cache.rgba.get(i..i + 4)?;
    (p[3] != 0).then_some([p[0], p[1], p[2]])
}

fn draw_grid(f: &mut Frame, app: &mut App, area: Rect) {
    rebuild_cache(app);
    let Some((ww, wh)) = app.world_size() else {
        return;
    };
    let big = app.wide_cells();
    let (cell_w, cell_rows): (u16, u16) = if big { (2, 1) } else { (1, 2) };
    let max_w = usize::from(area.width / cell_w);
    let max_h = usize::from(area.height) * usize::from(cell_rows);
    let vis_w = ww.min(max_w);
    let vis_h = wh.min(max_h);

    // Scroll so the cursor stays inside, keeping the old origin otherwise.
    let (mut ox, mut oy) = app.geom.map_or((0, 0), |g| (g.ox, g.oy));
    ox = ox.min(ww - vis_w);
    oy = oy.min(wh - vis_h);
    let (cx, cy) = app.cursor;
    if cx < ox {
        ox = cx;
    } else if cx >= ox + vis_w {
        ox = cx + 1 - vis_w;
    }
    if cy < oy {
        oy = cy;
    } else if cy >= oy + vis_h {
        oy = cy + 1 - vis_h;
    }

    let cols = vis_w as u16 * cell_w;
    let rows = (vis_h as u16).div_ceil(cell_rows);
    let rect = Rect {
        x: area.x + (area.width - cols) / 2,
        y: area.y + (area.height - rows) / 2,
        width: cols,
        height: rows,
    };
    app.geom = Some(MapGeom {
        area: rect,
        cell_w,
        cell_rows,
        ox,
        oy,
        vis_w,
        vis_h,
    });

    let Some(cache) = app.map_cache.as_ref() else {
        return;
    };
    let mode = app.color_mode;
    let frame = app.frame.clone();
    let Some(frame) = frame else {
        return;
    };
    let Some(map) = frame.map.as_ref() else {
        return;
    };
    let genotype_layer = app.layer == LayerId::Genotype;
    let buf = f.buffer_mut();
    let at = |x: usize, y: usize| -> ([u8; 3], bool) {
        let rgb = cell_rgb(cache, ww, x, y);
        (rgb.unwrap_or(EMPTY_CURSOR), rgb.is_some())
    };
    for row in 0..rows {
        for col in 0..vis_w {
            if big {
                let (x, y) = (ox + col, oy + usize::from(row));
                let (rgb, occupied) = at(x, y);
                let is_cursor = (x, y) == app.cursor;
                let id = map.genotype_id.get(y * ww + x).copied().unwrap_or(-1);
                if mode.is_mono() {
                    let sym = if !occupied {
                        '·'
                    } else if genotype_layer {
                        genotype_char(map, id)
                    } else {
                        shade(luminance(rgb))
                    };
                    for k in 0..2u16 {
                        if let Some(c) = buf.cell_mut((rect.x + col as u16 * 2 + k, rect.y + row)) {
                            c.set_char(if k == 0 { sym } else { ' ' });
                            if is_cursor {
                                c.modifier.insert(Modifier::REVERSED | Modifier::BOLD);
                            }
                        }
                    }
                    continue;
                }
                let sym = if !occupied {
                    '·'
                } else if genotype_layer {
                    genotype_char(map, id)
                } else {
                    ' '
                };
                // An empty cell keeps the terminal's own colours (a dim dot),
                // so it reads on dark and light backgrounds.
                let empty_plain = !occupied && !is_cursor;
                let ink = mode.quantised(contrast(mode.quantised(rgb)));
                let (fg, bg) = if is_cursor { (rgb, ink) } else { (ink, rgb) };
                for k in 0..2u16 {
                    let c = buf.cell_mut((rect.x + col as u16 * 2 + k, rect.y + row));
                    if let Some(c) = c {
                        c.set_char(if k == 0 { sym } else { ' ' });
                        if empty_plain {
                            c.set_fg(Color::Reset).set_bg(Color::Reset);
                            c.modifier.insert(Modifier::DIM);
                        } else {
                            c.set_fg(mode.color(fg)).set_bg(mode.color(bg));
                        }
                        if is_cursor {
                            c.modifier.insert(Modifier::BOLD);
                        }
                    }
                }
            } else {
                let (x, top_y) = (ox + col, oy + usize::from(row) * 2);
                if mode.is_mono() {
                    // Brightness of an occupied half, none for an empty one.
                    let lum = |y: usize| -> Option<f64> {
                        if y >= oy + vis_h {
                            return None;
                        }
                        let (rgb, occupied) = at(x, y);
                        occupied.then(|| luminance(rgb))
                    };
                    let glyph = match (lum(top_y), lum(top_y + 1)) {
                        (Some(a), Some(b)) => shade((a + b) / 2.0),
                        (Some(_), None) => '▀',
                        (None, Some(_)) => '▄',
                        (None, None) => ' ',
                    };
                    if let Some(c) = buf.cell_mut((rect.x + col as u16, rect.y + row)) {
                        c.set_char(glyph);
                        if (x, top_y) == app.cursor || (x, top_y + 1) == app.cursor {
                            c.modifier.insert(Modifier::REVERSED | Modifier::BOLD);
                        }
                    }
                    continue;
                }
                // `None` is an empty cell (the terminal's own background) or
                // a row below the view.
                let half = |y: usize| -> Option<[u8; 3]> {
                    if y >= oy + vis_h {
                        return None;
                    }
                    let (rgb, occupied) = at(x, y);
                    if (x, y) == app.cursor {
                        Some(if occupied {
                            mode.quantised(contrast(mode.quantised(rgb)))
                        } else {
                            rgb
                        })
                    } else {
                        occupied.then_some(rgb)
                    }
                };
                let (top, bottom) = (half(top_y), half(top_y + 1));
                if let Some(c) = buf.cell_mut((rect.x + col as u16, rect.y + row)) {
                    match (top, bottom) {
                        (Some(t), Some(b)) => {
                            c.set_char('▀').set_fg(mode.color(t)).set_bg(mode.color(b));
                        }
                        (Some(t), None) => {
                            c.set_char('▀').set_fg(mode.color(t)).set_bg(Color::Reset);
                        }
                        (None, Some(b)) => {
                            c.set_char('▄').set_fg(mode.color(b)).set_bg(Color::Reset);
                        }
                        (None, None) => {
                            c.set_char(' ').set_fg(Color::Reset).set_bg(Color::Reset);
                        }
                    }
                }
            }
        }
    }
}

/// The legend line: the colour ramp with its range, or the genotype
/// swatches.
pub fn map_legend_spans(app: &App, width: usize) -> Vec<Span<'static>> {
    let env = app.env();
    let title = layer_title(env, app.layer);
    let mode = app.color_mode;
    let mut spans = vec![Span::styled(format!(" {title}"), heading())];
    if app.mono_letters() {
        spans.push(Span::styled(" (a letter per genotype, · empty)", DIM));
    }
    let Some(frame) = app.frame.as_ref() else {
        return spans;
    };
    let Some(map) = frame.map.as_ref() else {
        return spans;
    };
    if app.layer == LayerId::Genotype {
        spans.push(Span::raw("  "));
        let mut used: usize = spans.iter().map(|s| s.content.chars().count()).sum();
        for (slot, &id) in map.top_genotypes.iter().enumerate().take(CATEGORICAL_SLOTS) {
            let name = frame
                .genotypes
                .iter()
                .find(|g| g.id == id)
                .map_or_else(|| format!("#{id}"), |g| g.name.clone());
            let count = frame
                .genotypes
                .iter()
                .find(|g| g.id == id)
                .map_or(0, |g| g.abundance);
            let label = format!(" {} {name} {count}  ", genotype_char(map, id));
            let need = 1 + label.chars().count();
            if used + need > width {
                break;
            }
            let c = Palette::OkabeIto.category(Some(slot));
            spans.push(Span::styled(
                "█",
                Style::new().fg(mode.color([c[0], c[1], c[2]])),
            ));
            spans.push(Span::raw(label));
            used += need;
        }
        return spans;
    }
    let scale = app.scale();
    let legend = app
        .map_cache
        .as_ref()
        .and_then(|c| c.legend.clone())
        .or_else(|| layer_legend(map, app.layer, scale));
    let scale_note = match &legend {
        Some(l) if l.scale == avida_viewmodel::Scale::Log => " (log)",
        _ => "",
    };
    spans.push(Span::styled(scale_note, DIM));
    match legend {
        Some(l) => {
            let used: usize = spans.iter().map(|s| s.content.chars().count()).sum();
            let lo = format_number(l.min);
            let hi = format_number(l.max);
            let bar = width
                .saturating_sub(used + lo.chars().count() + hi.chars().count() + 6)
                .clamp(4, 40);
            let palette = app.palette_for(app.layer);
            spans.push(Span::raw("  "));
            spans.push(Span::raw(lo));
            spans.push(Span::raw(" "));
            for i in 0..bar {
                let c = palette.sample(i as f64 / (bar - 1) as f64);
                let glyph = if mode.is_mono() {
                    shade(i as f64 / (bar - 1) as f64).to_string()
                } else {
                    "█".to_string()
                };
                spans.push(Span::styled(
                    glyph,
                    Style::new().fg(mode.color([c[0], c[1], c[2]])),
                ));
            }
            spans.push(Span::raw(" "));
            spans.push(Span::raw(hi));
        }
        None => spans.push(Span::styled("  no values yet", DIM)),
    }
    spans
}

fn layer_value(frame: &SimFrame, layer: LayerId, cell: usize) -> Option<f32> {
    let v = frame.map.as_ref()?.layer(layer)?.get(cell).copied()?;
    v.is_finite().then_some(v)
}

/// One line about the cell under the cursor.
pub fn cell_summary(app: &App) -> String {
    let Some(frame) = app.frame.as_ref() else {
        return String::new();
    };
    let Some(map) = frame.map.as_ref() else {
        return String::new();
    };
    let Some((w, _)) = app.world_size() else {
        return String::new();
    };
    let (x, y) = app.cursor;
    let cell = y * w + x;
    let mut s = format!(" ({x},{y}) cell {cell}");
    let id = map.genotype_id.get(cell).copied().unwrap_or(-1);
    if id == -1 {
        s.push_str("  empty");
    } else {
        let name = frame
            .genotypes
            .iter()
            .find(|g| g.id == id)
            .map_or_else(|| format!("#{id}"), |g| g.name.clone());
        s.push_str(&format!("  {} {name}", genotype_char(map, id)));
        if app.layer != LayerId::Genotype {
            if let Some(v) = layer_value(frame, app.layer, cell) {
                s.push_str(&format!(
                    "  {} {}",
                    layer_title(app.env(), app.layer).to_lowercase(),
                    format_number(f64::from(v))
                ));
            }
        }
    }
    if app.selection == Some(cell as i32) {
        s.push_str("  [inspected]");
    }
    s
}

fn draw_side(f: &mut Frame, app: &App, area: Rect) {
    let layers = app.layers();
    let h = area.height;
    // Layers block gets up to a third of the height.
    let layer_rows = (layers.len() as u16).min((h / 3).max(4)).max(3);
    let [lay, geno, cell] = Layout::vertical([
        Constraint::Length(layer_rows + 2),
        Constraint::Min(5),
        Constraint::Length(9),
    ])
    .areas(area);

    // Layers, windowed around the current one.
    let cur = layers.iter().position(|&l| l == app.layer).unwrap_or(0);
    let n = usize::from(layer_rows);
    let start = cur
        .saturating_sub(n / 2)
        .min(layers.len().saturating_sub(n));
    let lines: Vec<Line> = layers
        .iter()
        .enumerate()
        .skip(start)
        .take(n)
        .map(|(i, &l)| {
            let t = layer_title(app.env(), l);
            let w = usize::from(lay.width.saturating_sub(4));
            if i == cur {
                Line::styled(
                    format!("▸ {}", trunc(&t, w)),
                    Style::new().add_modifier(Modifier::BOLD | Modifier::REVERSED),
                )
            } else {
                Line::raw(format!("  {}", trunc(&t, w)))
            }
        })
        .collect();
    f.render_widget(
        Paragraph::new(clip_lines(lines, usize::from(lay.width.saturating_sub(2))))
            .block(block("Layers [ ]")),
        lay,
    );

    // Top genotypes.
    let mut lines: Vec<Line> = Vec::new();
    if let Some(fr) = app.frame.as_ref() {
        if let Some(map) = fr.map.as_ref() {
            for (slot, &id) in map.top_genotypes.iter().enumerate() {
                let Some(g) = fr.genotypes.iter().find(|g| g.id == id) else {
                    continue;
                };
                let c = Palette::OkabeIto.category(Some(slot));
                lines.push(Line::from(vec![
                    Span::styled(
                        "█ ",
                        Style::new().fg(app.color_mode.color([c[0], c[1], c[2]])),
                    ),
                    Span::raw(format!(
                        "{} {} {:>5}",
                        genotype_char(map, id),
                        trunc(&g.name, 14),
                        g.abundance
                    )),
                ]));
            }
        }
    }
    f.render_widget(
        Paragraph::new(clip_lines(lines, usize::from(geno.width.saturating_sub(2))))
            .block(block("Top genotypes")),
        geno,
    );

    // The cell under the cursor.
    let mut lines: Vec<Line> = Vec::new();
    if let Some(fr) = app.frame.as_ref() {
        let cell_id = app.cursor_cell();
        match (&fr.inspect, cell_id) {
            (Some(o), Some(c)) if o.cell == c => {
                let p = &o.phenotype;
                lines.push(kv("genotype", 10, o.genotype_name.clone()));
                lines.push(kv("fitness", 10, format_number(p.fitness)));
                lines.push(kv("merit", 10, format_number(p.merit)));
                lines.push(kv("gestation", 10, p.gestation_time.to_string()));
                lines.push(kv("age", 10, p.age.to_string()));
                lines.push(kv("generation", 10, p.generation.to_string()));
                lines.push(kv("length", 10, p.genome_length.to_string()));
            }
            _ => lines.push(Line::styled("Enter inspects the cell", DIM)),
        }
    }
    f.render_widget(
        Paragraph::new(lines).block(block(&format!("Cell ({},{})", app.cursor.0, app.cursor.1))),
        cell,
    );
}
