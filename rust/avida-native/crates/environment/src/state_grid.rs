//! Port of `cStateGrid` (avida-core/source/main/cStateGrid.h).

/// `cStateGrid`: a named grid of discrete states that organisms can walk
/// over and sense.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateGrid {
    name: String,
    w: i32,
    h: i32,
    init_x: i32,
    init_y: i32,
    init_facing: i32,
    states: Vec<String>,
    sense_values: Vec<i32>,
    grid: Vec<i32>,
}

impl StateGrid {
    /// `cStateGrid(name, w, h, x, y, f, states, sense_values, grid)`.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: &str,
        w: i32,
        h: i32,
        x: i32,
        y: i32,
        f: i32,
        states: Vec<String>,
        sense_values: Vec<i32>,
        grid: Vec<i32>,
    ) -> Self {
        StateGrid {
            name: name.to_string(),
            w,
            h,
            init_x: x,
            init_y: y,
            init_facing: f,
            states,
            sense_values,
            grid,
        }
    }

    /// `GetName()`.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// `GetWidth()`.
    pub fn width(&self) -> i32 {
        self.w
    }
    /// `GetHeight()`.
    pub fn height(&self) -> i32 {
        self.h
    }
    /// `GetInitialX()`.
    pub fn initial_x(&self) -> i32 {
        self.init_x
    }
    /// `GetInitialY()`.
    pub fn initial_y(&self) -> i32 {
        self.init_y
    }
    /// `GetInitialFacing()`.
    pub fn initial_facing(&self) -> i32 {
        self.init_facing
    }
    /// `GetNumStates()`.
    pub fn num_states(&self) -> i32 {
        self.states.len() as i32
    }
    /// The state names in definition order.
    pub fn states(&self) -> &[String] {
        &self.states
    }
    /// The sense value of each state.
    pub fn sense_values(&self) -> &[i32] {
        &self.sense_values
    }
    /// `GetStateID(state_name)`: -1 when unknown.
    pub fn state_id(&self, state_name: &str) -> i32 {
        self.states
            .iter()
            .position(|s| s == state_name)
            .map(|i| i as i32)
            .unwrap_or(-1)
    }
    /// `GetIDFor(x, y)`.
    pub fn id_for(&self, x: i32, y: i32) -> i32 {
        y.wrapping_mul(self.w).wrapping_add(x)
    }
    /// `GetStateAt(grid_id)`.
    pub fn state_at_id(&self, grid_id: i32) -> i32 {
        self.grid[grid_id as usize]
    }
    /// `GetStateAt(x, y)`.
    pub fn state_at(&self, x: i32, y: i32) -> i32 {
        self.state_at_id(self.id_for(x, y))
    }
    /// `SenseStateAt(x, y)`.
    pub fn sense_state_at(&self, x: i32, y: i32) -> i32 {
        self.sense_values[self.state_at(x, y) as usize]
    }
}
