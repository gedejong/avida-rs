//! The open-endedness view: the rows that `PrintMODESData` and
//! `PrintPhylogenyMetrics` have written, turned into small charts that the
//! desktop and the terminal front end draw the same way.
//!
//! The world keeps the last [`avida_world::modes::HISTORY_CAP`] rows of each
//! action instance in memory (observation only: nothing is added to a data
//! file and no random number is drawn); [`capture`](crate::capture) copies
//! them into [`Frame::open_ended`](crate::Frame::open_ended); [`build`] makes
//! the series. A recording does not carry these rows, so a playback shows
//! [`PLAYBACK_NOTE`] instead.
//!
//! The definitions behind the explanations are in the documentation of
//! `avida_world::modes` and `avida_systematics::phylometrics`.

// The row types are the world's; front ends reach them through here.
pub use avida_world::modes::{FilterMode, ModesHistory, ModesRow};
pub use avida_world::phylo_actions::PhyloRow;

/// The rows of every open-endedness action instance of a world.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OpenEnded {
    /// One entry per `PrintMODESData` instance.
    pub modes: Vec<ModesHistory>,
    /// One entry per `PrintPhylogenyMetrics` file: its name and its rows.
    pub phylogeny: Vec<PhyloHistory>,
}

/// The rows one `PrintPhylogenyMetrics` file has received.
#[derive(Clone, Debug, PartialEq)]
pub struct PhyloHistory {
    /// The file under the data directory.
    pub filename: String,
    /// Rows, oldest first.
    pub rows: Vec<PhyloRow>,
}

impl OpenEnded {
    /// True when no instance has fired yet.
    pub fn is_empty(&self) -> bool {
        self.modes.is_empty() && self.phylogeny.is_empty()
    }
}

/// What is shown instead of the charts while a recording is played back.
pub const PLAYBACK_NOTE: &str =
    "A recording does not contain the open-endedness measures, so they are only available in a live run.";

/// The event line that adds the MODES measures.
pub const MODES_EXAMPLE: &str = "u 0:1:end PrintMODESData modes.dat 100 500";
/// The event line that adds the phylogeny measures.
pub const PHYLOGENY_EXAMPLE: &str = "u 0:200:end PrintPhylogenyMetrics phylogeny.dat";

/// Which of the two actions the run's events file asks for, read from its
/// event lines.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OeSources {
    /// The `(resolution, t)` arguments of each `PrintMODESData` line that
    /// parses (`None` for one that does not); empty when there is no line.
    pub modes: Vec<Option<(i64, i64)>>,
    /// Whether a `PrintPhylogenyMetrics` line exists.
    pub phylogeny: bool,
    /// Whether a `PrintMODESData` line exists.
    pub has_modes: bool,
}

impl OeSources {
    /// From `(action, arguments)` of every event line.
    pub fn from_lines<'a>(lines: impl IntoIterator<Item = (&'a str, &'a str)>) -> OeSources {
        let mut s = OeSources::default();
        for (action, args) in lines {
            if action.eq_ignore_ascii_case("PrintMODESData") {
                s.has_modes = true;
                let w: Vec<&str> = args.split_whitespace().collect();
                let parsed = match (
                    w.get(1).and_then(|x| x.parse::<i64>().ok()),
                    w.get(2).and_then(|x| x.parse::<i64>().ok()),
                ) {
                    (Some(r), Some(t)) if r >= 1 && t >= r => Some((r, t)),
                    _ => None,
                };
                s.modes.push(parsed);
            } else if action.eq_ignore_ascii_case("PrintPhylogenyMetrics") {
                s.phylogeny = true;
            }
        }
        s
    }
}

/// One small chart: a single measure against the x axis its group shares.
#[derive(Clone, Debug, PartialEq)]
pub struct OeChart {
    /// Stable key of the measure (`change`, `living`, ...).
    pub id: &'static str,
    /// Title, with the unit when there is one.
    pub title: String,
    /// What the x axis is called.
    pub x_label: &'static str,
    /// Values are whole numbers (counts).
    pub whole: bool,
    /// One plain sentence saying what the measure means.
    pub explanation: &'static str,
    /// `(x, y)` points, oldest first; rows whose value is not a number are
    /// left out.
    pub points: Vec<(f64, f64)>,
}

impl OeChart {
    /// The newest point.
    pub fn latest(&self) -> Option<(f64, f64)> {
        self.points.last().copied()
    }

    /// The point whose x is nearest to `x`.
    pub fn nearest(&self, x: f64) -> Option<(f64, f64)> {
        self.points
            .iter()
            .copied()
            .min_by(|a, b| (a.0 - x).abs().total_cmp(&(b.0 - x).abs()))
    }
}

/// The charts of one action instance, sharing an x axis.
#[derive(Clone, Debug, PartialEq)]
pub struct OeGroup {
    /// Heading: which instance this is.
    pub title: String,
    /// What the instance's settings mean, in sentences.
    pub caption: String,
    /// Set while the group has no chart: why not yet.
    pub note: Option<String>,
    /// Empty while [`OeGroup::note`] is set.
    pub charts: Vec<OeChart>,
}

/// Everything a front end draws for the open-endedness view.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OeView {
    /// A sentence about the measures as a whole (where they come from).
    pub intro: Option<String>,
    /// Set when the events file lacks an action: what to do about it.
    pub notice: Option<String>,
    /// The event lines to add, exactly as they are written.
    pub example_lines: Vec<String>,
    /// One group per MODES instance and per phylogeny file.
    pub groups: Vec<OeGroup>,
}

impl OeView {
    /// True when there is nothing to chart because no action is in the
    /// events file.
    pub fn is_unconfigured(&self) -> bool {
        self.groups.is_empty()
    }
}

const MODES_INTRO: &str = "MODES (Dolson, Vostinar, Wiser and Ofria, Artificial Life 25(1), 2019) measures open-ended evolution. A genotype counts as persistent when it, or a descendant of it, is still alive the persistence lag t generations after a sample was taken.";

const X_GENERATION: &str = "average generation";
const X_UPDATE: &str = "update";

const CHANGE: &str = "Change is the number of skeletons (the instructions of a genome that matter for its fitness) among the persistent genotypes that were not among those of the previous sample.";
const NOVELTY: &str = "Novelty is the number of skeletons among the persistent genotypes that have never appeared in any earlier sample.";
const COMPLEXITY: &str = "Complexity is the length, in instructions, of the longest skeleton among the persistent genotypes.";
const ECOLOGY: &str = "Ecology is the Shannon entropy, in bits, of how the organisms of the sampled population are spread over the skeletons of the persistent genotypes, so it is zero when one skeleton holds them all.";

const LIVING: &str =
    "Living genotypes is the number of genotypes that have at least one living organism.";
const PD: &str = "Phylogenetic diversity is the number of branches in the genealogy tree from the most recent common ancestor of all living genotypes down to the living genotypes.";
const PAIR: &str = "Mean pairwise distance is the average number of branches on the path between two living genotypes in the genealogy tree.";
const ED: &str = "Mean evolutionary distinctiveness is the average, over the living genotypes, of the time in updates back along their ancestry, where a stretch of time that several living genotypes share is divided among them.";
const SHANNON: &str = "Shannon diversity is the entropy, in bits, of the living genotypes weighted by their number of organisms, so it is high when many genotypes are similarly common.";
const DEPTH: &str = "Depth of the common ancestor is how many generations of genotypes lie between the founder and the most recent common ancestor of all living genotypes, which grows as old lineages die out.";
const CHANGES: &str = "Changes of the common ancestor counts how many times the most recent common ancestor of all living genotypes has been a different genotype from the one at the previous row.";

fn chart(
    id: &'static str,
    title: &str,
    x_label: &'static str,
    whole: bool,
    explanation: &'static str,
    points: Vec<(f64, f64)>,
) -> OeChart {
    OeChart {
        id,
        title: title.to_string(),
        x_label,
        whole,
        explanation,
        points,
    }
}

fn series<R>(rows: &[R], x: impl Fn(&R) -> f64, y: impl Fn(&R) -> f64) -> Vec<(f64, f64)> {
    rows.iter()
        .map(|r| (x(r), y(r)))
        .filter(|p| p.0.is_finite() && p.1.is_finite())
        .collect()
}

fn modes_charts(rows: &[ModesRow]) -> Vec<OeChart> {
    let g = |r: &ModesRow| r.generation;
    vec![
        chart(
            "change",
            "Change",
            X_GENERATION,
            true,
            CHANGE,
            series(rows, g, |r| r.change as f64),
        ),
        chart(
            "novelty",
            "Novelty",
            X_GENERATION,
            true,
            NOVELTY,
            series(rows, g, |r| r.novelty as f64),
        ),
        chart(
            "complexity",
            "Complexity (instructions)",
            X_GENERATION,
            true,
            COMPLEXITY,
            series(rows, g, |r| r.complexity as f64),
        ),
        chart(
            "ecology",
            "Ecology (bits)",
            X_GENERATION,
            false,
            ECOLOGY,
            series(rows, g, |r| r.ecology),
        ),
    ]
}

fn phylo_charts(rows: &[PhyloRow]) -> Vec<OeChart> {
    let u = |r: &PhyloRow| f64::from(r.update);
    // A row with no common ancestor (nothing alive, or several roots) has
    // depth -1; it is a gap, not a value.
    let depth: Vec<(f64, f64)> = rows
        .iter()
        .filter(|r| r.mrca_depth >= 0)
        .map(|r| (u(r), f64::from(r.mrca_depth)))
        .collect();
    vec![
        chart(
            "living",
            "Living genotypes",
            X_UPDATE,
            true,
            LIVING,
            series(rows, u, |r| f64::from(r.living)),
        ),
        chart(
            "pd",
            "Phylogenetic diversity (branches)",
            X_UPDATE,
            true,
            PD,
            series(rows, u, |r| f64::from(r.pd)),
        ),
        chart(
            "pairwise",
            "Mean pairwise distance (branches)",
            X_UPDATE,
            false,
            PAIR,
            series(rows, u, |r| r.pair_mean),
        ),
        chart(
            "distinctiveness",
            "Mean evolutionary distinctiveness (updates)",
            X_UPDATE,
            false,
            ED,
            series(rows, u, |r| r.ed_mean),
        ),
        chart(
            "shannon",
            "Shannon diversity (bits)",
            X_UPDATE,
            false,
            SHANNON,
            series(rows, u, |r| r.shannon),
        ),
        chart(
            "depth",
            "Depth of the common ancestor",
            X_UPDATE,
            true,
            DEPTH,
            depth,
        ),
        chart(
            "changes",
            "Changes of the common ancestor",
            X_UPDATE,
            true,
            CHANGES,
            series(rows, u, |r| f64::from(r.mrca_changes)),
        ),
    ]
}

fn mode_sentence(mode: FilterMode) -> &'static str {
    match mode {
        FilterMode::Nearest => {
            "Persistence is judged by the nearest ancestor that was alive when the sample was taken."
        }
        FilterMode::Any => {
            "Persistence is judged for every sampled genotype that has a living descendant."
        }
    }
}

/// Generations before the first row of an instance exists: the resolution
/// plus the persistence lag.
fn first_row_generation(resolution: i64, filter: i64) -> i64 {
    resolution + filter
}

fn waiting_note(resolution: i64, filter: i64) -> String {
    format!(
        "No row has been written yet. Rows start after {} generations of the average generation, which is the resolution of {} plus the persistence lag t of {}.",
        first_row_generation(resolution, filter),
        resolution,
        filter
    )
}

fn modes_group(h: &ModesHistory) -> OeGroup {
    let title = format!(
        "MODES, persistence lag t = {} {}, {} filter",
        h.filter,
        if h.filter == 1 {
            "generation"
        } else {
            "generations"
        },
        h.mode.name()
    );
    let every = if h.resolution == 1 {
        "generation".to_string()
    } else {
        format!("{} generations", h.resolution)
    };
    let caption = format!(
        "A sample is taken every {every} and written to {}. {}",
        h.filename,
        mode_sentence(h.mode)
    );
    if h.rows.is_empty() {
        OeGroup {
            title,
            caption,
            note: Some(waiting_note(h.resolution, h.filter)),
            charts: Vec::new(),
        }
    } else {
        OeGroup {
            title,
            caption,
            note: None,
            charts: modes_charts(&h.rows),
        }
    }
}

/// The view for the rows in `data` and the actions the events file asks
/// for in `sources`.
pub fn build(data: &OpenEnded, sources: &OeSources) -> OeView {
    let mut view = OeView::default();
    let modes_on = sources.has_modes || !data.modes.is_empty();
    let phylo_on = sources.phylogeny || !data.phylogeny.is_empty();

    if modes_on {
        view.intro = Some(MODES_INTRO.to_string());
        if data.modes.is_empty() {
            // Configured, but the action has not fired yet.
            let note = match sources.modes.iter().flatten().next() {
                Some(&(r, t)) => waiting_note(r, t),
                None => "No row has been written yet. Rows start after the resolution plus the persistence lag t generations of the average generation have passed.".to_string(),
            };
            view.groups.push(OeGroup {
                title: "MODES".to_string(),
                caption: String::new(),
                note: Some(note),
                charts: Vec::new(),
            });
        }
        view.groups.extend(data.modes.iter().map(modes_group));
    }
    if phylo_on {
        if data.phylogeny.is_empty() {
            view.groups.push(OeGroup {
                title: "Genealogy of the genotypes".to_string(),
                caption: String::new(),
                note: Some(
                    "No row has been written yet. The first one appears the first time the event fires."
                        .to_string(),
                ),
                charts: Vec::new(),
            });
        }
        for p in &data.phylogeny {
            let caption = format!(
                "Measures of the genealogy tree of the genotypes, from Dolson, Lalejini, Jorgensen and Ofria, Artificial Life 26(1), 2020. One row is written to {} each time the event fires.",
                p.filename
            );
            if p.rows.is_empty() {
                view.groups.push(OeGroup {
                    title: "Genealogy of the genotypes".to_string(),
                    caption,
                    note: Some("No row has been written yet.".to_string()),
                    charts: Vec::new(),
                });
            } else {
                view.groups.push(OeGroup {
                    title: "Genealogy of the genotypes".to_string(),
                    caption,
                    note: None,
                    charts: phylo_charts(&p.rows),
                });
            }
        }
    }

    match (modes_on, phylo_on) {
        (false, false) => {
            view.notice = Some(
                "The events file of this run has neither PrintMODESData nor PrintPhylogenyMetrics, so there is nothing to chart yet. Add these lines to the events file and start the run again."
                    .to_string(),
            );
            view.example_lines = vec![MODES_EXAMPLE.to_string(), PHYLOGENY_EXAMPLE.to_string()];
        }
        (true, false) => {
            view.notice = Some(
                "The events file has no PrintPhylogenyMetrics line, so the genealogy is not charted. Add this line to the events file and start the run again to chart it."
                    .to_string(),
            );
            view.example_lines = vec![PHYLOGENY_EXAMPLE.to_string()];
        }
        (false, true) => {
            view.notice = Some(
                "The events file has no PrintMODESData line, so the MODES measures are not charted. Add this line to the events file and start the run again to chart them."
                    .to_string(),
            );
            view.example_lines = vec![MODES_EXAMPLE.to_string()];
        }
        (true, true) => {}
    }
    view
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(u: i32, change: i64) -> ModesRow {
        ModesRow {
            update: u,
            generation: f64::from(u) / 10.0,
            snapshot_update: u - 50,
            change,
            novelty: 1,
            complexity: 40,
            ecology: 0.5,
            persistent: 3,
            skeletons: 2,
            living: 9,
            cumulative_novelty: 4,
        }
    }

    fn modes(rows: Vec<ModesRow>) -> ModesHistory {
        ModesHistory {
            filename: "modes.dat".into(),
            resolution: 100,
            filter: 500,
            mode: FilterMode::Nearest,
            rows,
        }
    }

    fn prow(u: i32, depth: i32, pair: f64) -> PhyloRow {
        PhyloRow {
            update: u,
            living: 7,
            pd: 12,
            pair_mean: pair,
            ed_mean: 3.5,
            shannon: 1.25,
            mrca_depth: depth,
            mrca_changes: 2,
        }
    }

    #[test]
    fn no_action_in_the_events_file_says_so_and_shows_both_lines() {
        let v = build(&OpenEnded::default(), &OeSources::default());
        assert!(v.is_unconfigured());
        assert!(v.groups.is_empty());
        let n = v.notice.unwrap();
        assert!(n.contains("neither PrintMODESData nor PrintPhylogenyMetrics"));
        assert!(n.ends_with('.'));
        assert_eq!(
            v.example_lines,
            vec![
                "u 0:1:end PrintMODESData modes.dat 100 500",
                "u 0:200:end PrintPhylogenyMetrics phylogeny.dat"
            ]
        );
    }

    #[test]
    fn sources_are_read_from_event_lines() {
        let s = OeSources::from_lines([
            ("PrintAverageData", ""),
            ("printmodesdata", "m.dat 50 200 any"),
            ("PrintMODESData", "m.dat oops"),
            ("PrintPhylogenyMetrics", "p.dat"),
        ]);
        assert!(s.has_modes && s.phylogeny);
        assert_eq!(s.modes, vec![Some((50, 200)), None]);
        assert_eq!(OeSources::from_lines([("Exit", "")]), OeSources::default());
    }

    #[test]
    fn modes_without_a_row_says_when_rows_start() {
        let s = OeSources::from_lines([("PrintMODESData", "modes.dat 100 500")]);
        // Before the action has fired.
        let v = build(&OpenEnded::default(), &s);
        assert_eq!(v.groups.len(), 1);
        let note = v.groups[0].note.clone().unwrap();
        assert!(note.contains("after 600 generations"), "{note}");
        assert!(v.groups[0].charts.is_empty());
        // The instance exists but has no row.
        let data = OpenEnded {
            modes: vec![modes(Vec::new())],
            phylogeny: Vec::new(),
        };
        let v = build(&data, &s);
        assert_eq!(v.groups.len(), 1);
        assert!(v.groups[0].note.as_ref().unwrap().contains("after 600"));
        // The phylogeny line is missing: the notice offers it.
        assert_eq!(v.example_lines, vec![PHYLOGENY_EXAMPLE]);
        assert!(v.notice.unwrap().contains("no PrintPhylogenyMetrics"));
    }

    #[test]
    fn a_modes_instance_gets_four_charts_labelled_by_lag_and_filter() {
        let data = OpenEnded {
            modes: vec![modes(vec![row(1000, 3), row(2000, 5)])],
            phylogeny: Vec::new(),
        };
        let s = OeSources::from_lines([("PrintMODESData", "modes.dat 100 500")]);
        let v = build(&data, &s);
        let g = &v.groups[0];
        assert_eq!(
            g.title,
            "MODES, persistence lag t = 500 generations, nearest filter"
        );
        let ids: Vec<_> = g.charts.iter().map(|c| c.id).collect();
        assert_eq!(ids, ["change", "novelty", "complexity", "ecology"]);
        assert_eq!(g.charts[0].points, vec![(100.0, 3.0), (200.0, 5.0)]);
        assert_eq!(g.charts[0].latest(), Some((200.0, 5.0)));
        assert_eq!(g.charts[0].nearest(120.0), Some((100.0, 3.0)));
        assert!(g.charts.iter().all(|c| c.x_label == "average generation"));
        assert!(g.charts.iter().all(|c| c.explanation.ends_with('.')));
        assert!(v.intro.unwrap().contains("Artificial Life 25(1), 2019"));
    }

    #[test]
    fn phylogeny_charts_skip_nan_and_missing_ancestors() {
        let data = OpenEnded {
            modes: Vec::new(),
            phylogeny: vec![PhyloHistory {
                filename: "phylogeny.dat".into(),
                rows: vec![prow(0, -1, f64::NAN), prow(200, 4, 2.5), prow(400, 5, 3.0)],
            }],
        };
        let s = OeSources::from_lines([("PrintPhylogenyMetrics", "")]);
        let v = build(&data, &s);
        let g = &v.groups[0];
        let by = |id: &str| g.charts.iter().find(|c| c.id == id).unwrap();
        assert_eq!(g.charts.len(), 7);
        assert_eq!(by("pairwise").points, vec![(200.0, 2.5), (400.0, 3.0)]);
        assert_eq!(by("depth").points, vec![(200.0, 4.0), (400.0, 5.0)]);
        assert_eq!(by("living").points.len(), 3);
        assert!(g.charts.iter().all(|c| c.x_label == "update"));
        // MODES is missing: the notice offers its line.
        assert_eq!(v.example_lines, vec![MODES_EXAMPLE]);
    }

    #[test]
    fn both_actions_with_rows_need_no_notice() {
        let data = OpenEnded {
            modes: vec![modes(vec![row(1000, 1)])],
            phylogeny: vec![PhyloHistory {
                filename: "p.dat".into(),
                rows: vec![prow(10, 1, 1.0)],
            }],
        };
        let v = build(&data, &OeSources::default());
        assert_eq!(v.groups.len(), 2);
        assert!(v.notice.is_none() && v.example_lines.is_empty());
    }

    #[test]
    fn the_playback_note_is_one_sentence() {
        assert!(PLAYBACK_NOTE.ends_with('.') && PLAYBACK_NOTE.matches(". ").count() == 0);
    }
}
