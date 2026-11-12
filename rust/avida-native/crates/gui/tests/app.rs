//! Headless tests of the real application (egui_kittest), driven through
//! its widgets against copies of `heads_default_100u/config`.

mod common;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use avida_gui::model::debugger::Fate;
use avida_gui::model::vision::ColorVision;
use avida_gui::model::{Phase, SideTab, SpeedSetting};
use avida_gui::settings::ThemeChoice;
use avida_gui::ui::inspector;
use avida_gui::StartupArgs;
use avida_session::{LayerId, RunState, UntilOutcome};
use avida_viewmodel::{Palette, Scale};
use common::{fresh_config, gene_config, harness, screenshot, settle, wait_until};
use egui::{vec2, Key, Modifiers, Role};
use egui_kittest::kittest::{by, NodeT, Queryable};

fn open_args(dir: &std::path::Path) -> StartupArgs {
    StartupArgs {
        dir: Some(dir.to_path_buf()),
        avida_args: Vec::new(),
    }
}

fn open_with(dir: &Path, args: &[&str]) -> StartupArgs {
    StartupArgs {
        dir: Some(dir.to_path_buf()),
        avida_args: args.iter().map(|a| a.to_string()).collect(),
    }
}

fn ready(app: &avida_gui::AvidaApp) -> bool {
    let s = app.state();
    s.phase == Phase::Ready && s.frame.as_ref().is_some_and(|f| f.map.is_some())
}

type H = egui_kittest::Harness<'static, avida_gui::AvidaApp>;

/// Clicks a cell of the map where it is drawn now.
fn click_cell(h: &mut H, cell: usize) {
    let w = h.state().map_image().map_or(1, |m| m.w);
    let canvas = h.get_by_label("Population map").rect();
    let vp = h.state().state().viewport;
    let target = vp.cell_rect(canvas, cell % w, cell / w).center();
    h.hover_at(target);
    h.step();
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos: target,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
}

/// The view of the watched organism, once the debugger has one.
fn watched(h: &mut H, cell: i32) -> avida_viewmodel::OrganismView {
    wait_until(h, "the watched organism", |app| {
        let s = app.state();
        s.selection == Some(cell) && s.debugger.current.as_ref().is_some_and(|o| o.cell == cell)
    });
    h.state().state().debugger.current.clone().unwrap()
}

/// Waits for the debugger's next stop.
fn stopped(h: &mut H) -> UntilOutcome {
    h.state_mut().state_mut().debugger.last_stop = None;
    wait_until(h, "the debugger to stop", |app| {
        let s = app.state();
        s.debugger.last_stop.is_some()
            && s.status.as_ref().is_some_and(|st| st.run_until.is_none())
            && s.run_state() != Some(RunState::Running)
    });
    h.state()
        .state()
        .debugger
        .last_stop
        .as_ref()
        .unwrap()
        .outcome
}

/// A path dropped on the window.
#[derive(Debug)]
struct Dropped(PathBuf);

impl egui::DroppedFile for Dropped {
    fn path(&self) -> &Path {
        &self.0
    }

    fn bytes(&self) -> Result<Vec<u8>, String> {
        std::fs::read(&self.0).map_err(|e| e.to_string())
    }
}

#[test]
fn opens_from_the_start_screen_steps_and_switches_views() {
    let dir = fresh_config("start-and-step");
    let mut h = harness(StartupArgs::default(), vec2(1280.0, 800.0));

    // The start screen.
    h.get_by_label("Open folder…");
    h.get_by_label("New experiment…");
    assert!(
        h.query_by_label("RECENT").is_none(),
        "no recent card without recent experiments"
    );
    screenshot(&mut h, "start-dark");

    // Paste the folder and open it.
    h.get_by_role(Role::TextInput).click();
    h.step();
    h.get_by_role(Role::TextInput)
        .type_text(dir.to_str().unwrap());
    h.step();
    h.get_by_role_and_label(Role::Button, "Open").click();
    wait_until(&mut h, "the world", ready);
    assert_eq!(
        h.state().settings().recent.first(),
        Some(&std::path::absolute(&dir).unwrap())
    );
    h.get_by_label("Paused");
    h.get_by_label("Population map");
    h.check_accessibility();

    // Step three updates.
    for n in 0..3 {
        h.get_by_label("Step update").click();
        wait_until(&mut h, "the next update", |app| {
            let s = app.state();
            s.history.latest().is_some_and(|r| r.update == n) && s.in_progress().is_none()
        });
    }
    let updates: Vec<i32> = h
        .state()
        .state()
        .history
        .rows()
        .iter()
        .map(|r| r.update)
        .collect();
    assert_eq!(updates, vec![0, 1, 2]);

    // One time slice: paused inside update 3, labelled as such.
    h.get_by_label("Step slice").click();
    wait_until(&mut h, "a slice", |app| app.state().in_progress().is_some());
    h.get_by_label_contains("Paused inside update 3");
    h.get_by_label_contains("update 3 in progress");

    // Keyboard: `2` shows the fitness layer, `.` finishes the update.
    h.key_press(Key::Num2);
    wait_until(&mut h, "the fitness layer", |app| {
        app.map_image().is_some_and(|m| m.layer == LayerId::Fitness)
    });
    assert_eq!(h.state().state().map.layer, LayerId::Fitness);
    h.key_press(Key::Period);
    wait_until(&mut h, "update 3 to end", |app| {
        app.state().history.latest().is_some_and(|r| r.update == 3)
    });

    // Every side tab.
    for tab in SideTab::ALL {
        h.get_by_role_and_label(Role::Tab, tab.title()).click();
        settle(&mut h);
        assert_eq!(h.state().state().side_tab, tab);
    }
    h.get_by_role_and_label(Role::Tab, "Environment").click();
    settle(&mut h);
    h.get_by_label("Resources · 0");
    h.get_by_role_and_label(Role::Tab, "Stats").click();
    settle(&mut h);
    h.get_by_label("Statistics of update 3");
    h.check_accessibility();
    screenshot(&mut h, "stepped-dark");

    // Shortcut overlay.
    h.key_press(Key::Questionmark);
    settle(&mut h);
    assert!(h.state().state().show_shortcuts);
    h.get_by_label("Run or pause");
    screenshot(&mut h, "shortcuts-dark");
    h.key_press(Key::Escape);
    settle(&mut h);
    assert!(!h.state().state().show_shortcuts);
    // Setup warnings on stderr are not errors.
    assert_eq!(h.state().state().log.error_count(), 0);
}

#[test]
fn runs_to_the_end_and_everything_stays_inspectable() {
    let dir = fresh_config("run-to-end");
    let mut h = harness(open_args(&dir), vec2(1440.0, 900.0));
    wait_until(&mut h, "the world", ready);

    // Limit the speed through the speed popover, so the run is slow
    // enough to watch.
    h.get_by_label("Speed").click();
    settle(&mut h);
    h.get_by_label("Limited").click();
    settle(&mut h);
    assert!(h.state().state().speed.limited);
    h.key_press(Key::Escape);
    settle(&mut h);
    h.get_by_role_and_label(Role::Button, "Run").click();
    wait_until(&mut h, "running", |app| {
        app.state().run_state() == Some(RunState::Running) && app.state().history.len() >= 3
    });
    h.get_by_label("Running");
    h.get_by_role_and_label(Role::Button, "Pause");
    screenshot(&mut h, "running-limited-dark");

    // Time the UI frames while the world runs (frames include draining
    // rows and events, re-rendering the map and the side panel).
    let mut frame_ms = Vec::new();
    for _ in 0..60 {
        let t0 = Instant::now();
        h.step();
        frame_ms.push(t0.elapsed().as_secs_f64() * 1000.0);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    frame_ms.sort_by(f64::total_cmp);
    let median = frame_ms[frame_ms.len() / 2];
    eprintln!(
        "UI frame while running (debug build): median {median:.1} ms, max {:.1} ms over {} frames",
        frame_ms.last().unwrap(),
        frame_ms.len()
    );
    assert!(
        median < 100.0,
        "UI frames too slow while running: {median:.1} ms"
    );

    // Back to maximum speed.
    h.get_by_label("Speed").click();
    settle(&mut h);
    h.get_by_label("Max").click();
    settle(&mut h);
    assert!(!h.state().state().speed.limited);
    wait_until(&mut h, "the end of the run", |app| {
        app.state().run_state() == Some(RunState::Finished)
    });
    h.get_by_label("Finished");
    // The banner over the map (the status bar may repeat the log line).
    assert!(h
        .query_all_by_label("The run finished at update 100.")
        .next()
        .is_some());
    h.get_by_label("Exit status 0");
    assert_eq!(
        h.state().state().history.latest().map(|r| r.update),
        Some(100)
    );
    assert!(h.state().state().history.len() >= 101);

    // Finished: stepping is refused without a panic.
    h.key_press(Key::Period);
    settle(&mut h);
    assert!(h
        .state()
        .state()
        .toasts
        .iter()
        .any(|t| t.text.contains("finished")));

    // Select a cell with the dominant genotype by clicking it on the map.
    let cell = {
        let s = h.state().state();
        let f = s.frame.as_ref().unwrap();
        let map = f.map.as_ref().unwrap();
        let dom = f.dominant.as_ref().unwrap().row.id;
        map.first_cell_of(dom).unwrap() as usize
    };
    click_cell(&mut h, cell);
    wait_until(&mut h, "the inspected organism", |app| {
        let s = app.state();
        s.selection == Some(cell as i32)
            && s.frame
                .as_ref()
                .and_then(|f| f.inspect.as_ref())
                .is_some_and(|o| o.cell == cell as i32)
    });
    assert_eq!(h.state().state().side_tab, SideTab::Organism);
    h.get_by_label_contains("Organism #");
    h.get_by_label_contains("Memory ·");
    // Finished: the debugger cannot step.
    assert!(h.get_by_label("Step").accesskit_node().is_disabled());
    h.state_mut().state_mut().toasts.clear();
    h.key_press(Key::I);
    settle(&mut h);
    assert!(h
        .state()
        .state()
        .toasts
        .iter()
        .any(|t| t.text.contains("finished")));
    h.check_accessibility();
    screenshot(&mut h, "finished-inspector-dark");

    // Fitness layer, cividis, linear scale, locked legend.
    h.key_press(Key::Num2);
    wait_until(&mut h, "the fitness layer", |app| {
        app.map_image().is_some_and(|m| m.layer == LayerId::Fitness)
    });
    h.get_by_label("Palette").click();
    settle(&mut h);
    h.get_by_label("Cividis").click();
    settle(&mut h);
    assert_eq!(h.state().state().map.palette, Palette::Cividis);
    h.get_by_label("Linear").click();
    settle(&mut h);
    assert_eq!(h.state().state().map.scale(LayerId::Fitness), Scale::Linear);
    h.get_by_label("Lock legend range").click();
    settle(&mut h);
    assert!(h.state().state().map.is_locked(LayerId::Fitness));
    assert_eq!(h.map_image_palette(), Some(Palette::Cividis));

    // Genotypes: the abundance chart; highlight the dominant one, which
    // traces its lineage.
    h.get_by_role_and_label(Role::Tab, "Genotypes").click();
    settle(&mut h);
    h.get_by_label("Dominant");
    h.get_by_label("Abundance chart");
    assert!(h.state().state().abundance.len() > 10);
    h.get_by_label("Highlight on map").click();
    let dom = h.state().state().dominant_id().unwrap();
    wait_until(&mut h, "the lineage", |app| {
        app.state()
            .frame
            .as_ref()
            .is_some_and(|f| f.lineage.first().is_some_and(|g| g.id == dom))
    });
    assert_eq!(h.state().state().highlight, Some(dom));
    h.get_by_label_contains("Lineage of");
    // The dominant genotype of a short run is often the founder itself.
    h.get_by_label_contains("founder");
    // A short run is still dominated by the founder; the deepest living
    // genotype has a real ancestry, one row per generation of genotypes.
    let deepest = {
        let frame = h.state().state().frame.as_ref().unwrap();
        frame
            .genotypes
            .iter()
            .max_by_key(|g| g.depth)
            .unwrap()
            .clone()
    };
    h.state_mut().state_mut().set_highlight(Some(deepest.id));
    wait_until(&mut h, "the deeper lineage", |app| {
        app.state()
            .frame
            .as_ref()
            .is_some_and(|f| f.lineage.first().is_some_and(|g| g.id == deepest.id))
    });
    let lineage = h.state().state().frame.as_ref().unwrap().lineage.clone();
    assert_eq!(lineage.len(), deepest.depth as usize + 1, "{lineage:?}");
    if deepest.depth > 0 {
        settle(&mut h);
        h.get_by_label_contains("back to the founder");
    }
    assert_eq!(lineage.last().unwrap().parent_id, -1, "ends at the founder");
    h.get_by_label("Sort by fitness").click();
    settle(&mut h);
    screenshot(&mut h, "finished-genotypes-dark");

    // The colour-vision preview changes the map, not the exported colours.
    h.get_by_label("Colour vision").click();
    settle(&mut h);
    h.get_by_label("Deuteranopia").click();
    wait_until(&mut h, "the preview", |app| {
        app.map_image()
            .is_some_and(|m| m.vision == ColorVision::Deuteranopia)
    });
    h.get_by_label("Deuteranopia preview");
    screenshot(&mut h, "finished-cvd-deuteranopia-dark");
    h.get_by_label("Colour vision").click();
    settle(&mut h);
    h.get_by_label("Normal vision").click();
    wait_until(&mut h, "true colours", |app| {
        app.map_image()
            .is_some_and(|m| m.vision == ColorVision::Normal)
    });

    // Charts: time series, tasks, histogram.
    h.get_by_role_and_label(Role::Tab, "Charts").click();
    settle(&mut h);
    h.get_by_label("Average fitness");
    screenshot(&mut h, "finished-charts-dark");
    h.get_by_label("Tasks").click();
    settle(&mut h);
    h.get_by_label("Histogram").click();
    settle(&mut h);
    h.get_by_label_contains("cells counted");

    // Light theme (System -> Light).
    h.get_by_label("Theme").click();
    settle(&mut h);
    assert_eq!(h.state().settings().theme, ThemeChoice::Light);
    screenshot(&mut h, "finished-histogram-light");
    h.get_by_role_and_label(Role::Tab, "Genotypes").click();
    settle(&mut h);
    screenshot(&mut h, "finished-lineage-light");
    h.get_by_role_and_label(Role::Tab, "Stats").click();
    h.key_press(Key::Num1);
    h.key_press(Key::Escape);
    wait_until(&mut h, "the genotype layer", |app| {
        app.map_image()
            .is_some_and(|m| m.layer == LayerId::Genotype)
    });
    assert!(h.state().state().highlight.is_none());
    screenshot(&mut h, "finished-stats-light");

    // Exports through the experiment menu (no native dialog in tests).
    h.get_by_label("Experiment menu").click();
    settle(&mut h);
    h.get_by_label("Export map as PNG…").click();
    settle(&mut h);
    h.get_by_label("Experiment menu").click();
    settle(&mut h);
    h.get_by_label("Export statistics as CSV…").click();
    settle(&mut h);
    h.get_by_label("Experiment menu").click();
    settle(&mut h);
    h.get_by_label("Export the genotype table as CSV…").click();
    settle(&mut h);
    h.get_by_label("Experiment menu").click();
    settle(&mut h);
    h.get_by_label("Save the charts as PNG…").click();
    // Written on helper threads; the charts once they have been drawn.
    wait_until(&mut h, "the exports", |app| app.exports_pending() == 0);
    assert_eq!(h.state().state().side_tab, SideTab::Charts);
    let saved = h
        .state()
        .state()
        .log
        .entries()
        .filter(|e| e.text.contains("was saved to"))
        .count();
    assert_eq!(saved, 4, "map, statistics, genotypes and charts");
    let data = dir.join("data");
    let png = data.join("map-genotype-u100.png");
    let csv = data.join("stats-u100.csv");
    let genotypes = std::fs::read_to_string(data.join("genotypes-u100.csv")).unwrap();
    assert!(genotypes.starts_with("rank,id,name,"));
    assert!(genotypes.lines().count() > 1);
    let charts = image::open(data.join("charts-u100.png")).unwrap();
    assert!(
        charts.width() > 200 && charts.height() > 200,
        "{:?}",
        (charts.width(), charts.height())
    );
    assert!(png.is_file(), "{}", png.display());
    let csv_text = std::fs::read_to_string(&csv).unwrap();
    assert_eq!(csv_text.lines().count(), 102, "header and updates 0..=100");
    assert!(csv_text.starts_with("update,orgs,"));
    let img = image::open(&png).unwrap();
    assert_eq!((img.width(), img.height()), (1080, 1080));

    // Save the population with Cmd/Ctrl+S.
    h.key_press_modifiers(egui::Modifiers::COMMAND, Key::S);
    wait_until(&mut h, "the saved population", |app| {
        app.state()
            .log
            .entries()
            .any(|e| e.text.contains("Population saved"))
    });
    assert!(data.join("detail-100.spop").is_file());

    // Close: back to the start screen, with the folder in the recent list.
    h.get_by_label("Experiment menu").click();
    settle(&mut h);
    h.get_by_label("Close experiment").click();
    settle(&mut h);
    assert_eq!(h.state().state().phase, Phase::Start);
    let name = format!(
        "Open {}",
        avida_gui::model::display_name(&std::path::absolute(&dir).unwrap())
    );
    h.get_by_label(&name);
    screenshot(&mut h, "start-recent-light");
}

#[test]
fn a_fatal_error_is_shown_and_the_last_frame_stays() {
    let dir = fresh_config("fatal");
    let text = std::fs::read_to_string(dir.join("events.cfg")).unwrap();
    std::fs::write(
        dir.join("events.cfg"),
        format!("{text}u 3 Inject missing.org\n"),
    )
    .unwrap();
    let mut h = harness(open_args(&dir), vec2(1000.0, 650.0));
    wait_until(&mut h, "the world", ready);
    h.key_press(Key::Space);
    wait_until(&mut h, "the fatal error", |app| {
        app.state().run_state() == Some(RunState::Failed)
    });
    h.get_by_label("Failed");
    h.get_by_label("The run stopped on an error during update 3.");
    h.get_by_label("Exit status 1");
    assert!(h
        .state()
        .state()
        .fatal
        .as_deref()
        .is_some_and(|f| f.contains("missing.org")));
    assert_eq!(h.state().state().frame.as_ref().map(|f| f.update), Some(3));

    // Still inspectable: open the log, pick a layer, select a cell.
    h.get_by_label("Show the log").click();
    settle(&mut h);
    assert!(h.state().state().show_log);
    h.key_press(Key::Num3);
    wait_until(&mut h, "the merit layer", |app| {
        app.map_image().is_some_and(|m| m.layer == LayerId::Merit)
    });
    h.get_by_label("Population map").click();
    wait_until(&mut h, "a selection", |app| app.state().selection.is_some());
    h.key_press(Key::Space);
    settle(&mut h);
    assert!(h
        .state()
        .state()
        .toasts
        .iter()
        .any(|t| t.text.contains("error")));
    h.check_accessibility();
    screenshot(&mut h, "fatal-compact-dark");
}

#[test]
fn a_folder_that_is_not_an_experiment_is_reported() {
    let missing = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("gui/no-such-experiment");
    let _ = std::fs::remove_dir_all(&missing);
    let mut h = harness(open_args(&missing), vec2(1000.0, 650.0));
    wait_until(&mut h, "the error", |app| app.state().open_error.is_some());
    assert_eq!(h.state().state().phase, Phase::Start);
    h.get_by_label("This folder does not exist.");
    h.get_by_label("Dismiss").click();
    settle(&mut h);
    assert!(h.state().state().open_error.is_none());
    h.check_accessibility();
    screenshot(&mut h, "start-error-dark");
}

#[test]
fn the_debugger_steps_stops_at_breakpoints_and_follows_offspring() {
    // `SPECULATIVE` is on (the default): a time slice runs up to 33
    // instructions, and the debugger still steps them one at a time and
    // stops on every breakpoint.
    let dir = fresh_config("debugger");
    let mut h = harness(open_args(&dir), vec2(1440.0, 900.0));
    wait_until(&mut h, "the world", ready);
    h.get_by_role_and_label(Role::Tab, "Organism").click();
    settle(&mut h);
    h.get_by_label("No organism selected");
    h.get_by_label("Step update").click();
    wait_until(&mut h, "the ancestor", |app| {
        app.state()
            .frame
            .as_ref()
            .and_then(|f| f.map.as_ref())
            .is_some_and(|m| m.occupied() == 1)
            && app.state().run_state() == Some(RunState::Paused)
    });

    // Watch the ancestor and step it one instruction.
    click_cell(&mut h, 0);
    let before = watched(&mut h, 0);
    // On the heads CPU a step (one CPU cycle) is one instruction.
    assert!(!before.cpu.cycle_runs_micro_ops());
    h.get_by_label(inspector::step_hint(false));
    assert!(h.query_by_label(inspector::MICRO_OPS_NOTE).is_none());
    h.get_by_label("Step").click();
    assert_eq!(stopped(&mut h), UntilOutcome::Reached);
    wait_until(&mut h, "the stepped organism", |app| {
        app.state()
            .debugger
            .current
            .as_ref()
            .is_some_and(|o| o.phenotype.lifetime_cpu_cycles > before.phenotype.lifetime_cpu_cycles)
    });
    let after = h.state().state().debugger.current.clone().unwrap();
    assert_eq!(
        after.phenotype.lifetime_cpu_cycles,
        before.phenotype.lifetime_cpu_cycles + 1,
        "one instruction"
    );
    assert_eq!(
        h.state().state().debugger.moved_heads().first(),
        Some(&true)
    );
    let _ = h.get_all_by_label_contains("Stepped after");
    h.check_accessibility();
    screenshot(&mut h, "debugger-step-dark");

    // A breakpoint set and cleared in the listing (the IP's row is in view).
    let ip = h.state().state().debugger.ip().unwrap();
    h.get_by_label(&format!("Set breakpoint at {ip}")).click();
    settle(&mut h);
    assert!(h.state().state().debugger.breakpoints.contains(&(0, ip)));
    h.get_by_label(&format!("Remove breakpoint at {ip}"))
        .click();
    settle(&mut h);
    assert!(h.state().state().debugger.breakpoints.is_empty());

    // Run to a breakpoint on the copy loop, far down the listing: the
    // listing scrolls to it.
    let copy = (0..after.cpu.memory.len())
        .find(|&p| after.cpu.inst_name(p) == "h-copy")
        .unwrap() as i32;
    h.state_mut()
        .state_mut()
        .debugger
        .toggle_breakpoint(0, copy);
    h.get_by_label("Run to breakpoint").click();
    assert_eq!(stopped(&mut h), UntilOutcome::Reached);
    wait_until(&mut h, "the IP on the breakpoint", |app| {
        app.state().debugger.ip() == Some(copy)
    });
    settle(&mut h);
    h.get_by_label(&format!("Remove breakpoint at {copy}"));
    let _ = h.get_all_by_label_contains("Breakpoint reached");
    screenshot(&mut h, "debugger-breakpoint-dark");
    // `B` goes round the loop once more.
    let cycles = h
        .state()
        .state()
        .debugger
        .current
        .as_ref()
        .unwrap()
        .phenotype
        .lifetime_cpu_cycles;
    h.key_press(Key::B);
    assert_eq!(stopped(&mut h), UntilOutcome::Reached);
    wait_until(&mut h, "the next copy", |app| {
        app.state().debugger.current.as_ref().is_some_and(|o| {
            o.phenotype.lifetime_cpu_cycles > cycles && o.cpu.threads[0].heads[0].pos == copy
        })
    });

    // Stop on the divide, follow offspring, and step over it: the watch
    // moves to the child.
    let parent = h.state().state().debugger.org_id.unwrap();
    let divide = (0..after.cpu.memory.len())
        .find(|&p| after.cpu.inst_name(p) == "h-divide")
        .unwrap() as i32;
    {
        let d = &mut h.state_mut().state_mut().debugger;
        d.breakpoints.clear();
        d.toggle_breakpoint(0, divide);
    }
    h.key_press(Key::B);
    assert_eq!(stopped(&mut h), UntilOutcome::Reached);
    wait_until(&mut h, "the IP on the divide", |app| {
        app.state().debugger.ip() == Some(divide)
    });
    h.get_by_label("Follow offspring").click();
    settle(&mut h);
    assert!(h.state().state().debugger.follow_offspring);
    h.key_press(Key::I);
    wait_until(&mut h, "the offspring", |app| {
        let s = app.state();
        s.selection != Some(0)
            && s.debugger.org_id.is_some_and(|id| id != parent)
            && s.debugger.current.is_some()
    });
    let child = h.state().state().debugger.current.clone().unwrap();
    assert_eq!(h.state().state().selection, Some(child.cell));
    // The birth itself is in the log, from the parent watched.
    assert!(h
        .state()
        .state()
        .log
        .entries()
        .any(|e| e.text.starts_with(&format!(
            "Organism #{parent} gave birth to #{}",
            child.org_id
        ))));
    assert_eq!(h.state().state().debugger.fate, Fate::Alive);
    assert!(h
        .state()
        .state()
        .log
        .entries()
        .any(|e| e.text.starts_with("Following offspring")));
    h.get_by_label("Theme").click();
    settle(&mut h);
    assert_eq!(h.state().settings().theme, ThemeChoice::Light);
    h.check_accessibility();
    screenshot(&mut h, "debugger-offspring-light");
}

#[test]
fn the_organism_tab_lists_the_memory_space_of_a_parasite_thread() {
    let dir = fresh_config("memory-spaces");
    let mut h = harness(open_args(&dir), vec2(1440.0, 900.0));
    wait_until(&mut h, "the world", ready);
    h.get_by_role_and_label(Role::Tab, "Organism").click();
    settle(&mut h);
    h.get_by_label("Step update").click();
    wait_until(&mut h, "the ancestor", |app| {
        app.state()
            .frame
            .as_ref()
            .and_then(|f| f.map.as_ref())
            .is_some_and(|m| m.occupied() == 1)
            && app.state().run_state() == Some(RunState::Paused)
    });
    click_cell(&mut h, 0);
    let mut org = watched(&mut h, 0);

    // Dress the ancestor up as a TransSMT host with an offspring space and
    // a parasite whose thread runs in space 2.
    let genome = org.cpu.memory.clone();
    let space = |id: i32, label: &str, n: usize| avida_viewmodel::MemSpaceView {
        id,
        label: label.into(),
        memory: genome.iter().copied().cycle().take(n).collect(),
    };
    org.cpu.spaces.push(space(1, "offspring", 20));
    org.cpu.spaces.push(space(2, "parasite", 40));
    let mut parasite = org.cpu.threads[0].clone();
    parasite.id = 1;
    for (i, head) in parasite.heads.iter_mut().enumerate() {
        head.mem_space = if i == 2 { 1 } else { 2 };
        head.pos = [7, 3, 4, 0][i.min(3)];
    }
    org.cpu.threads.push(parasite);
    {
        let d = &mut h.state_mut().state_mut().debugger;
        d.current = Some(org);
        d.previous = None;
    }
    settle(&mut h);

    // The host thread lists the genome; the spaces are tabs.
    h.get_by_role_and_label(Role::Tab, "Memory space 0 genome");
    h.get_by_role_and_label(Role::Tab, "Memory space 2 parasite");
    assert_eq!(h.state().state().debugger.listed_space(), 0);
    let _ = h.get_all_by_label_contains("Memory space 0 (genome)");

    // The parasite thread follows its IP into its own space, and says where
    // the write head is.
    h.get_by_label("Thread 1").click();
    settle(&mut h);
    assert_eq!(h.state().state().debugger.listed_space(), 2);
    let _ = h.get_all_by_label_contains("Memory space 2 (parasite)");
    let _ = h.get_all_by_label_contains("Write in space 1 at 4");
    h.get_by_label("Set breakpoint at 7 in space 2").click();
    settle(&mut h);
    assert!(h.state().state().debugger.breakpoints.contains(&(2, 7)));
    screenshot(&mut h, "debugger-parasite-space-dark");

    // A tab pins another space; the IP's space comes back on request.
    h.get_by_role_and_label(Role::Tab, "Memory space 1 offspring")
        .click();
    settle(&mut h);
    assert_eq!(h.state().state().debugger.listed_space(), 1);
    let _ = h.get_all_by_label_contains("Memory space 1 (offspring)");
    h.get_by_label("Show the IP's space").click();
    settle(&mut h);
    assert_eq!(h.state().state().debugger.listed_space(), 2);
    h.check_accessibility();
}

/// The Organism tab watching the BCR organism of two genes (in cell 12),
/// in a window of `size`.
fn watch_bcr_organism(name: &str, size: egui::Vec2) -> (H, avida_viewmodel::OrganismView) {
    let dir = gene_config(5, name);
    let mut h = harness(open_args(&dir), size);
    wait_until(&mut h, "the world", ready);
    h.get_by_role_and_label(Role::Tab, "Organism").click();
    settle(&mut h);
    h.get_by_label("Step update").click();
    wait_until(&mut h, "the organism", |app| {
        app.state()
            .frame
            .as_ref()
            .and_then(|f| f.map.as_ref())
            .is_some_and(|m| m.occupied() == 1)
            && app.state().run_state() == Some(RunState::Paused)
    });
    click_cell(&mut h, 12);
    let org = watched(&mut h, 12);
    (h, org)
}

/// A live BCR organism of two genes: threads, heads, registers and gene
/// memories show up with the widgets the heads CPU uses.
#[test]
fn the_organism_tab_shows_the_threads_heads_and_gene_memories_of_a_bcr_organism() {
    let (mut h, org) = watch_bcr_organism("bcr-organism", vec2(1440.0, 900.0));
    assert_eq!(org.cpu.hw_type, 5);
    assert_eq!(org.cpu.threads.len(), 2);
    // A step is one CPU cycle of several micro-operations on this hardware.
    assert!(org.cpu.cycle_runs_micro_ops());
    h.get_by_label(inspector::step_hint(true));
    h.get_by_label(inspector::MICRO_OPS_NOTE);
    assert!(h
        .query_by_label_contains("Step runs one instruction")
        .is_none());

    // Both threads are tabs; the thread's gene is the listed space.
    h.get_by_label("Thread 0 ●");
    h.get_by_label("Thread 1");
    h.get_by_role_and_label(Role::Tab, "Memory space 0 genome");
    h.get_by_role_and_label(Role::Tab, "Memory space 16 gene 0, label AB");
    h.get_by_role_and_label(Role::Tab, "Memory space 17 gene 1, label BB");
    assert_eq!(h.state().state().debugger.listed_space(), 16);
    let _ = h.get_all_by_label_contains("Memory space 16 (gene 0, label AB)");
    // Twelve registers and twelve heads with the C++ names.
    h.get_by_label("LX 0");
    let _ = h.get_all_by_label_contains("Flow9");
    let _ = h.get_all_by_label_contains("Thread 0: label AB");
    screenshot(&mut h, "debugger-bcr-gene-dark");

    // The second thread waits in its own gene.
    h.get_by_label("Thread 1").click();
    settle(&mut h);
    assert_eq!(h.state().state().debugger.listed_space(), 17);
    let _ = h.get_all_by_label_contains("waiting for LX > 2 into BX");
    h.check_accessibility();
}

/// At the smallest window the heading of a gene's memory space is too long
/// to share its line with the flag legend: the legend goes on a line of its
/// own below, and the two never overlap.
#[test]
fn the_memory_heading_and_the_flag_legend_do_not_overlap_in_the_smallest_window() {
    let (mut h, _) = watch_bcr_organism("bcr-organism-narrow", vec2(1000.0, 650.0));
    assert_eq!(h.state().state().debugger.listed_space(), 16);
    let heading = h
        .query_all(
            by().label_contains("Memory space 16 (gene 0, label AB) · ")
                .role(Role::Label),
        )
        .map(|n| n.rect())
        .next()
        .expect("the memory heading");
    let legend = h.get_by_label_contains("© copied").rect();
    assert!(heading.width() > 0.0 && legend.width() > 0.0);
    assert!(
        !heading.intersects(legend),
        "the heading {heading:?} overlaps the legend {legend:?}"
    );
    // Below it, not squeezed beside it.
    assert!(legend.top() >= heading.bottom(), "{heading:?} {legend:?}");
    screenshot(&mut h, "debugger-bcr-gene-1000-dark");
}

#[test]
fn the_command_palette_finds_and_runs_actions() {
    let dir = fresh_config("palette");
    let mut h = harness(open_args(&dir), vec2(1280.0, 800.0));
    wait_until(&mut h, "the world", ready);

    h.key_press_modifiers(Modifiers::COMMAND, Key::K);
    settle(&mut h);
    assert!(h.state().state().command_palette.open);
    h.get_by_label("Command").type_text("fitn");
    settle(&mut h);
    h.get_by_role_and_label(Role::Button, "Layer: Fitness");
    h.check_accessibility();
    screenshot(&mut h, "palette-dark");
    h.key_press(Key::Enter);
    wait_until(&mut h, "the fitness layer", |app| {
        app.map_image().is_some_and(|m| m.layer == LayerId::Fitness)
    });
    assert!(!h.state().state().command_palette.open);

    // From the top bar; keywords find the theme; a click runs it.
    h.get_by_label("Command palette").click();
    settle(&mut h);
    h.get_by_label("Command").type_text("appearance");
    settle(&mut h);
    h.get_by_role_and_label(Role::Button, "Light theme").click();
    settle(&mut h);
    assert_eq!(h.state().settings().theme, ThemeChoice::Light);
    assert!(!h.state().state().command_palette.open);

    // Arrows move the selection; Esc closes without doing anything.
    h.key_press_modifiers(Modifiers::COMMAND, Key::K);
    settle(&mut h);
    h.get_by_label("Command").type_text("step");
    settle(&mut h);
    h.key_press(Key::ArrowDown);
    settle(&mut h);
    assert_eq!(h.state().state().command_palette.selected, 1);
    screenshot(&mut h, "palette-light");
    h.key_press(Key::Escape);
    settle(&mut h);
    assert!(!h.state().state().command_palette.open);
    assert_eq!(h.state().state().run_state(), Some(RunState::Paused));
}

/// Replaces the text of the field named `label`.
fn retype(h: &mut H, label: &str, text: &str) {
    h.get_by_role_and_label(Role::TextInput, label).click();
    h.step();
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.step();
    h.get_by_role_and_label(Role::TextInput, label)
        .type_text(text);
    h.step();
}

#[test]
fn the_config_tab_lists_settings_and_restarts_with_overrides() {
    let dir = fresh_config("config");
    let mut h = harness(
        open_with(&dir, &["-set", "WORLD_Y", "40"]),
        vec2(1440.0, 900.0),
    );
    wait_until(&mut h, "the world", ready);
    h.get_by_role_and_label(Role::Tab, "Config").click();
    settle(&mut h);

    // Search the settings.
    h.get_by_label("Search settings").click();
    h.step();
    h.get_by_label("Search settings").type_text("world");
    settle(&mut h);
    h.get_by_label("WORLD_X = 60");
    h.get_by_label("WORLD_Y = 40");
    h.check_accessibility();
    screenshot(&mut h, "config-dark");

    // The events.
    h.get_by_label_contains("Events ·").click();
    settle(&mut h);
    h.get_by_label_contains("Inject default-classic.org");
    screenshot(&mut h, "config-events-dark");
    h.get_by_label_contains("Settings ·").click();
    settle(&mut h);

    // Restart with a seed, a width and an update limit.
    retype(&mut h, "Random seed", "7");
    retype(&mut h, "World width", "30");
    retype(&mut h, "Update limit", "5");
    settle(&mut h);
    h.get_by_label_contains("avida-gui");
    h.get_by_label("Restart with changes").click();
    wait_until(&mut h, "the restarted world", |app| {
        ready(app)
            && app
                .state()
                .frame
                .as_ref()
                .is_some_and(|f| f.header.world_x == 30)
    });
    let s = h.state().state();
    let exp = s.experiment.clone().unwrap();
    assert_eq!(
        exp.args,
        ["-set", "WORLD_Y", "40", "-s", "7", "-set", "WORLD_X", "30"]
            .map(String::from)
            .to_vec()
    );
    assert_eq!(exp.update_limit, Some(5));
    let header = s.frame.as_ref().unwrap().header.clone();
    assert_eq!(
        (header.world_x, header.world_y, header.random_seed),
        (30, 40, 7)
    );
    let setup = s.setup.clone().unwrap();
    assert!(setup.setting("WORLD_X").unwrap().from_command_line);
    assert!(!s.config_view.is_edited(), "the form shows the new world");

    // Run stops at the limit.
    h.get_by_role_and_label(Role::Button, "Run").click();
    wait_until(&mut h, "the update limit", |app| {
        let s = app.state();
        s.run_state() == Some(RunState::Paused) && s.history.latest().is_some_and(|r| r.update == 5)
    });
    h.get_by_label("Theme").click();
    settle(&mut h);
    h.check_accessibility();
    screenshot(&mut h, "config-restarted-light");
}

#[test]
fn a_dropped_folder_opens_and_the_title_follows_the_run() {
    let dir = fresh_config("dropped");
    let mut h = harness(StartupArgs::default(), vec2(1000.0, 650.0));
    h.step();
    assert_eq!(h.state().title(), "Avida");

    // Hovering shows the drop target; dropping a file opens its folder.
    h.input_mut().hovered_files.push(egui::HoveredFile {
        path: Some(dir.join("avida.cfg")),
        mime: String::new(),
    });
    screenshot(&mut h, "drop-target-dark");
    h.input_mut().hovered_files.clear();
    h.input_mut()
        .dropped_files
        .push(Arc::new(Dropped(dir.join("avida.cfg"))));
    wait_until(&mut h, "the world", ready);
    assert_eq!(
        h.state().state().experiment.as_ref().unwrap().dir,
        std::path::absolute(&dir).unwrap()
    );
    let title = h.state().title();
    assert!(title.starts_with("dropped / config · paused"), "{title}");
    // At full speed a debug build runs the 100 updates between two frames.
    h.state_mut().state_mut().set_speed(SpeedSetting {
        limited: true,
        ups: 5.0,
    });
    h.key_press(Key::Space);
    wait_until(&mut h, "a running title", |app| {
        app.title().contains("running")
    });
    h.key_press(Key::Space);
    wait_until(&mut h, "a paused title", |app| {
        app.title().contains("paused at update")
    });
}

/// Small helpers on the harness.
trait HarnessExt {
    fn map_image_palette(&self) -> Option<Palette>;
}

impl HarnessExt for egui_kittest::Harness<'static, avida_gui::AvidaApp> {
    fn map_image_palette(&self) -> Option<Palette> {
        self.state().map_image().map(|m| m.palette)
    }
}
