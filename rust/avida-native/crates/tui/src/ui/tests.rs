//! Renders the new screens and overlays on a `TestBackend` at 80x24 (the
//! size they are laid out for) and at the smallest supported size.

use std::sync::Arc;

use ratatui::backend::TestBackend;
use ratatui::Terminal;

use super::{draw, MIN_SIZE};
use crate::actions::{Action, PaletteState};
use crate::app::{App, GenoView, Screen};
use crate::config_view::tests::{setting, setup};
use crate::fixtures::{
    app_with, frame_with_map, frame_with_org, frame_with_values, genotype, organism,
};

fn render(app: &mut App, w: u16, h: u16) -> String {
    let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
    let done = t.draw(|f| draw(f, app)).unwrap();
    let a = done.buffer.area;
    let mut out = String::new();
    for y in 0..a.height {
        let mut line = String::new();
        for x in 0..a.width {
            line.push_str(done.buffer[(x, y)].symbol());
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}

fn has(t: &str, needle: &str) {
    assert!(t.contains(needle), "expected {needle:?} in:\n{t}");
}

fn debugging() -> App {
    let mut a = app_with(frame_with_org(organism(5, 7, 3, 40)));
    a.goto(Screen::Debug);
    a
}

#[test]
fn a_parasite_thread_lists_its_own_memory_space() {
    let org = crate::fixtures::parasitised(5, 3, 4);
    let mut a = app_with(frame_with_org(org));
    a.goto(Screen::Debug);
    // The host thread lists the genome.
    let t = render(&mut a, 110, 30);
    has(&t, "Memory space 0 (genome), 8 sites");
    assert!(
        !t.contains("other spaces"),
        "the host's heads are all home:\n{t}"
    );
    // Switching to the parasite thread follows its IP into space 2.
    a.on_key(key(ratatui::crossterm::event::KeyCode::Char('t')));
    let t = render(&mut a, 110, 30);
    has(&t, "thread 2/2");
    has(&t, "IP 2:4");
    has(&t, "Memory space 2 (parasite), 6 sites");
    has(&t, "other spaces: Wr 1:2");
    assert!(
        t.lines().any(|l| l.contains('>') && l.contains("   4 ")),
        "the IP marker sits on line 4 of space 2:\n{t}"
    );
    assert!(
        !t.lines().any(|l| l.contains("   7 ")),
        "space 0 has gone:\n{t}"
    );
    // The space key cycles: 2 wraps to genome 0, then offspring 1.
    a.on_key(key(ratatui::crossterm::event::KeyCode::Char('y')));
    let t = render(&mut a, 110, 30);
    has(&t, "Memory space 0 (genome), 8 sites");
    a.on_key(key(ratatui::crossterm::event::KeyCode::Char('y')));
    let t = render(&mut a, 110, 30);
    has(&t, "Memory space 1 (offspring), 4 sites");
    has(&t, "other spaces: IP 2:4");
    // A breakpoint in space 1 is per space.
    a.on_key(key(ratatui::crossterm::event::KeyCode::Char('b')));
    let t = render(&mut a, 110, 30);
    has(&t, "breakpoints: 1:0");
    has(&t, "space");
}

#[test]
fn the_tab_strip_fits_80_columns_with_the_new_screens() {
    let mut a = debugging();
    let t = render(&mut a, 80, 24);
    // At 80 columns the longer titles are shortened so that every tab fits.
    has(&t, "U Dbg");
    has(&t, "F Cfg");
    has(&t, "O Opts");
    has(&t, "K Open");
    has(&render(&mut a, 100, 30), "U Debug");
}

#[test]
fn the_debugger_shows_registers_heads_stacks_and_the_listing_at_80x24() {
    let mut a = debugging();
    a.debugger.sel = 6;
    a.debugger.follow_ip = false;
    a.debugger.breakpoints.insert((0, 6));
    let t = render(&mut a, 80, 24);
    has(&t, "AX");
    has(&t, "7");
    has(&t, "IP 3");
    has(&t, "stk A*");
    has(&t, "Memory 40 sites");
    has(&t, "h-copy");
    // The IP line and the breakpoint line carry their markers.
    assert!(
        t.lines().any(|l| l.contains('>') && l.contains("   3 ")),
        "{t}"
    );
    assert!(
        t.lines().any(|l| l.contains('●') && l.contains("   6 ")),
        "{t}"
    );
    has(&t, "breakpoints: 6");
    has(&t, "Watching organism #5");
    has(&t, "i step");
}

#[test]
fn a_step_marks_the_changed_register_and_the_stop_note() {
    let mut a = debugging();
    a.apply_frame(Arc::new(crate::fixtures::frame_with_org(organism(
        5, 8, 4, 40,
    ))));
    a.apply_events(vec![avida_session::SessionEvent::RunUntilEnded {
        outcome: avida_session::UntilOutcome::Reached,
        slices: 1,
        update: 12,
    }]);
    let t = render(&mut a, 80, 24);
    assert!(
        t.lines().any(|l| l.contains("AX") && l.contains("8*")),
        "{t}"
    );
    has(&t, "Stepped after 1 time slice.");
    has(&t, "+1 cycles");
}

#[test]
fn the_debugger_degrades_to_the_smallest_terminal() {
    let mut a = debugging();
    let (w, h) = MIN_SIZE;
    let t = render(&mut a, w, h);
    has(&t, "AX");
    has(&t, "Memory");
    for l in t.lines() {
        assert!(l.chars().count() <= usize::from(w), "{l}");
    }
}

#[test]
fn the_debugger_without_an_organism_explains_itself() {
    let mut a = app_with(frame_with_map());
    a.goto(Screen::Debug);
    let t = render(&mut a, 80, 24);
    has(&t, "is empty");
}

#[test]
fn a_dead_organism_is_marked() {
    let mut a = debugging();
    a.apply_frame(Arc::new(avida_session::Frame {
        update: 20,
        ..frame_with_map()
    }));
    let t = render(&mut a, 80, 24);
    has(&t, "DIED u20");
    has(&t, "The organism died at update 20");
}

fn configuring() -> App {
    let mut a = app_with(frame_with_map());
    let mut mut_prob = setting("COPY_MUT_PROB", "double", "0.02", "0.0075");
    mut_prob.desc = "Mutation rate\nper copy";
    a.set_setup(Arc::new(setup(vec![
        setting("WORLD_X", "int", "60", "60"),
        setting("WORLD_Y", "int", "60", "60"),
        mut_prob,
    ])));
    a.goto(Screen::Config);
    a
}

#[test]
fn the_config_screen_lists_filters_and_previews_the_restart() {
    let mut a = configuring();
    let t = render(&mut a, 80, 24);
    has(&t, "3 of 3 settings, 0 overrides");
    has(&t, "WORLD_X");
    has(&t, "COPY_MUT_PROB");
    has(&t, "A restart would run with: no extra arguments");
    a.config.query = "copy".into();
    a.config.filtering = true;
    let t = render(&mut a, 80, 24);
    has(&t, "1 of 3 settings");
    assert!(!t.contains("WORLD_X"), "{t}");
    a.config.filtering = false;
    a.config.query.clear();
    a.config.sel = 1;
    a.config.overrides.push(("WORLD_Y".into(), "25".into()));
    let t = render(&mut a, 80, 24);
    has(&t, "60 > 25");
    has(&t, "1 override");
    has(&t, "A restart would run with: -set WORLD_Y 25");
}

#[test]
fn the_config_edit_prompt_shows_the_value_and_the_error() {
    let mut a = configuring();
    a.config_edit();
    a.config.edit.as_mut().unwrap().buffer = "abc".into();
    a.config_commit_edit();
    let t = render(&mut a, 80, 24);
    has(&t, "WORLD_X (int) = abc_");
    has(&t, "is not a whole number");
    let (w, h) = MIN_SIZE;
    let t = render(&mut a, w, h);
    has(&t, "WORLD_X");
}

#[test]
fn the_palette_overlay_lists_matches_with_their_keys() {
    let mut a = debugging();
    a.cmd_palette = Some(PaletteState {
        query: "bre".into(),
        selected: 0,
    });
    let t = render(&mut a, 80, 24);
    has(&t, "Commands");
    has(&t, "> bre_");
    assert!(
        t.lines()
            .any(|l| l.contains("breakpoint on the selected line") && l.contains('b')),
        "{t}"
    );
    a.cmd_palette = Some(PaletteState {
        query: "zzzzzz".into(),
        selected: 0,
    });
    let t = render(&mut a, 80, 24);
    has(&t, "No action matches.");
    let (w, h) = MIN_SIZE;
    a.cmd_palette = Some(PaletteState::default());
    let t = render(&mut a, w, h);
    has(&t, "Commands");
}

#[test]
fn the_help_overlay_lists_every_binding_across_its_scroll() {
    let mut a = app_with(frame_with_map());
    a.perform(Action::Help, false);
    let mut seen = String::new();
    for _ in 0..8 {
        let t = render(&mut a, 80, 24);
        seen.push_str(&t);
        a.help_scroll += 18;
    }
    for b in crate::actions::BINDINGS {
        has(&seen, &crate::actions::keys_label(b.keys));
        has(&seen, &b.title[..b.title.len().min(30)]);
    }
    has(&seen, "(Debug)");
}

#[test]
fn the_footer_hints_come_from_the_table() {
    let mut a = debugging();
    let t = render(&mut a, 100, 24);
    has(&t, "i step");
    has(&t, "b breakpoint");
    has(&t, "g run to bp");
    has(&t, ": commands");
    a.goto(Screen::Config);
    let t = render(&mut a, 100, 24);
    has(&t, "/ filter");
    has(&t, "e edit");
}

#[test]
fn the_abundance_chart_stacks_the_top_genotypes() {
    let mut a = app_with(frame_with_map());
    for u in 1..6 {
        let mut f = frame_with_map();
        f.update = u;
        a.apply_frame(Arc::new(f));
    }
    a.goto(Screen::Genotypes);
    a.geno_view = GenoView::Abundance;
    let t = render(&mut a, 80, 24);
    has(&t, "Abundance over time");
    assert!(t.contains('█'), "{t}");
    has(&t, "050-aaaaa");
    has(&t, "other");
    has(&t, "u1");
    has(&t, "u5");
    let (w, h) = MIN_SIZE;
    let t = render(&mut a, w, h);
    has(&t, "Abundance over time");
}

#[test]
fn the_abundance_chart_waits_for_samples() {
    let mut a = app_with(frame_with_map());
    a.goto(Screen::Genotypes);
    a.geno_view = GenoView::Abundance;
    let t = render(&mut a, 80, 24);
    has(&t, "Collecting samples");
}

#[test]
fn the_lineage_view_lists_the_ancestry_founder_first() {
    let mut f = frame_with_map();
    f.lineage = vec![
        genotype(0, 8, 7, "051-bbbbb", 1),
        genotype(1, 7, 3, "050-aaaaa", 2),
        genotype(2, 3, -1, "050-ccccc", 0),
    ];
    let mut a = app_with(f);
    a.goto(Screen::Genotypes);
    a.geno_view = GenoView::Lineage;
    let t = render(&mut a, 80, 24);
    has(&t, "No lineage traced");
    a.highlight = Some(8);
    let t = render(&mut a, 80, 24);
    has(&t, "051-bbbbb #8: 3 genotypes back to the founder");
    let founder = t.lines().position(|l| l.contains("050-ccccc")).unwrap();
    let newest = t
        .lines()
        .position(|l| l.contains("051-bbbbb") && l.contains("u80"))
        .unwrap();
    assert!(founder < newest, "{t}");
    has(&t, "fitness");
    let (w, h) = MIN_SIZE;
    let t = render(&mut a, w, h);
    has(&t, "Lineage of #8");
}

#[test]
fn speculative_execution_needs_no_warning_and_a_step_inside_a_slice_says_so() {
    let mut a = debugging();
    a.set_setup(Arc::new(setup(vec![setting(
        "SPECULATIVE",
        "int",
        "1",
        "1",
    )])));
    let t = render(&mut a, 100, 24);
    assert!(!t.contains("SPECULATIVE"), "{t}");
    has(&t, "Watching organism #5");

    let mut status = crate::fixtures::status(avida_session::RunState::Paused, 12);
    status.mid_slice = true;
    a.apply_status(status);
    a.apply_events(vec![avida_session::SessionEvent::RunUntilEnded {
        outcome: avida_session::UntilOutcome::Reached,
        slices: 1,
        update: 12,
    }]);
    let t = render(&mut a, 120, 24);
    has(&t, "inside a time slice");
}

#[test]
fn births_and_restarts_reach_the_log() {
    let mut a = debugging();
    a.apply_events(vec![avida_session::SessionEvent::Born {
        parent_id: 5,
        parent_cell: 3,
        org_id: 9,
        cell: 4,
        update: 12,
        slice: 30,
    }]);
    assert!(
        a.log
            .iter()
            .any(|l| l.text.contains("#5 gave birth to #9 in cell 4")),
        "{:?}",
        a.log.iter().map(|l| &l.text).collect::<Vec<_>>()
    );

    // While a restart is pending nothing is sent and nothing old is shown.
    a.begin_restart(vec!["-s".into(), "3".into()]);
    a.send(avida_session::Command::StepSlice);
    assert!(a.take_commands().is_empty());
    let frames = a.frames_applied;
    a.apply_frame(Arc::new(crate::fixtures::frame_with_org(organism(
        5, 8, 4, 40,
    ))));
    assert_eq!(a.frames_applied, frames);
    a.apply_events(vec![avida_session::SessionEvent::RestartFailed(
        "error: bad".into(),
    )]);
    assert!(a.restarting.is_none());
    assert!(a.toasts.iter().any(|t| t.text.contains("Restart failed")));

    a.begin_restart(vec!["-s".into(), "3".into()]);
    a.apply_events(vec![avida_session::SessionEvent::Restarted(Arc::new(
        setup(vec![]),
    ))]);
    assert!(a.restarting.is_none() && a.frame.is_none());
    assert_eq!(a.args, ["-s", "3"]);
    let cmds = a.take_commands();
    assert!(cmds
        .iter()
        .any(|c| matches!(c, avida_session::Command::Subscribe(_))));
}

fn stderr(a: &mut App, n: usize) {
    a.apply_events(
        (0..n)
            .map(|i| avida_session::SessionEvent::Stderr(format!("WARNING: thing {i}")))
            .collect(),
    );
}

#[test]
fn the_header_shows_a_warning_count_and_drops_items_rather_than_overflowing() {
    let mut a = app_with(frame_with_map());
    a.apply_status(crate::fixtures::status(avida_session::RunState::Paused, 12));
    stderr(&mut a, 3);
    let t = render(&mut a, 100, 24);
    has(t.lines().next().unwrap(), "3 warnings");
    // Narrow: the update and the badge always stay, the first line never
    // runs past the width, and the name is cut with an ellipsis.
    a.name = "a-rather-long-run-directory-name".to_string();
    for w in MIN_SIZE.0..=100 {
        let t = render(&mut a, w, 24);
        let first = t.lines().next().unwrap();
        assert!(first.chars().count() <= usize::from(w), "{w}: {first:?}");
        has(first, "PAUSED");
        has(first, "update 12");
    }
    let t = render(&mut a, 60, 24);
    has(t.lines().next().unwrap(), "…");
    // One warning is singular.
    let mut b = app_with(frame_with_map());
    stderr(&mut b, 1);
    let t = render(&mut b, 100, 24);
    let first = t.lines().next().unwrap();
    has(first, "1 warning");
    assert!(!first.contains("warnings"), "{first}");
}

#[test]
fn the_footer_drops_whole_hints_and_keeps_help() {
    let mut a = app_with(frame_with_map());
    for w in [40u16, 50, 60, 80, 120] {
        let t = render(&mut a, w, 24);
        let last = t.lines().last().unwrap();
        assert!(last.chars().count() <= usize::from(w), "{w}: {last:?}");
        has(last, "? help");
        // No hint is cut in half: every word is one of the table's words.
        assert!(!last.contains('…'), "{w}: {last:?}");
    }
    let wide = render(&mut a, 120, 24);
    has(wide.lines().last().unwrap(), "Enter inspect");
    let narrow = render(&mut a, 40, 24);
    assert!(!narrow.lines().last().unwrap().contains("Enter inspect"));
}

#[test]
fn the_log_marks_kinds_without_colour_filters_problems_and_scrolls_from_the_bottom() {
    let mut a = app_with(frame_with_map());
    for i in 0..100 {
        a.log.push(crate::log::LogKind::Output, format!("out {i}"));
    }
    stderr(&mut a, 2);
    a.log.push(crate::log::LogKind::Info, "note");
    a.goto(Screen::Log);
    let t = render(&mut a, 80, 24);
    has(&t, "following");
    has(&t, "2 warnings");
    has(&t, "* note");
    // Page up from the bottom moves one page up, not to the top.
    a.on_key(key(ratatui::crossterm::event::KeyCode::PageUp));
    let t = render(&mut a, 80, 24);
    assert!(!t.contains("following"), "{t}");
    assert!(!t.contains("Log 1-"), "{t}");
    // End follows again.
    a.on_key(key(ratatui::crossterm::event::KeyCode::End));
    has(&render(&mut a, 80, 24), "following");
    // v shows only the problems.
    a.on_key(key(ratatui::crossterm::event::KeyCode::Char('v')));
    let t = render(&mut a, 80, 24);
    has(&t, "Warnings and errors");
    has(&t, "! WARNING: thing 1");
    assert!(!t.contains("out 5"), "{t}");
    a.on_key(key(ratatui::crossterm::event::KeyCode::Char('v')));
    has(&render(&mut a, 80, 24), "out 99");
}

fn key(code: ratatui::crossterm::event::KeyCode) -> ratatui::crossterm::event::KeyEvent {
    ratatui::crossterm::event::KeyEvent::new(code, ratatui::crossterm::event::KeyModifiers::NONE)
}

#[test]
fn without_colour_nothing_is_coloured_and_the_map_shows_shades() {
    use ratatui::style::Color;
    let mut a = app_with(frame_with_map());
    a.color_mode = crate::color::ColorMode::Mono;
    stderr(&mut a, 1);
    for screen in [Screen::Map, Screen::Stats, Screen::Charts, Screen::Log] {
        a.goto(screen);
        let mut t = Terminal::new(TestBackend::new(100, 30)).unwrap();
        let done = t.draw(|f| draw(f, &mut a)).unwrap();
        for cell in &done.buffer.content {
            assert_eq!(cell.fg, Color::Reset, "{screen:?}");
            assert_eq!(cell.bg, Color::Reset, "{screen:?}");
        }
    }
    // The state badge is reversed instead of coloured.
    a.goto(Screen::Map);
    let mut t = Terminal::new(TestBackend::new(100, 30)).unwrap();
    let done = t.draw(|f| draw(f, &mut a)).unwrap();
    let badge = (0..100u16)
        .map(|x| &done.buffer[(x, 0)])
        .find(|c| c.symbol() == "P")
        .expect("the badge");
    assert!(badge.modifier.contains(ratatui::style::Modifier::REVERSED));
}

#[test]
fn without_colour_the_genotype_map_shows_letters_and_says_so() {
    let mut a = app_with(frame_with_map());
    a.color_mode = crate::color::ColorMode::Mono;
    a.goto(Screen::Map);
    assert!(!a.big_cells);
    let t = render(&mut a, 100, 30);
    has(&t, "a letter per genotype");
    assert_eq!(a.geom.map(|g| g.cell_w), Some(2), "{t}");
    // A numeric layer stays on half blocks.
    a.layer = avida_session::LayerId::Merit;
    let t = render(&mut a, 100, 30);
    assert_eq!(a.geom.map(|g| g.cell_w), Some(1), "{t}");
    assert!(!t.contains("a letter per genotype"), "{t}");
}

#[test]
fn empty_cells_keep_the_terminals_own_background() {
    use ratatui::style::Color;
    let mut a = app_with(frame_with_map());
    a.color_mode = crate::color::ColorMode::TrueColor;
    a.goto(Screen::Map);
    for big in [false, true] {
        a.big_cells = big;
        let mut t = Terminal::new(TestBackend::new(100, 30)).unwrap();
        let rect = {
            t.draw(|f| draw(f, &mut a)).unwrap();
            a.geom.expect("map geometry").area
        };
        let done = t.draw(|f| draw(f, &mut a)).unwrap();
        let mut empties = 0;
        for y in rect.y..rect.y + rect.height {
            for x in rect.x..rect.x + rect.width {
                let c = &done.buffer[(x, y)];
                // The cursor cell is bold and carries its own colours.
                if c.symbol() == if big { "·" } else { " " }
                    && !c.modifier.contains(ratatui::style::Modifier::BOLD)
                {
                    empties += 1;
                    assert_eq!(c.bg, Color::Reset, "big={big} at {x},{y}");
                }
            }
        }
        assert!(empties > 0, "the fixture has empty cells (big={big})");
    }
}

#[test]
fn panels_cut_long_lines_with_an_ellipsis_and_wrap_messages() {
    let mut a = debugging();
    let t = render(&mut a, 80, 24);
    // The organism panel is 34 columns wide at this size: its long lines
    // end in an ellipsis rather than being cut mid-word.
    assert!(t.lines().any(|l| l.contains('…') && l.contains('│')), "{t}");
    // A message that does not fit wraps.
    let mut b = app_with(frame_with_map());
    b.goto(Screen::Debug);
    let t = render(&mut b, 50, 24);
    has(&t, "organism and press Enter");
}

#[test]
fn before_the_first_report_the_header_says_starting_not_paused() {
    let mut a = crate::fixtures::app();
    let t = render(&mut a, 80, 24);
    has(&t, "STARTING");
    assert!(!t.contains("PAUSED"), "{t}");
    a.apply_status(crate::fixtures::status(avida_session::RunState::Paused, 3));
    let t = render(&mut a, 80, 24);
    has(&t, "PAUSED");
}

#[test]
fn the_help_overlay_explains_the_memory_marks() {
    let mut a = app_with(frame_with_map());
    a.perform(Action::Help, false);
    a.help_scroll = 1000;
    let t = render(&mut a, 80, 24);
    has(&t, "Marks in the memory listings");
    has(&t, "I R W F");
}

fn event(action: &str, args: &str) -> avida_session::EventLine {
    avida_session::EventLine {
        text: format!("u 0:1:end {action} {args}"),
        trigger: "u".into(),
        timing: "0:1:end".into(),
        action: action.into(),
        args: args.into(),
    }
}

fn with_events(a: &mut App, events: Vec<avida_session::EventLine>) {
    let mut s = crate::config_view::tests::setup(Vec::new());
    s.events = events;
    a.setup = Some(Arc::new(s));
}

#[test]
fn the_open_endedness_screen_explains_an_empty_events_file() {
    let mut a = app_with(frame_with_map());
    with_events(&mut a, Vec::new());
    a.on_key(key(ratatui::crossterm::event::KeyCode::Char('K')));
    assert_eq!(a.screen, Screen::OpenEnded);
    let t = render(&mut a, 80, 24);
    has(&t, "has neither PrintMODESData nor");
    has(
        &t,
        "PrintPhylogenyMetrics, so there is nothing to chart yet.",
    );
    has(&t, "u 0:1:end PrintMODESData modes.dat 100 500");
    has(&t, "u 0:200:end PrintPhylogenyMetrics phylogeny.dat");
    // The tab strip names the screen and its key.
    has(&render(&mut a, 100, 30), "K Open");
}

#[test]
fn the_open_endedness_screen_says_when_modes_rows_start() {
    let mut a = app_with(frame_with_map());
    with_events(&mut a, vec![event("PrintMODESData", "modes.dat 100 500")]);
    a.goto(Screen::OpenEnded);
    let t = render(&mut a, 80, 24);
    has(&t, "No row has been written yet");
    has(&t, "Rows start after 600 generations");
}

#[test]
fn the_open_endedness_screen_charts_rows_and_explains_the_chosen_chart() {
    use avida_viewmodel::{FilterMode, ModesHistory, ModesRow, OpenEnded, PhyloHistory};
    let row = |g: i32| ModesRow {
        update: g * 10,
        generation: f64::from(g),
        snapshot_update: g * 10 - 20,
        change: i64::from(g % 3),
        novelty: 1,
        complexity: 10 + i64::from(g),
        ecology: f64::from(g) / 4.0,
        persistent: 2,
        skeletons: 2,
        living: 5,
        cumulative_novelty: 3,
    };
    let mut frame = frame_with_map();
    frame.open_ended = OpenEnded {
        modes: vec![ModesHistory {
            filename: "modes.dat".into(),
            resolution: 1,
            filter: 2,
            mode: FilterMode::Nearest,
            rows: (3..12).map(row).collect(),
        }],
        phylogeny: vec![PhyloHistory {
            filename: "phylogeny.dat".into(),
            rows: Vec::new(),
        }],
    };
    let mut a = app_with(frame);
    with_events(
        &mut a,
        vec![
            event("PrintMODESData", "modes.dat 1 2"),
            event("PrintPhylogenyMetrics", "phylogeny.dat"),
        ],
    );
    a.goto(Screen::OpenEnded);
    let t = render(&mut a, 100, 30);
    has(
        &t,
        "MODES, persistence lag t = 2 generations, nearest filter",
    );
    has(&t, "< 1/2 >");
    has(&t, "Change");
    has(&t, "Change is the number of skeletons");
    // Down chooses the next chart and its explanation replaces the first.
    a.on_key(key(ratatui::crossterm::event::KeyCode::Down));
    let t = render(&mut a, 100, 30);
    has(&t, "Novelty is the number of skeletons");
    // Right switches to the phylogeny group, which has no row yet.
    a.on_key(key(ratatui::crossterm::event::KeyCode::Right));
    let t = render(&mut a, 100, 30);
    has(&t, "Genealogy of the genotypes");
    has(&t, "No row has been written yet.");
}

#[test]
fn the_open_endedness_screen_in_a_playback_says_the_rows_are_not_recorded() {
    let mut a = app_with(frame_with_map());
    a.apply_playback(Some(avida_session::PlaybackInfo {
        path: "x.avrec".into(),
        frames: 4,
        index: 0,
        update: 0,
        first_update: 0,
        last_update: 30,
        cut_tail_bytes: 0,
        layers: Vec::new(),
    }));
    a.goto(Screen::OpenEnded);
    let t = render(&mut a, 120, 24);
    has(&t, "only available in a live run");
}

// --- the organism filter ---------------------------------------------------

mod filtering {
    use ratatui::buffer::Buffer;
    use ratatui::crossterm::event::{KeyCode, KeyEvent};
    use ratatui::style::Color;

    use super::*;
    use crate::color::ColorMode;
    use crate::filtering::Kind;
    use avida_viewmodel::{Condition, Prop};

    fn render_buf(app: &mut App, w: u16, h: u16) -> Buffer {
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        t.draw(|f| draw(f, app)).unwrap().buffer.clone()
    }

    fn key(c: KeyCode) -> KeyEvent {
        KeyEvent::new(c, ratatui::crossterm::event::KeyModifiers::NONE)
    }

    fn filtered(c: Condition) -> App {
        let mut a = app_with(frame_with_values());
        a.edit_filter(|f| f.set(c));
        a.take_commands();
        a
    }

    #[test]
    fn the_bar_names_the_conditions_and_counts_the_matches() {
        let mut a = filtered(Condition::Genotype { id: 8 });
        let t = render(&mut a, 80, 24);
        has(&t, "Filter: Genotype 051-bbbbb");
        has(&t, "1 of 3 organisms match (33%)");
        // Two conditions: both labels, the count at the right edge.
        a.edit_filter(|f| f.set(Condition::range(Prop::Fitness, 0.5, f64::INFINITY)));
        let t = render(&mut a, 80, 24);
        let bar = t.lines().find(|l| l.contains("Filter:")).unwrap();
        assert!(bar.contains("Genotype 051-bbbbb · Fitness ≥ 0.5"), "{bar}");
        assert!(bar.ends_with("match (33%)"), "{bar}");
        // A narrow terminal keeps the count and shortens the rest.
        let t = render(&mut a, 40, 12);
        let bar = t.lines().find(|l| l.contains("Filter:")).unwrap();
        assert!(bar.contains("1 of 3 match (33%)"), "{bar}");
        assert!(bar.chars().count() <= 40, "{bar}");
        // No filter, no bar.
        a.clear_filter();
        assert!(!render(&mut a, 80, 24).contains("Filter:"));
    }

    #[test]
    fn the_bar_says_when_nothing_matches_and_when_a_layer_is_missing() {
        let mut a = filtered(Condition::range(Prop::Fitness, 100.0, 200.0));
        let t = render(&mut a, 80, 24);
        has(&t, "Fitness 100 to 200");
        has(&t, "No organism matches");
        // The fixture has no merit layer.
        let mut a = filtered(Condition::range(Prop::Merit, 0.0, 1.0));
        has(&render(&mut a, 80, 24), "Merit 0 to 1 (no data)");
    }

    #[test]
    fn the_map_dims_the_organisms_the_filter_leaves_out() {
        let mut a = filtered(Condition::Genotype { id: 8 });
        let buf = render_buf(&mut a, 80, 24);
        let g = a.geom.unwrap();
        let at = |x: u16, y: u16| buf[(g.area.x + x, g.area.y + y)].clone();
        // The world is 3x2: column 0 holds genotype 7 twice, column 1
        // genotype 8 over an empty cell. Dimmed cells are grey.
        let grey = |c: Color| matches!(c, Color::Rgb(r, g, b) if r == g && g == b);
        let dimmed = at(0, 0);
        assert!(grey(dimmed.fg) && grey(dimmed.bg), "{dimmed:?}");
        let kept = at(1, 0);
        assert!(!grey(kept.fg), "{kept:?}");
        // Without the filter nothing is grey.
        a.clear_filter();
        let buf = render_buf(&mut a, 80, 24);
        let g = a.geom.unwrap();
        assert!(!grey(buf[(g.area.x, g.area.y)].fg));
    }

    #[test]
    fn dimming_survives_16_colours_and_no_colour() {
        let mut a = filtered(Condition::Genotype { id: 8 });
        a.color_mode = ColorMode::Ansi16;
        let buf = render_buf(&mut a, 80, 24);
        let g = a.geom.unwrap();
        let dim = &buf[(g.area.x, g.area.y)];
        let kept = &buf[(g.area.x + 1, g.area.y)];
        assert_eq!((dim.fg, dim.bg), (Color::DarkGray, Color::DarkGray));
        assert_ne!(kept.fg, Color::DarkGray);
        // No colour, genotypes: letters take the place of colours, and the
        // others are the light shade.
        a.color_mode = ColorMode::Mono;
        let t = render(&mut a, 80, 24);
        has(&t, "░ no match");
        let buf = render_buf(&mut a, 80, 24);
        let g = a.geom.unwrap();
        assert_eq!(buf[(g.area.x, g.area.y)].symbol(), "░");
        let kept = buf[(g.area.x + 2, g.area.y)].symbol().to_string();
        assert!(kept != "░" && kept != " " && kept != "·", "{kept:?}");
        // No colour, another layer: half blocks, light for the others.
        a.set_layer(avida_session::LayerId::Fitness);
        let buf = render_buf(&mut a, 80, 24);
        let g = a.geom.unwrap();
        assert_eq!(buf[(g.area.x, g.area.y)].symbol(), "░");
        assert_eq!(buf[(g.area.x + 1, g.area.y)].symbol(), "▀");
    }

    #[test]
    fn the_dialog_lists_kinds_and_walks_through_a_range() {
        let mut a = app_with(frame_with_values());
        a.on_key(key(KeyCode::Char('/')));
        let t = render(&mut a, 80, 24);
        has(&t, "Filter organisms");
        has(&t, "Add condition");
        has(&t, "▸ Performs task");
        has(&t, "Does not perform task");
        has(&t, "Region of the map");
        has(&t, "Esc closes");
        for _ in 0..Kind::all()
            .iter()
            .position(|&k| k == Kind::Prop(Prop::Fitness))
            .unwrap()
        {
            a.on_key(key(KeyCode::Down));
        }
        a.on_key(key(KeyCode::Enter));
        a.on_key(key(KeyCode::Char('2')));
        let t = render(&mut a, 80, 24);
        has(&t, "Merit divided by gestation time.");
        has(&t, "Now from 1 to 4.");
        has(&t, "[at least]");
        has(&t, "Value");
        has(&t, "2_");
        a.on_key(key(KeyCode::Enter));
        let t = render(&mut a, 80, 24);
        has(&t, "Fitness ≥ 2");
        has(&t, "2 of 3 organisms match");
        assert!(!t.contains("Filter organisms"), "the dialog closed:\n{t}");
        // The list of conditions.
        a.on_key(key(KeyCode::Char('/')));
        a.on_key(key(KeyCode::Tab));
        let t = render(&mut a, 80, 24);
        has(&t, "Conditions (1)");
        has(&t, "1. Fitness ≥ 2");
        has(&t, "Del removes");
    }

    #[test]
    fn the_dialog_fits_the_smallest_terminal() {
        let mut a = app_with(frame_with_values());
        a.on_key(key(KeyCode::Char('/')));
        let t = render(&mut a, 40, 10);
        has(&t, "Filter organisms");
        has(&t, "Performs task");
        for _ in 0..12 {
            a.on_key(key(KeyCode::Down));
        }
        let t = render(&mut a, 40, 10);
        has(&t, "▸ Region of the map");
    }

    #[test]
    fn the_histogram_overlays_the_matching_organisms() {
        let mut a = filtered(Condition::Genotype { id: 7 });
        a.set_layer(avida_session::LayerId::Fitness);
        a.goto(Screen::Histogram);
        let t = render(&mut a, 80, 24);
        has(&t, "Filter: Genotype 050-aaaaa");
        has(&t, "Histogram: Fitness");
        // Genotype 7 holds the fitness 1 and 2 cells; 4 belongs to genotype 8.
        assert!(t.lines().any(|l| l.contains("1 of 1")), "{t}");
        assert!(t.lines().any(|l| l.contains("0 of 1")), "{t}");
        has(&t, "█ matching the filter, ░ the others.");
        // The genotype view overlays per genotype.
        a.set_layer(avida_session::LayerId::Genotype);
        let t = render(&mut a, 80, 24);
        assert!(
            t.lines()
                .any(|l| l.contains("050-aaaaa") && l.contains("2 of 2")),
            "{t}"
        );
        assert!(
            t.lines()
                .any(|l| l.contains("051-bbbbb") && l.contains("0 of 1")),
            "{t}"
        );
    }

    #[test]
    fn the_tasks_show_both_shares() {
        let mut a = filtered(Condition::Genotype { id: 8 });
        a.goto(Screen::Stats);
        let t = render(&mut a, 80, 24);
        has(&t, "Tasks: all organisms, matching ones");
        // Cell 0 of three organisms performs the task; the one matching
        // organism does not.
        let row = t.lines().find(|l| l.contains("nand")).unwrap();
        assert!(row.contains("33%") && row.contains("0%"), "{row}");
    }

    #[test]
    fn the_time_series_gets_a_second_line_for_the_subset() {
        let mut a = filtered(Condition::Genotype { id: 7 });
        let mut next = frame_with_values();
        next.update = 13;
        a.apply_frame(Arc::new(next));
        let rows: Vec<_> = (10..14)
            .map(|u| avida_session::UpdateRow {
                update: u,
                ave_fitness: f64::from(u),
                ..Default::default()
            })
            .collect();
        a.apply_rows(rows);
        a.goto(Screen::Charts);
        let t = render(&mut a, 100, 30);
        has(&t, "Filter: Genotype 050-aaaaa");
        has(&t, "Average fitness");
        has(&t, "• matching");
        // A measure without an organism counterpart has no subset line.
        a.chart_metrics = vec![crate::metrics::Metric::Births];
        let t = render(&mut a, 100, 30);
        has(&t, "no line for the matching organisms");
    }

    #[test]
    fn the_help_and_palette_list_the_filter() {
        let mut a = app_with(frame_with_values());
        a.show_help = true;
        a.help_scroll = 1000;
        let t = render(&mut a, 100, 40);
        has(&t, "Filter");
        has(&t, "Filter organisms: add a condition");
        has(&t, "Clear the organism filter");
        a.show_help = false;
        a.on_key(key(KeyCode::Char(':')));
        for c in "filter".chars() {
            a.on_key(key(KeyCode::Char(c)));
        }
        let t = render(&mut a, 80, 24);
        has(&t, "Filter: Filter organisms");
        has(&t, "Filter: Clear the organism filter");
        has(&t, "Filter: Show or remove the filter's conditions");
        // Enter runs the first match.
        for _ in 0.."filter".len() {
            a.on_key(key(KeyCode::Backspace));
        }
        for c in "add a cond".chars() {
            a.on_key(key(KeyCode::Char(c)));
        }
        a.on_key(key(KeyCode::Enter));
        assert!(a.filtering.dialog.is_some());
    }
}
