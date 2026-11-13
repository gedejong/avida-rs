//! The real application loop against a `TestBackend`, with injected key
//! events, on the `heads_default_100u` world.

mod common;

use avida_session::RunState;
use common::*;
use ratatui::crossterm::event::KeyCode;

fn tour(w: u16, h: u16) {
    let size = format!("{w}x{h}");
    let dir = fresh_config(&format!("tour_{size}"));
    let name = dir.file_name().unwrap().to_string_lossy().into_owned();
    let snap = |n: &str| Step::Snap(n.to_string());
    let steps = vec![
        Step::Until(
            "first frame",
            Box::new(|a| a.frame.as_ref().is_some_and(|f| f.map.is_some())),
        ),
        Step::Check(Box::new(move |a, t| {
            has(t, "PAUSED");
            has(t, &name);
            has(t, "M Map");
            has(t, "Genotype");
            has(t, "(30,30) cell 1830");
            assert_eq!(a.run_state(), RunState::Paused);
        })),
        snap("01_map_initial"),
        // Step three updates.
        Step::Keys("..."),
        Step::Until("three rows", Box::new(|a| a.history.len() >= 3)),
        Step::Check(Box::new(|a, t| {
            has(t, "update 2");
            assert_eq!(a.run_state(), RunState::Paused);
        })),
        Step::Keys("]"),
        Step::Until(
            "fitness layer",
            Box::new(|a| {
                a.frame
                    .as_ref()
                    .and_then(|f| f.map.as_ref())
                    .is_some_and(|m| m.layer(avida_session::LayerId::Fitness).is_some())
            }),
        ),
        Step::Check(Box::new(|_, t| {
            has(t, "Fitness");
        })),
        snap("02_map_fitness"),
        Step::Keys("["),
        Step::Keys("i"),
        snap("03_map_big_cells"),
        Step::Keys("i"),
        // Stats.
        Step::Keys("S"),
        Step::Check(Box::new(|_, t| {
            has(t, "Average fitness");
            has(t, "Organisms");
            has(t, "Tasks");
            assert!(
                t.chars().any(|c| ('▁'..='█').contains(&c)),
                "no sparkline:\n{t}"
            );
        })),
        snap("04_stats"),
        // Charts: add organisms to the chart.
        Step::Keys("G"),
        Step::Event(key(KeyCode::Down)),
        Step::Event(key(KeyCode::Down)),
        Step::Event(key(KeyCode::Down)),
        Step::Event(key(KeyCode::Down)),
        Step::Event(key(KeyCode::Up)),
        Step::Event(key(KeyCode::Up)),
        Step::Event(key(KeyCode::Up)),
        Step::Event(key(KeyCode::Up)),
        Step::Event(key(KeyCode::Enter)),
        Step::Check(Box::new(|a, t| {
            has(t, "Charts");
            has(t, "update");
            assert_eq!(a.chart_metrics.len(), 2);
            assert!(
                t.chars().any(|c| ('\u{2801}'..='\u{28ff}').contains(&c)),
                "no plot:\n{t}"
            );
        })),
        snap("05_charts"),
        // Genotype table.
        Step::Keys("T"),
        Step::Check(Box::new(|a, t| {
            has(t, "Genotypes (");
            has(t, "count");
            let g = &a.frame.as_ref().unwrap().genotypes[0];
            has(t, &g.name);
        })),
        snap("06_genotypes"),
        // Zoom selects the cursor's organism.
        Step::Keys("CZ"),
        Step::Until(
            "inspected organism",
            Box::new(|a| a.frame.as_ref().is_some_and(|f| f.inspect.is_some())),
        ),
        Step::Check(Box::new(|_, t| {
            has(t, "in cell 0 (0,0)");
            has(t, "h-alloc");
            has(t, "Memory");
            has(t, "AX");
            has(t, "heads");
        })),
        snap("07_zoom"),
        // Environment, with its three sub-views.
        Step::Keys("E"),
        Step::Check(Box::new(|_, t| has(t, "Resources"))),
        snap("08_env_resources"),
        Step::Event(key(KeyCode::Right)),
        Step::Check(Box::new(|_, t| has(t, "last count"))),
        snap("09_env_reactions"),
        Step::Event(key(KeyCode::Right)),
        Step::Check(Box::new(|_, t| {
            has(t, "description");
            has(t, "nand");
        })),
        snap("10_env_tasks"),
        // Histogram.
        Step::Keys("H"),
        Step::Check(Box::new(|_, t| {
            has(t, "Genotype abundance");
            has(t, "Bars are drawn to scale");
        })),
        snap("11_hist_genotype"),
        Step::Keys("]"),
        Step::Until(
            "fitness values",
            Box::new(|a| {
                a.frame
                    .as_ref()
                    .and_then(|f| f.map.as_ref())
                    .is_some_and(|m| m.layer(avida_session::LayerId::Fitness).is_some())
            }),
        ),
        Step::Check(Box::new(|_, t| has(t, "Histogram: Fitness"))),
        snap("12_hist_fitness"),
        Step::Keys("["),
        // Log, fed by session events.
        Step::Keys("D"),
        Step::Check(Box::new(|a, t| {
            assert!(!a.log.is_empty());
            has(t, "Log ");
        })),
        snap("13_log"),
        Step::Keys("O"),
        Step::Check(Box::new(|_, t| {
            has(t, "World size");
            has(t, "60x60");
            has(t, "Random seed");
        })),
        snap("14_options"),
        Step::Keys("B"),
        Step::Check(Box::new(|_, t| has(t, "Blank screen"))),
        Step::Keys("?"),
        Step::Check(Box::new(|_, t| {
            has(t, "Keys ");
            has(t, "Run or pause");
            has(t, "Step one update");
        })),
        snap("15_help"),
        Step::Event(key(KeyCode::Esc)),
        Step::Event(key(KeyCode::Esc)),
        Step::Check(Box::new(|a, t| {
            assert!(!a.show_help);
            has(t, "(0,0) cell 0");
        })),
        // Saving the population while paused writes the file and logs it.
        Step::Keys("W"),
        Step::Until(
            "population saved",
            Box::new(|a| a.toasts.iter().any(|t| t.text.contains("Population saved"))),
        ),
        Step::Check(Box::new(|_, t| has(t, "Population saved"))),
    ];
    let run = run_script(&dir, &[], w, h, steps, &size);
    assert_eq!(run.driver.app.history.len(), 3);
    let saved: Vec<_> = std::fs::read_dir(dir.join("data"))
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().ends_with(".spop"))
        .collect();
    assert_eq!(saved.len(), 1);
}

#[test]
fn every_screen_at_80x24() {
    tour(80, 24);
}

#[test]
fn every_screen_at_160x50() {
    tour(160, 50);
}

#[test]
fn run_to_the_end_of_the_run() {
    let dir = fresh_config("finish");
    let steps = vec![
        Step::Until("first frame", Box::new(|a| a.frame.is_some())),
        Step::Keys(" "),
        // The session publishes the finished status, then the last frame,
        // then the `Finished` event (the toast): wait for the event, not
        // only the status.
        Step::Until(
            "finished",
            Box::new(|a| {
                a.run_state() == RunState::Finished
                    && a.history.len() >= 101
                    && a.toasts.iter().any(|t| t.text.contains("finished"))
            }),
        ),
        Step::Check(Box::new(|a, t| {
            has(t, "FINISHED");
            has(t, "update 100");
            assert_eq!(a.history.len(), 101);
            assert!(a.toasts.iter().any(|t| t.text.contains("finished")));
        })),
        Step::Snap("finished_map".into()),
        // Nothing advances a finished run; the app says so.
        Step::Keys("."),
        Step::Check(Box::new(|_, t| has(t, "nothing can advance it"))),
        // The last frame stays inspectable.
        Step::Keys("S"),
        Step::Check(Box::new(|_, t| has(t, "Average fitness"))),
        Step::Snap("finished_stats".into()),
        Step::Keys("G"),
        Step::Check(Box::new(|_, t| has(t, "Charts"))),
        Step::Snap("finished_charts".into()),
        Step::Keys("CZ"),
        Step::Until(
            "organism",
            Box::new(|a| a.frame.as_ref().is_some_and(|f| f.inspect.is_some())),
        ),
        Step::Check(Box::new(|_, t| has(t, "Memory"))),
    ];
    let run = run_script(&dir, &[], 100, 30, steps, "100x30");
    let rows = run.driver.app.history.rows();
    assert_eq!(rows.first().map(|r| r.update), Some(0));
    assert_eq!(rows.last().map(|r| r.update), Some(100));
    assert!(rows.last().unwrap().orgs > 1);
}

#[test]
fn cursor_scrolling_click_and_organism_inspection() {
    let dir = fresh_config("inspect");
    let down = || shift_key(KeyCode::Down);
    let up = || shift_key(KeyCode::Up);
    let mut steps = vec![
        Step::Until(
            "first frame",
            Box::new(|a| a.frame.as_ref().is_some_and(|f| f.map.is_some())),
        ),
        Step::Keys("."),
        Step::Until("first update", Box::new(|a| !a.history.is_empty())),
        // The ancestor is injected into cell 0; C finds it from the middle.
        Step::Keys("C"),
        Step::Until(
            "inspect",
            Box::new(|a| a.frame.as_ref().is_some_and(|f| f.inspect.is_some())),
        ),
        Step::Check(Box::new(|a, t| {
            assert_eq!(a.cursor, (0, 0));
            assert_eq!(a.selection, Some(0));
            has(t, "[inspected]");
        })),
        Step::Keys("l"),
        Step::Check(Box::new(|a, t| {
            assert_eq!(a.cursor, (1, 0));
            has(t, "(1,0) cell 1  empty");
        })),
    ];
    // Shift-Down: 5 cells a press, clamped at the bottom row; the viewport
    // scrolls with the cursor.
    for _ in 0..12 {
        steps.push(Step::Event(down()));
    }
    steps.push(Step::Check(Box::new(|a, t| {
        assert_eq!(a.cursor, (1, 59));
        let g = a.geom.unwrap();
        assert!(g.oy > 0, "viewport did not scroll: {g:?}");
        assert!(g.oy <= 59 && 59 < g.oy + g.vis_h);
        has(t, "(1,59)");
    })));
    steps.push(Step::Snap("scrolled".into()));
    for _ in 0..12 {
        steps.push(Step::Event(up()));
    }
    steps.extend([
        Step::Event(key(KeyCode::Esc)),
        Step::Until(
            "deselected",
            Box::new(|a| a.frame.as_ref().is_some_and(|f| f.inspect.is_none())),
        ),
        Step::Check(Box::new(|a, _| {
            assert_eq!(a.cursor, (1, 0));
            assert_eq!(a.selection, None);
            assert_eq!(a.geom.unwrap().oy, 0);
        })),
        // Click on world cell (0, 0).
        Step::EventFrom(Box::new(|a| {
            let g = a.geom.unwrap();
            let col = g.area.x + ((0 - g.ox) as u16) * g.cell_w;
            let row = g.area.y + ((0 - g.oy) / usize::from(g.cell_rows)) as u16;
            click(col, row)
        })),
        Step::Until(
            "click inspects",
            Box::new(|a| {
                a.cursor == (0, 0)
                    && a.frame
                        .as_ref()
                        .and_then(|f| f.inspect.as_ref())
                        .is_some_and(|o| o.cell == 0)
            }),
        ),
        // Open the inspector with Enter; step slices and watch the IP move.
        Step::Event(key(KeyCode::Enter)),
        Step::Check(Box::new(|a, t| {
            assert_eq!(a.screen, avida_tui::app::Screen::Zoom);
            has(t, "in cell 0 (0,0)");
            has(t, "Memory");
        })),
        Step::Keys(",,,,,"),
        Step::Until(
            "ip moved",
            Box::new(|a| {
                a.frame
                    .as_ref()
                    .and_then(|f| f.inspect.as_ref())
                    .is_some_and(|o| o.cpu.threads[0].heads[0].pos > 0)
            }),
        ),
        Step::Check(Box::new(|a, t| {
            let o = a.frame.as_ref().unwrap().inspect.as_ref().unwrap();
            let ip = o.cpu.threads[0].heads[0].pos;
            // The listing marks the IP row: "I" in the marker column, then
            // the site number.
            has(t, &format!("I    {ip:>3} "));
            has(t, "stack");
        })),
        Step::Snap("zoom_stepped".into()),
        // Extract the genome.
        Step::Keys("X"),
        Step::Until(
            "extracted",
            Box::new(|a| a.toasts.iter().any(|t| t.text.starts_with("Wrote"))),
        ),
    ]);
    let run = run_script(&dir, &[], 80, 24, steps, "inspect");
    let extracted: Vec<_> = std::fs::read_dir(dir.join("data"))
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with("extracted-"))
        .collect();
    assert_eq!(extracted.len(), 1);
    let text = std::fs::read_to_string(extracted[0].path()).unwrap();
    assert!(text.starts_with("h-alloc\n"), "{text}");
    assert_eq!(text.lines().count(), 100);
    drop(run);
}

#[test]
fn a_fatal_error_is_shown_and_the_last_frame_stays_inspectable() {
    let dir = fresh_config("fatal");
    let events = dir.join("events.cfg");
    let text = std::fs::read_to_string(&events).unwrap();
    std::fs::write(&events, format!("{text}u 3 Inject missing.org\n")).unwrap();
    let steps = vec![
        Step::Until("first frame", Box::new(|a| a.frame.is_some())),
        Step::Keys(" "),
        Step::Until(
            "fatal",
            Box::new(|a| {
                a.fatal.is_some()
                    && a.status
                        .as_ref()
                        .is_some_and(|s| s.state == RunState::Failed)
            }),
        ),
        Step::Check(Box::new(|a, t| {
            has(t, "FAILED");
            has(t, "Press D for the log");
            assert_eq!(a.run_state(), RunState::Failed);
        })),
        Step::Snap("fatal_map".into()),
        Step::Keys("D"),
        Step::Check(Box::new(|_, t| has(t, "Log "))),
        Step::Snap("fatal_log".into()),
        Step::Keys("M"),
        Step::Check(Box::new(|_, t| {
            has(t, "Press D for the log");
            has(t, "cell ");
        })),
        Step::Keys(" "),
        Step::Check(Box::new(|_, t| has(t, "stopped on an error"))),
        Step::Keys("CZ"),
        Step::Until(
            "organism",
            Box::new(|a| a.frame.as_ref().is_some_and(|f| f.inspect.is_some())),
        ),
    ];
    let run = run_script(&dir, &[], 100, 30, steps, "100x30");
    assert!(run.driver.app.fatal.is_some());
}

#[test]
fn resizing_relayouts_and_a_tiny_terminal_is_handled() {
    use avida_tui::color::ColorMode;
    use avida_tui::runner::Driver;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    let dir = fresh_config("resize");
    let mut driver = Driver::start(dir, vec![], ColorMode::TrueColor).unwrap();
    let start = std::time::Instant::now();
    while driver.app.frame.as_ref().is_none_or(|f| f.map.is_none()) {
        driver.pump();
        assert!(start.elapsed() < TIMEOUT);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal
        .draw(|f| avida_tui::ui::draw(f, &mut driver.app))
        .unwrap();
    let small = text_of(terminal.backend().buffer());
    assert!(!small.contains("Top genotypes"));
    let g_small = driver.app.geom.unwrap();

    terminal.backend_mut().resize(160, 50);
    terminal
        .draw(|f| avida_tui::ui::draw(f, &mut driver.app))
        .unwrap();
    let big = text_of(terminal.backend().buffer());
    has(&big, "Top genotypes");
    has(&big, "Layers [ ]");
    let g_big = driver.app.geom.unwrap();
    assert!(g_big.vis_h > g_small.vis_h);
    assert_eq!(g_big.vis_w, 60);

    terminal.backend_mut().resize(30, 8);
    terminal
        .draw(|f| avida_tui::ui::draw(f, &mut driver.app))
        .unwrap();
    has(&text_of(terminal.backend().buffer()), "Terminal too small");

    // Every screen survives a small terminal.
    terminal.backend_mut().resize(40, 10);
    for s in avida_tui::app::Screen::TABS {
        driver.app.screen = s;
        terminal
            .draw(|f| avida_tui::ui::draw(f, &mut driver.app))
            .unwrap();
    }
}

#[test]
fn the_256_colour_fallback_draws_no_rgb_colours() {
    use avida_tui::color::ColorMode;
    use avida_tui::runner::Driver;
    use ratatui::backend::TestBackend;
    use ratatui::style::Color;
    use ratatui::Terminal;

    let dir = fresh_config("indexed");
    let mut driver = Driver::start(dir, vec![], ColorMode::Indexed256).unwrap();
    let start = std::time::Instant::now();
    while driver.app.frame.as_ref().is_none_or(|f| f.map.is_none()) {
        driver.pump();
        assert!(start.elapsed() < TIMEOUT);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    for s in [
        avida_tui::app::Screen::Map,
        avida_tui::app::Screen::Genotypes,
        avida_tui::app::Screen::Stats,
    ] {
        driver.app.screen = s;
        terminal
            .draw(|f| avida_tui::ui::draw(f, &mut driver.app))
            .unwrap();
        let buf = terminal.backend().buffer();
        let rgb = buf
            .content()
            .iter()
            .any(|c| matches!(c.fg, Color::Rgb(..)) || matches!(c.bg, Color::Rgb(..)));
        assert!(!rgb, "{s:?} used 24-bit colour");
    }
    driver.app.screen = avida_tui::app::Screen::Map;
    terminal
        .draw(|f| avida_tui::ui::draw(f, &mut driver.app))
        .unwrap();
    let buf = terminal.backend().buffer();
    assert!(buf
        .content()
        .iter()
        .any(|c| matches!(c.fg, Color::Indexed(_))));
}
