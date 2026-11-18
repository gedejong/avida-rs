//! The settings that are a choice among named options.
//!
//! The C++ program wrote such a setting as a number (`WORLD_GEOMETRY 2`) and
//! listed the numbers in the description. The experiment file writes the
//! name (`geometry: torus`). This table is the one source of the names: the
//! loader, `-set`, `avida convert`, the front ends, the generated reference
//! page and the comments of the stock file all read it. The simulation
//! still holds the integer, so nothing it computes depends on a name.
//!
//! A setting is listed here only when its comment or the code defines each
//! option and a value outside the list has no meaning of its own. Settings
//! that are a level (`VERBOSITY`), a bit set (`PRECALC_PHENOTYPE`), a
//! multiplier (`THREAD_SLICING_METHOD`) or a number with its own range stay
//! integers. The names are lower case, unique inside a setting, and taken
//! from the C++ description or from the identifiers of the C++ and Rust code.

use std::collections::HashMap;
use std::sync::OnceLock;

/// One option: the number the simulation holds, the name the file writes
/// and the explanation (a clause, in the wording of the C++ description).
pub type ChoiceOption = (i32, &'static str, &'static str);

/// A choice setting.
#[derive(Debug, Clone, Copy)]
pub struct ChoiceDef {
    /// The C++ name of the setting (`WORLD_GEOMETRY`).
    pub old: &'static str,
    /// The question or heading of the description (may be empty).
    pub intro: &'static str,
    /// The options in number order.
    pub options: &'static [ChoiceOption],
}

impl ChoiceDef {
    /// The name of option `number`.
    pub fn name_of(&self, number: i32) -> Option<&'static str> {
        self.options.iter().find(|o| o.0 == number).map(|o| o.1)
    }

    /// The number of option `name`.
    pub fn number_of(&self, name: &str) -> Option<i32> {
        self.options.iter().find(|o| o.1 == name).map(|o| o.0)
    }

    /// The names joined for a message.
    pub fn names(&self) -> String {
        self.options
            .iter()
            .map(|o| o.1)
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// The text `text` stands for: a name, or the number of an option.
    pub fn resolve(&self, text: &str) -> Option<i32> {
        let t = text.trim();
        if let Some(n) = self.number_of(t) {
            return Some(n);
        }
        let n: i32 = t.parse().ok()?;
        self.name_of(n).map(|_| n)
    }

    /// The description: the heading, then one `name: explanation` line per
    /// option.
    pub fn description(&self) -> String {
        let mut o = String::new();
        if !self.intro.is_empty() {
            o.push_str(self.intro);
            o.push('\n');
        }
        let lines: Vec<String> = self
            .options
            .iter()
            .map(|(_, name, desc)| format!("{name}: {desc}"))
            .collect();
        o.push_str(&lines.join("\n"));
        o
    }
}

/// The choice setting with the C++ name `old`.
pub fn choice_of(old: &str) -> Option<&'static ChoiceDef> {
    CHOICES.iter().find(|c| c.old == old)
}

/// The description of the choice `old`, built once.
pub fn description_of(old: &str) -> Option<&'static str> {
    static CACHE: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    CACHE
        .get_or_init(|| {
            CHOICES
                .iter()
                .map(|c| {
                    let text: &'static str = Box::leak(c.description().into_boxed_str());
                    (c.old, text)
                })
                .collect()
        })
        .get(old)
        .copied()
}

/// Every choice setting.
pub static CHOICES: &[ChoiceDef] = &[
    ChoiceDef {
        old: "WORLD_GEOMETRY",
        intro: "",
        options: &[
            (1, "bounded_grid", "Bounded Grid (world.x by world.y)"),
            (2, "torus", "Toroidal Grid (world.x by world.y; wraps at edges)"),
            (3, "clique", "Clique (all population cells are connected)"),
            (4, "hex", "Hexagonal grid"),
            (5, "partial", "Partial"),
            (6, "lattice_3d", "3D Lattice (under development)"),
            (7, "random_connected", "Random connected"),
            (8, "scale_free", "Scale-free (detailed below)"),
        ],
    },
    ChoiceDef {
        old: "SLIP_FILL_MODE",
        intro: "Fill insertions from slip mutations with:",
        options: &[
            (0, "duplication", "Duplication"),
            (1, "nop_x", "nop-X"),
            (2, "random", "Random"),
            (3, "scrambled", "Scrambled"),
            (4, "nop_c", "nop-C"),
        ],
    },
    ChoiceDef {
        old: "SLIP_COPY_MODE",
        intro: "How to handle 'on-copy' slip mutations:",
        options: &[
            (0, "read_head_slip", "actual read head slip"),
            (1, "large_mutation", "instant large mutation (obeys slip mode)"),
        ],
    },
    ChoiceDef {
        old: "TRANS_FILL_MODE",
        intro: "Fill insertions from translocation mutations with:",
        options: &[(0, "duplication", "Duplication"), (1, "scrambled", "Scrambled")],
    },
    ChoiceDef {
        old: "LGT_FILL_MODE",
        intro: "Fill insertions from lateral gene transfer mutations with:",
        options: &[(0, "duplication", "Duplication"), (1, "scrambled", "scrambled")],
    },
    ChoiceDef {
        old: "LGT_SOURCE_REGION",
        intro: "Extract gene for lateral transfer from:",
        options: &[
            (0, "neighborhood", "Local Neighborhood"),
            (1, "population", "Entire Population"),
        ],
    },
    ChoiceDef {
        old: "MUT_RATE_SOURCE",
        intro: "",
        options: &[
            (1, "environment", "Mutation rates determined by environment."),
            (2, "parent", "Mutation rates inherited from parent."),
        ],
    },
    ChoiceDef {
        old: "BIRTH_METHOD",
        intro: "Which organism should be replaced when a birth occurs?",
        options: &[
            (0, "neighborhood_random", "Random organism in neighborhood"),
            (1, "neighborhood_oldest", "Oldest in neighborhood"),
            (2, "neighborhood_merit", "Largest Age/Merit in neighborhood"),
            (3, "neighborhood_empty", "None (use only empty cells in neighborhood)"),
            (4, "population_random", "Random from population (Mass Action)"),
            (5, "population_oldest", "Oldest in entire population"),
            (6, "deme_random", "Random within deme"),
            (7, "parent_facing", "Organism faced by parent"),
            (8, "next_cell", "Next grid cell (id+1)"),
            (9, "population_energy", "Largest energy used in entire population"),
            (10, "neighborhood_energy", "Largest energy used in neighborhood"),
            (11, "dispersal", "Local neighborhood dispersal"),
            (12, "kill_offspring", "Kill offpsring after recording birth stats (for behavioral trials)"),
            (13, "kill_both", "Kill parent and offpsring (for behavioral trials)"),
        ],
    },
    ChoiceDef {
        old: "DEATH_METHOD",
        intro: "When should death by old age occur?",
        options: &[
            (0, "never", "Never"),
            (1, "age_limit", "When executed AGE_LIMIT (+deviation) total instructions"),
            (2, "age_limit_per_site", "When executed genome_length * AGE_LIMIT (+dev) instructions"),
        ],
    },
    ChoiceDef {
        old: "ALLOC_METHOD",
        intro: "When allocating blank tape, how should it be initialized?",
        options: &[
            (0, "default_instruction", "Allocated space is set to default instruction."),
            (1, "dead_genome", "Set to section of dead genome (creates potential for recombination)"),
            (2, "random_instruction", "Allocated space is set to random instruction."),
        ],
    },
    ChoiceDef {
        old: "DIVIDE_METHOD",
        intro: "",
        options: &[
            (0, "offspring", "Divide leaves state of mother untouched."),
            (1, "split", "Divide resets state of mother (effectively creating 2 offspring)"),
            (2, "birth", "Divide resets state of current thread only (use with parasites)"),
        ],
    },
    ChoiceDef {
        old: "GENERATION_INC_METHOD",
        intro: "",
        options: &[
            (0, "offspring", "Only increase generation of offspring on divide."),
            (1, "both", "Increase generation of both parent and offspring (suggested with divide_method split)."),
        ],
    },
    ChoiceDef {
        old: "INJECT_METHOD",
        intro: "What should happen to a parasite when it gives birth?",
        options: &[
            (0, "keep_thread", "Leave the parasite thread state untouched."),
            (1, "reset_thread", "Resets the state of the calling thread (for SMT parasites, this must be reset_thread)"),
        ],
    },
    ChoiceDef {
        old: "VIRULENCE_SOURCE",
        intro: "Where does the virulence come from?",
        options: &[
            (0, "config", "Virulence is set by config"),
            (1, "parent", "inhereted from parent"),
            (2, "host", "host-controlled, i.e. donation"),
        ],
    },
    ChoiceDef {
        old: "MP_SCHEDULING_STYLE",
        intro: "Style of scheduling:",
        options: &[
            (0, "single_world", "non-MP aware (default)"),
            (1, "integrated", "MP aware, integrated across worlds."),
        ],
    },
    ChoiceDef {
        old: "DEMES_COMPETITION_STYLE",
        intro: "How should demes compete?",
        options: &[
            (0, "proportional", "Fitness proportional selection"),
            (1, "tournament", "Tournament selection"),
        ],
    },
    ChoiceDef {
        old: "DEMES_USE_GERMLINE",
        intro: "Should demes use a distinct germline?",
        options: &[
            (0, "none", "No"),
            (1, "traditional", "Traditional germ lines"),
            (2, "genotype_tracking", "Genotype tracking"),
            (3, "organism_flag", "Organism flagging germline"),
        ],
    },
    ChoiceDef {
        old: "DEMES_RESET_RESOURCES",
        intro: "Reset resources in demes on replication?",
        options: &[
            (0, "both", "reset both demes"),
            (1, "target", "reset target deme"),
            (2, "none", "deme resources remain unchanged"),
        ],
    },
    ChoiceDef {
        old: "DEMES_ORGANISM_PLACEMENT",
        intro: "How should organisms be placed during deme replication.",
        options: &[
            (0, "array_middle", "cell-array middle"),
            (1, "deme_center", "deme center"),
            (2, "random", "random placement"),
            (3, "sequential", "sequential"),
        ],
    },
    ChoiceDef {
        old: "DEMES_ORGANISM_FACING",
        intro: "Which direction should organisms face after deme replication.",
        options: &[
            (0, "unchanged", "unchanged"),
            (1, "northwest", "northwest."),
            (2, "random", "random."),
        ],
    },
    ChoiceDef {
        old: "DEMES_REPLICATION_ONLY_RESETS",
        intro: "Kin selection mode.  On replication:",
        options: &[
            (0, "nothing", "Nothing extra"),
            (1, "reset_resources", "reset deme resources"),
            (2, "reset_and_reinject", "reset resources and re-inject organisms"),
        ],
    },
    ChoiceDef {
        old: "DEMES_MIGRATION_METHOD",
        intro: "Which demes can an offspring land in when it migrates?",
        options: &[
            (0, "any", "Any other deme"),
            (1, "eight_neighbors", "Eight neighboring demes"),
            (2, "adjacent", "Two adjacent demes in list"),
            (3, "points", "Proportional based on the number of points"),
            (4, "matrix", "Use the weight matrix specified in demes.migration_file"),
        ],
    },
    ChoiceDef {
        old: "DEMES_PARASITE_MIGRATION_TARGET_SELECTION_METHOD",
        intro: "How should a target cell be chosen in the migrated-to deme?",
        options: &[
            (0, "random_cell", "Select a cell randomly, if it is not occupied infection fails"),
            (1, "random_occupied", "Select an occupied cell randomly"),
        ],
    },
    ChoiceDef {
        old: "DEMES_SEED_METHOD",
        intro: "Deme seeding method.",
        options: &[
            (0, "legacy", "Maintain old consistency"),
            (1, "genotypes", "New method using genotypes"),
        ],
    },
    ChoiceDef {
        old: "SLICING_METHOD",
        intro: "",
        options: &[
            (0, "constant", "CONSTANT: all organisms receive equal number of CPU cycles"),
            (1, "probabilistic", "PROBABILISTIC: CPU cycles distributed randomly, proportional to merit."),
            (2, "integrated", "INTEGRATED: CPU cycles given out deterministicly, proportional to merit"),
            (3, "deme_probabilistic", "DEME_PROBABALISTIC: Demes receive fixed number of CPU cycles, awarded probabalistically to members"),
            (4, "cross_deme_probabilistic", "CROSS_DEME_PROBABALISTIC: Demes receive CPU cycles proportional to living population size, awarded probabalistically to members"),
            (5, "probabilistic_integrated", "PROB_INTEGRATED_MERIT: probabilistic and integrated (defined in the scheduler)"),
        ],
    },
    ChoiceDef {
        old: "BASE_MERIT_METHOD",
        intro: "How should merit be initialized?",
        options: &[
            (0, "constant", "Constant (merit independent of size)"),
            (1, "copied_size", "Merit proportional to copied size"),
            (2, "executed_size", "Merit prop. to executed size"),
            (3, "full_size", "Merit prop. to full size"),
            (4, "least_size", "Merit prop. to min of executed or copied size"),
            (5, "sqrt_least_size", "Merit prop. to sqrt of the minimum size"),
            (6, "bonus_instructions", "Merit prop. to num times time.merit_bonus_inst is in genome."),
            (7, "gestation_time", "Merit prop. to the CPU cycles used"),
        ],
    },
    ChoiceDef {
        old: "FITNESS_METHOD",
        intro: "",
        options: &[
            (0, "default", "merit times bonus over gestation time"),
            (1, "sigmoidal", "sigmoidal in the bonus"),
            (2, "exponential_gestation", "net bonus, discounted by the gestation time (uses fitness_coeff_1)"),
        ],
    },
    ChoiceDef {
        old: "MESSAGE_RECV_BUFFER_BEHAVIOR",
        intro: "Behavior of message receive buffer;",
        options: &[
            (0, "drop_oldest", "drop oldest (default)"),
            (1, "drop_incoming", "drop incoming"),
        ],
    },
    ChoiceDef {
        old: "MULTI_ABSORB_TYPE",
        intro: "What to do if a collect instruction is called on a range of resources.",
        options: &[
            (0, "random", "absorb a random resource in the range"),
            (1, "first", "absorb the first resource in the range"),
            (2, "last", "absorb the last resource in the range"),
            (3, "fraction", "absorb absorb_resource_fraction / (# of resources in range) of each resource in the range"),
        ],
    },
    ChoiceDef {
        old: "APPLY_ENERGY_METHOD",
        intro: "When should rewarded energy be applied to current energy?",
        options: &[
            (0, "on_divide", "on divide"),
            (1, "on_task", "on completion of task"),
            (2, "on_sleep", "on sleep"),
        ],
    },
    ChoiceDef {
        old: "ENERGY_PASSED_ON_DEME_REPLICATION_METHOD",
        intro: "Who get energy passed from a parent deme",
        options: &[
            (0, "organisms", "Energy divided among organisms injected to offspring deme"),
            (1, "cells", "Energy divided among cells in offspring deme"),
        ],
    },
    ChoiceDef {
        old: "ENERGY_SHARING_METHOD",
        intro: "Method for sharing energy.",
        options: &[
            (0, "request", "receiver must actively receive/request"),
            (1, "push", "energy pushed on receiver"),
        ],
    },
    ChoiceDef {
        old: "NO_ACTIVE_PROMOTER_EFFECT",
        intro: "What happens when there are no active promoters?",
        options: &[
            (0, "start_of_genome", "Start execution at the beginning of the genome."),
            (1, "kill", "Kill the organism."),
            (2, "stop", "Stop the organism from executing any further instructions."),
        ],
    },
    ChoiceDef {
        old: "INST_CODE_DEFAULT_TYPE",
        intro: "Default value of instruction binary code value.",
        options: &[
            (0, "zeros", "All zeros"),
            (1, "instruction_number", "Based off the instruction number"),
        ],
    },
    ChoiceDef {
        old: "MOVEMENT_COLLISIONS_LETHAL",
        intro: "Are collisions during movement lethal (not applied to avatars)?",
        options: &[
            (0, "swap", "no, use swap"),
            (1, "lethal", "yes, use collision selection type"),
            (2, "fail", "no, but movement fails"),
        ],
    },
    ChoiceDef {
        old: "MOVEMENT_COLLISIONS_SELECTION_TYPE",
        intro: "",
        options: &[
            (0, "coin_flip", "50% chance"),
            (1, "vitality", "binned vitality based"),
        ],
    },
    ChoiceDef {
        old: "USE_AVATARS",
        intro: "Set orgs to move & navigate in solo avatar worlds.",
        options: &[
            (0, "off", "no avatars"),
            (1, "on", "yes"),
            (2, "interacting", "yes, with org interactions"),
        ],
    },
    ChoiceDef {
        old: "AVATAR_BIRTH",
        intro: "Where does an offspring's avatar start?",
        options: &[
            (0, "parent", "Same as parent"),
            (1, "random", "Random"),
            (2, "faced", "Cell faced by parent avatar"),
            (3, "next_cell", "next grid cell"),
            (4, "center", "Center of the world"),
        ],
    },
    ChoiceDef {
        old: "AVATAR_BIRTH_FACING",
        intro: "",
        options: &[(0, "north", "North"), (1, "random", "Random")],
    },
    ChoiceDef {
        old: "LOOK_DISABLE_TYPE",
        intro: "",
        options: &[
            (0, "predators", "predators"),
            (1, "prey", "prey"),
            (2, "both", "both predators and prey"),
        ],
    },
    ChoiceDef {
        old: "LOOK_DISABLE_COMBO",
        intro: "",
        options: &[
            (0, "none", "none"),
            (1, "no_food", "return 'not found' for any food resource query"),
            (2, "no_predator", "return 'not found' for any looking-for-predator query"),
            (3, "no_prey", "return 'not found' for any looking-for-prey query"),
        ],
    },
    ChoiceDef {
        old: "PHEROMONE_DROP_MODE",
        intro: "Where to drop pheromone",
        options: &[
            (0, "split", "Half amount at src, half at dest"),
            (1, "source", "All at source"),
            (2, "destination", "All at dest"),
        ],
    },
    ChoiceDef {
        old: "AUTO_REPUTATION",
        intro: "Is an organism's reputation automatically computed based on its donations",
        options: &[
            (0, "off", "no"),
            (1, "donations_and_standing", "increment for each donation + standing"),
            (2, "given_minus_received", "+1 for donations given -1 for donations received"),
            (3, "donors_and_non_donors", "1 for donors -1 for recivers who have not donated"),
            (4, "donors", "+1 for donors"),
            (5, "donors_at_task", "+1 for donors during task check"),
        ],
    },
    ChoiceDef {
        old: "DONATION_RESTRICTIONS",
        intro: "",
        options: &[
            (0, "none", "none"),
            (1, "inter_species", "inter-species only"),
            (2, "different_tag", "different tag only"),
        ],
    },
    ChoiceDef {
        old: "USE_FORM_GROUPS",
        intro: "Enable organisms to form groups.",
        options: &[
            (0, "off", "off"),
            (1, "unrestricted", "on, no restrict"),
            (2, "defined_only", "on, restrict to defined"),
        ],
    },
    ChoiceDef {
        old: "TOLERANCE_VARIATIONS",
        intro: "",
        options: &[
            (0, "all", "all tolerance active"),
            (1, "immigration", "only immigration tolerance active"),
            (2, "immigration_and_sex", "immigrants + sex"),
        ],
    },
    ChoiceDef {
        old: "PRED_PREY_SWITCH",
        intro: "",
        options: &[
            (-2, "track_prey", "no predators, but track prey stats"),
            (-1, "no_predators", "no predators in experiment"),
            (0, "pred_to_prey_denied", "don't allow a predator to switch to being a prey (prey to pred always allowed)"),
            (1, "pred_to_prey_allowed", "allow predators to switch to being prey"),
            (2, "no_switch", "don't allow a predator to switch to being a prey & don't allow prey to switch via set-forage-target (via attack allowed)"),
        ],
    },
    ChoiceDef {
        old: "TRACK_GROUP_ATTACK_DETAILS",
        intro: "Track details around execution of EVERY group attack instructions for every update.",
        options: &[
            (0, "off", "no tracking"),
            (1, "string", "as string in one file."),
            (2, "bits", "as bits in new file for every update that this is on!"),
        ],
    },
    ChoiceDef {
        old: "HGT_SOURCE",
        intro: "Source of HGT fragments;",
        options: &[
            (0, "dead_organisms", "dead organisms (default)"),
            (1, "parent", "parent."),
        ],
    },
    ChoiceDef {
        old: "HGT_FRAGMENT_SELECTION",
        intro: "Method used to select fragments for HGT mutation;",
        options: &[
            (0, "random", "random (default)"),
            (1, "trimmed", "trimmed selection"),
            (2, "random_placement", "random placement."),
        ],
    },
    ChoiceDef {
        old: "HGT_FRAGMENT_XFORM",
        intro: "Transformation to apply to each fragment prior to incorporation into offspring's genome;",
        options: &[
            (0, "none", "none (default),"),
            (1, "shuffle", "random shuffle,"),
            (2, "random_instructions", "replace with random instructions."),
        ],
    },
    ChoiceDef {
        old: "TASK_SWITCH_PENALTY_TYPE",
        intro: "Type of task switch cost:",
        options: &[
            (0, "none", "none"),
            (1, "learning", "learning"),
            (2, "retooling", "retooling or context"),
            (3, "centrifuge", "centrifuge"),
        ],
    },
    ChoiceDef {
        old: "ANALYZE_MODE",
        intro: "DEPRECATED: use command line options -a[nalyze] or -i[nteractive])",
        options: &[
            (0, "disabled", "Disabled"),
            (1, "analyze", "Enabled"),
            (2, "interactive", "Interactive"),
        ],
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn names_and_numbers_are_unique_inside_a_setting() {
        let mut olds = HashSet::new();
        for c in CHOICES {
            assert!(olds.insert(c.old), "{} is listed twice", c.old);
            assert!(c.options.len() >= 2, "{} has fewer than two options", c.old);
            let mut names = HashSet::new();
            let mut numbers = HashSet::new();
            for (n, name, desc) in c.options {
                assert!(names.insert(*name), "{}: name {name} twice", c.old);
                assert!(numbers.insert(*n), "{}: number {n} twice", c.old);
                assert!(!desc.is_empty(), "{}: {name} has no description", c.old);
                assert!(
                    name.chars()
                        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_')
                        && !name.starts_with(|ch: char| ch.is_ascii_digit()),
                    "{}: '{name}' is not snake_case",
                    c.old
                );
                assert!(
                    name.parse::<i32>().is_err(),
                    "{}: the name {name} reads as a number",
                    c.old
                );
            }
        }
    }

    #[test]
    fn every_choice_round_trips_name_number_name() {
        for c in CHOICES {
            for (n, name, _) in c.options {
                assert_eq!(c.number_of(name), Some(*n));
                assert_eq!(c.name_of(*n), Some(*name));
                assert_eq!(c.resolve(name), Some(*n));
                assert_eq!(c.resolve(&n.to_string()), Some(*n));
            }
            assert_eq!(c.resolve("no_such_option"), None);
        }
    }
}
