//! Port of the organism phenotype layer (`cMerit`, `cMutationRates`, `cPhenotype`).

// `println!` prints through `avida_apto::diag`, which a front end can
// capture (the std macro's output when nothing captures).
macro_rules! println {
    ($($arg:tt)*) => {
        ::avida_apto::diag::stdout(format_args!("{}\n", format_args!($($arg)*)))
    };
}

pub mod merit;
pub mod mutation_rates;
pub mod phenotype;

pub use merit::Merit;
pub use mutation_rates::MutationRates;
pub use phenotype::{Phenotype, PhenotypeDeme, PhenotypeSizes, PhenotypeStats};
