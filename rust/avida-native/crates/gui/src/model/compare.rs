//! The comparison screen's model: the state shared with the terminal
//! interface (`avida_viewmodel::compare::CompareState`) and the colours of
//! the arms.

pub use avida_viewmodel::compare::CompareState;

/// Colours of the arms in order: the Okabe-Ito set without black, ordered
/// so that the first two (orange and sky blue) stay apart for every kind of
/// colour vision. An arm keeps its colour in every panel.
pub const ARM_COLORS: [[u8; 3]; 7] = [
    [0xE6, 0x9F, 0x00],
    [0x56, 0xB4, 0xE9],
    [0x00, 0x9E, 0x73],
    [0xD5, 0x5E, 0x00],
    [0xCC, 0x79, 0xA7],
    [0x00, 0x72, 0xB2],
    [0xF0, 0xE4, 0x42],
];

/// The colour of arm `i`; past the seventh a neutral grey.
pub fn arm_rgb(i: usize, dark: bool) -> [u8; 3] {
    let Some(c) = ARM_COLORS.get(i) else {
        return [0x99, 0x99, 0x99];
    };
    if dark {
        *c
    } else {
        // Light backgrounds need darker marks to keep 3:1 against white.
        [
            (f32::from(c[0]) * 0.8) as u8,
            (f32::from(c[1]) * 0.8) as u8,
            (f32::from(c[2]) * 0.8) as u8,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arm_colours_are_distinct_and_fixed() {
        for dark in [true, false] {
            let all: Vec<[u8; 3]> = (0..8).map(|i| arm_rgb(i, dark)).collect();
            for i in 0..all.len() {
                for j in i + 1..all.len() {
                    if i < 7 {
                        assert_ne!(all[i], all[j], "{i} {j} {dark}");
                    }
                }
            }
        }
        assert_eq!(arm_rgb(0, true), [0xE6, 0x9F, 0x00]);
        assert_eq!(arm_rgb(99, true), [0x99; 3]);
    }
}
