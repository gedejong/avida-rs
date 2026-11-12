#ifndef AVIDA_RUST_RUNNING_STATS_FFI_H
#define AVIDA_RUST_RUNNING_STATS_FFI_H

#ifdef __cplusplus
extern "C" {
#endif

typedef struct AvidaRunningStatsHandle AvidaRunningStatsHandle;

/* ---------- ConfigSnapshot (Rust-owned, populated by C++) ---------- */

typedef struct TaskLibConfig {
    int world_x;
    int world_y;
    int use_avatars;
    int match_already_produced;
} TaskLibConfig;

typedef struct PhenotypeConfig {
    int base_merit_method;
    int base_const_merit;
    double default_bonus;
    int divide_method;
    int generation_inc_method;
    int merit_bonus_effect;
    int merit_default_bonus;
    int fitness_method;
    int fitness_valley;
    int fitness_valley_start;
    int fitness_valley_stop;
    int energy_enabled;
    double energy_cap;
    int apply_energy_method;
    double fix_metabolic_rate;
    int inherit_exe_rate;
    int inherit_merit;
    int inherit_multithread;
    double energy_given_at_birth;
    double resource_given_at_birth;
    double resource_given_on_inject;
    double frac_energy_decay_at_org_birth;
    double frac_parent_energy_given_to_org_at_birth;
    double energy_thresh_low;
    double energy_thresh_high;
    double demes_default_germline_propensity;
    int demes_orgs_start_in_germ;
    int tolerance_variations;
    int tolerance_window;
    int max_tolerance;
    int use_resource_bins;
    int collect_specific_resource;
    int split_on_divide;
    double task_refractory_period;
    int task_switch_penalty_type;
    int learning_count;
    int age_poly_tracking;
} PhenotypeConfig;

typedef struct BirthChamberConfig {
    int birth_method;
    int module_num;
    double recombination_prob;
    int cont_rec_regs;
    int corespond_rec_regs;
    int same_length_sex;
    int two_fold_cost_sex;
    int max_birth_wait_time;
    int allow_mate_selection;
    int mating_types;
    int legacy_grid_local_selection;
    int default_group;
    int num_demes;
} BirthChamberConfig;

typedef struct OrganismConfig {
    int death_method;
    int age_limit;
    int age_deviation;
    int max_unique_task_count;
    int require_single_reaction;
    int required_task;
    int required_reaction;
    double required_bonus;
    int required_resource;
    double required_resource_level;
    int use_form_groups;
    int pred_prey_switch;
    double sterilize_fatal;
    double sterilize_detrimental;
    double sterilize_neutral;
    double sterilize_beneficial;
    double sterilize_taskloss;
    int sterilize_unstable;
    double revert_fatal;
    double revert_detrimental;
    double revert_neutral;
    double revert_beneficial;
    double revert_equals;
    double revert_taskloss;
    int save_received;
    int merit_inc_apply_immediate;
} OrganismConfig;

typedef struct ConfigSnapshot {
    TaskLibConfig task_lib;
    PhenotypeConfig phenotype;
    BirthChamberConfig birth_chamber;
    OrganismConfig organism;
} ConfigSnapshot;

/* ---------- TaskContextSnapshot (Rust-owned, populated by C++) ---------- */

#define TASK_CTX_BUFFER_CAP   8
#define TASK_CTX_INT_ARGS_CAP 4
#define TASK_CTX_DOUBLE_ARGS_CAP 2

typedef struct TaskContextSnapshot {
    /* From cTaskContext */
    int    logic_id;
    int    output_value;
    int    output_buffer[TASK_CTX_BUFFER_CAP];
    int    output_count;
    int    input_buffer[TASK_CTX_BUFFER_CAP];
    int    input_count;

    /* Task entry arguments (from cArgContainer) */
    int    task_arg_int[TASK_CTX_INT_ARGS_CAP];
    double task_arg_double[TASK_CTX_DOUBLE_ARGS_CAP];
    int    has_task_args;

    /* From cOrganism */
    int    cell_id;
    int    av_cell_id;
    int    forage_target;
    double gradient_movement;
    int    has_opinion;
    int    opinion_value;
    int    kaboom_executed;
    int    kaboom_executed2;
    int    event_killed;
    int    prev_seen_cell_id;

    /* Neighbor information */
    int    num_neighbors_with_outputs;
} TaskContextSnapshot;

typedef struct AvidaRunningAverageHandle AvidaRunningAverageHandle;
typedef struct AvidaDoubleSumHandle AvidaDoubleSumHandle;
typedef struct AvidaWeightedIndexHandle AvidaWeightedIndexHandle;
typedef struct AvidaOrderedWeightedIndexHandle AvidaOrderedWeightedIndexHandle;
typedef struct AvidaHistogramHandle AvidaHistogramHandle;
typedef struct AvidaRawBitArrayHandle AvidaRawBitArrayHandle;
typedef struct AvidaTimeSeriesHandle AvidaTimeSeriesHandle;

enum {
  AVD_RC_DISPATCH_NONE = 0,
  AVD_RC_DISPATCH_NONSPATIAL = 1,
  AVD_RC_DISPATCH_SPATIAL = 2,
  AVD_RC_WRAPPER_GLOBAL_ONLY = 0,
  AVD_RC_WRAPPER_RANDOM = 1,
  AVD_RC_WRAPPER_FULL = 2,
  AVD_RC_READ_PATH_GLOBAL = 0,
  AVD_RC_READ_PATH_SPATIAL = 1,
  AVD_RC_SETCELL_GLOBAL_NOOP = 0,
  AVD_RC_SETCELL_SPATIAL_WRITE = 1,
  AVD_RC_SETUP_PATH_GLOBAL = 0,
  AVD_RC_SETUP_PATH_PARTIAL = 1,
  AVD_RC_SETUP_PATH_SPATIAL = 2,
  AVD_RC_GRAD_SETTER_PEAK_X = 0,
  AVD_RC_GRAD_SETTER_PEAK_Y = 1,
  AVD_RC_GRAD_SETTER_HEIGHT = 2,
  AVD_RC_GRAD_SETTER_SPREAD = 3,
  AVD_RC_GRAD_SETTER_PLATEAU = 4,
  AVD_RC_GRAD_SETTER_INITIAL_PLAT = 5,
  AVD_RC_GRAD_SETTER_DECAY = 6,
  AVD_RC_GRAD_SETTER_MAX_X = 7,
  AVD_RC_GRAD_SETTER_MAX_Y = 8,
  AVD_RC_GRAD_SETTER_MIN_X = 9,
  AVD_RC_GRAD_SETTER_MIN_Y = 10,
  AVD_RC_GRAD_SETTER_MOVE_SCALER = 11,
  AVD_RC_GRAD_SETTER_UPDATE_STEP = 12,
  AVD_RC_GRAD_SETTER_IS_HALO = 13,
  AVD_RC_GRAD_SETTER_HALO_INNER_RADIUS = 14,
  AVD_RC_GRAD_SETTER_HALO_WIDTH = 15,
  AVD_RC_GRAD_SETTER_HALO_ANCHOR_X = 16,
  AVD_RC_GRAD_SETTER_HALO_ANCHOR_Y = 17,
  AVD_RC_GRAD_SETTER_MOVE_SPEED = 18,
  AVD_RC_GRAD_SETTER_MOVE_RESISTANCE = 19,
  AVD_RC_GRAD_SETTER_PLATEAU_INFLOW = 20,
  AVD_RC_GRAD_SETTER_PLATEAU_OUTFLOW = 21,
  AVD_RC_GRAD_SETTER_CONE_INFLOW = 22,
  AVD_RC_GRAD_SETTER_CONE_OUTFLOW = 23,
  AVD_RC_GRAD_SETTER_GRADIENT_INFLOW = 24,
  AVD_RC_GRAD_SETTER_PLATEAU_COMMON = 25,
  AVD_RC_GRAD_SETTER_FLOOR = 26,
  AVD_RC_GRAD_SETTER_HABITAT = 27,
  AVD_RC_GRAD_SETTER_MIN_SIZE = 28,
  AVD_RC_GRAD_SETTER_MAX_SIZE = 29,
  AVD_RC_GRAD_SETTER_CONFIG = 30,
  AVD_RC_GRAD_SETTER_COUNT = 31,
  AVD_RC_GRAD_SETTER_RESISTANCE = 32,
  AVD_RC_GRAD_SETTER_DAMAGE = 33,
  AVD_RC_GRAD_SETTER_THRESHOLD = 34,
  AVD_RC_GRAD_SETTER_REFUGE = 35,
  AVD_RC_GRAD_SETTER_DEATH_ODDS = 36,
  AVD_RC_GRAD_SETTER_INVALID = -1,
  AVD_RC_GRAD_SCALAR_SETTER_PLATEAU_INFLOW = 0,
  AVD_RC_GRAD_SCALAR_SETTER_PLATEAU_OUTFLOW = 1,
  AVD_RC_GRAD_SCALAR_SETTER_CONE_INFLOW = 2,
  AVD_RC_GRAD_SCALAR_SETTER_CONE_OUTFLOW = 3,
  AVD_RC_GRAD_SCALAR_SETTER_GRADIENT_INFLOW = 4,
  AVD_RC_GRAD_SCALAR_SETTER_INVALID = -1,
  AVD_RC_GRAD_VAR_INFLOW_SETTER_PLAT_VAR_INFLOW = 0,
  AVD_RC_GRAD_VAR_INFLOW_SETTER_INVALID = -1,
  AVD_RC_PREDATORY_SETTER_SET_PREDATORY_RESOURCE = 0,
  AVD_RC_PREDATORY_SETTER_INVALID = -1,
  AVD_RC_PROBABILISTIC_SETTER_SET_PROBABILISTIC_RESOURCE = 0,
  AVD_RC_PROBABILISTIC_SETTER_INVALID = -1,
  AVD_RC_PEAK_GETTER_CURR_X = 0,
  AVD_RC_PEAK_GETTER_CURR_Y = 1,
  AVD_RC_PEAK_GETTER_FROZEN_X = 2,
  AVD_RC_PEAK_GETTER_FROZEN_Y = 3,
  AVD_RC_PEAK_GETTER_INVALID = -1,
  AVD_TASKLIB_UNARY_OP_LOG = 0,
  AVD_TASKLIB_UNARY_OP_LOG2 = 1,
  AVD_TASKLIB_UNARY_OP_LOG10 = 2,
  AVD_TASKLIB_UNARY_OP_SQRT = 3,
  AVD_TASKLIB_UNARY_OP_SINE = 4,
  AVD_TASKLIB_UNARY_OP_COSINE = 5,
  AVD_TASKLIB_UNARY_OP_INVALID = -1,
  AVD_TASKLIB_BINARY_OP_MULT = 0,
  AVD_TASKLIB_BINARY_OP_DIV = 1,
  AVD_TASKLIB_BINARY_OP_INVALID = -1,
  AVD_CPU_DISPATCH_FAMILY_INVALID = -1,
  AVD_CPU_DISPATCH_FAMILY_NOP = 0,
  AVD_CPU_DISPATCH_FAMILY_LABEL = 1,
  AVD_CPU_DISPATCH_FAMILY_PROMOTER = 2,
  AVD_CPU_DISPATCH_FAMILY_STALL = 3,
  AVD_CPU_DISPATCH_FAMILY_DEFAULT = 4,
  AVD_POPACTION_SEED_ACTION_PROCEED = 0,
  AVD_POPACTION_SEED_ACTION_SKIP_AND_COUNT = 1,
  AVD_POPACTION_PARASITE_WARNING_ACTION_INJECT_PARASITE = 0,
  AVD_POPACTION_PARASITE_WARNING_ACTION_INJECT_PARASITE_SEQUENCE = 1,
  AVD_POPACTION_PARASITE_WARNING_ACTION_INJECT_PARASITE_PAIR = 2,
  AVD_POPACTION_PARASITE_WARNING_ACTION_INVALID = -1,
  AVD_POPACTION_PARASITE_WARNING_KIND_INJECT_PARASITE = 0,
  AVD_POPACTION_PARASITE_WARNING_KIND_INJECT_PARASITE_PAIR = 1,
  AVD_POPACTION_PARASITE_WARNING_KIND_INVALID = -1,
  AVD_POPACTION_PARASITE_MISSING_TOKEN_FILENAME = 0,
  AVD_POPACTION_PARASITE_MISSING_TOKEN_LABEL = 1,
  AVD_POPACTION_PARASITE_MISSING_TOKEN_SEQUENCE = 2,
  AVD_POPACTION_PARASITE_MISSING_TOKEN_INVALID = -1,
  AVD_POPACTION_PARASITE_MISSING_ACTION_INJECT_PARASITE = 0,
  AVD_POPACTION_PARASITE_MISSING_ACTION_INJECT_PARASITE_SEQUENCE = 1,
  AVD_POPACTION_PARASITE_MISSING_ACTION_INJECT_PARASITE_PAIR = 2,
  AVD_POPACTION_PARASITE_MISSING_ACTION_INVALID = -1,
  AVD_POPACTION_PARASITE_ERROR_KIND_ORGANISM_FILE = 0,
  AVD_POPACTION_PARASITE_ERROR_KIND_LABEL = 1,
  AVD_POPACTION_PARASITE_ERROR_KIND_SEQUENCE = 2,
  AVD_POPACTION_PARASITE_ERROR_KIND_INVALID = -1,
  AVD_PRINTACTION_FILENAME_MODE_DEFAULT_PLAIN = 0,
  AVD_PRINTACTION_FILENAME_MODE_KEEP_PROVIDED = 1,
  AVD_PRINTACTION_FILENAME_MODE_FORMAT_WITH_INSTSET = 2,
  AVD_PRINTACTION_OUTPUT_SINK_STATS = 0,
  AVD_PRINTACTION_OUTPUT_SINK_RECORDER = 1,
  AVD_PRINTACTION_OUTPUT_SINK_INVALID = -1,
  AVD_CPOP_DEME_BLOCK_SKIP = 0,
  AVD_CPOP_DEME_BLOCK_RUN = 1,
  AVD_CPOP_ROUTING_MODE_PROCESS_STEP = 0,
  AVD_CPOP_ROUTING_MODE_SPECULATIVE_STEP = 1,
  AVD_CPOP_ROUTING_MODE_INVALID = -1,
  AVD_ANALYZE_OUTPUT_FILE_TYPE_KIND_KEEP_CURRENT = 0,
  AVD_ANALYZE_OUTPUT_FILE_TYPE_KIND_HTML = 1,
  AVD_ANALYZE_OUTPUT_SINK_KIND_FILE = 0,
  AVD_ANALYZE_OUTPUT_SINK_KIND_COUT = 1,
  AVD_ANALYZE_OUTPUT_SINK_KIND_INVALID = -1,
  AVD_ANALYZE_OUTPUT_HANDLE_MODE_CREATE = 0,
  AVD_ANALYZE_OUTPUT_HANDLE_MODE_STATIC = 1,
  AVD_ANALYZE_OUTPUT_HANDLE_MODE_INVALID = -1,
  AVD_ANALYZE_OUTPUT_HANDLE_ACTION_DETAIL = 0,
  AVD_ANALYZE_OUTPUT_HANDLE_ACTION_DETAIL_TIMELINE = 1,
  AVD_ANALYZE_OUTPUT_HANDLE_ACTION_HISTOGRAM = 2,
  AVD_ANALYZE_OUTPUT_HANDLE_ACTION_INVALID = -1,
  AVD_ANALYZE_OUTPUT_TOKEN_ABSENT = 0,
  AVD_ANALYZE_OUTPUT_TOKEN_PRESENT = 1,
  AVD_ANALYZE_FILE_TYPE_TOKEN_KIND_INVALID = -1,
  AVD_ANALYZE_FILE_TYPE_TOKEN_KIND_TEXT = 0,
  AVD_ANALYZE_FILE_TYPE_TOKEN_KIND_HTML = 1,
  AVD_ANALYZE_REL_MASK_LESS = 1,
  AVD_ANALYZE_REL_MASK_EQUAL = 2,
  AVD_ANALYZE_REL_MASK_GREATER = 4,
  AVD_ENV_PROCTYPE_ADD = 0,
  AVD_ENV_PROCTYPE_MULT = 1,
  AVD_ENV_PROCTYPE_POW = 2,
  AVD_ENV_PROCTYPE_LIN = 3,
  AVD_ENV_PROCTYPE_ENERGY = 4,
  AVD_ENV_PROCTYPE_ENZYME = 5,
  AVD_ENV_PROCTYPE_EXP = 6,
  AVD_ENV_PROCTYPE_UNKNOWN = -1,
  AVD_ENV_PHENPLAST_DEFAULT = 0,
  AVD_ENV_PHENPLAST_NO_BONUS = 1,
  AVD_ENV_PHENPLAST_FRAC_BONUS = 2,
  AVD_ENV_PHENPLAST_FULL_BONUS = 3,
  AVD_ENV_PHENPLAST_UNKNOWN = -1,
  AVD_ENV_ENTRY_TYPE_PROCESS = 0,
  AVD_ENV_ENTRY_TYPE_REQUISITE = 1,
  AVD_ENV_ENTRY_TYPE_CONTEXT_REQUISITE = 2,
  AVD_ENV_ENTRY_TYPE_UNKNOWN = -1,
  AVD_CPU_THREAD_CHANGE_NONE = 0,
  AVD_CPU_THREAD_CHANGE_KILLED_ONE = 1,
  AVD_CPU_THREAD_CHANGE_DIVIDE = 2,
  AVD_CPU_THREAD_CHANGE_ERROR = 3,
  AVD_CPOP_FORAGER_TYPE_PREY = 0,
  AVD_CPOP_FORAGER_TYPE_TOP_PRED = 1,
  AVD_CPOP_FORAGER_TYPE_PRED = 2,
  AVD_CPU_ALLOC_OK = 0,
  AVD_CPU_ALLOC_TOO_SMALL = 1,
  AVD_CPU_ALLOC_OUT_OF_RANGE = 2,
  AVD_CPU_ALLOC_TOO_LARGE = 3,
  AVD_CPU_ALLOC_PARENT_TOO_LARGE = 4,
  AVD_CPU_MATH_COMPUTE = 0,
  AVD_CPU_MATH_NOOP = 1,
  AVD_CPU_MATH_FAULT_NEGATIVE = 2,
  AVD_CPU_DIV_OK = 0,
  AVD_CPU_DIV_ZERO = 1,
  AVD_CPU_DIV_OVERFLOW = 2,
  AVD_CPOP_DEME_RESET_BOTH = 0,
  AVD_CPOP_DEME_RESET_TARGET_ONLY = 1,
  AVD_CPOP_DEME_RESET_NEITHER = 2,
  AVD_CPOP_DEME_RESET_INVALID = -1,
  AVD_ENV_GEOMETRY_GLOBAL = 0,
  AVD_ENV_GEOMETRY_GRID = 1,
  AVD_ENV_GEOMETRY_TORUS = 2,
  AVD_ENV_GEOMETRY_PARTIAL = 5,
  AVD_ENV_GEOMETRY_UNKNOWN = -1,
  AVD_ENV_BOOL_FALSE = 0,
  AVD_ENV_BOOL_TRUE = 1,
  AVD_ENV_BOOL_INVALID = -1,
  AVD_ENV_GRADIENT_ACTION_BARRIER = 0,
  AVD_ENV_GRADIENT_ACTION_HILLS = 1,
  AVD_ENV_GRADIENT_ACTION_PROBABILISTIC = 2,
  AVD_ENV_GRADIENT_ACTION_PEAK = 3,
  AVD_ENV_CELLBOX_OK = 0,
  AVD_ENV_CELLBOX_BAD_X = 1,
  AVD_ENV_CELLBOX_BAD_Y = 2,
  AVD_ENV_CELLBOX_BAD_WIDTH = 3,
  AVD_ENV_CELLBOX_BAD_HEIGHT = 4,
  AVD_ENV_REQUISITE_REACTION = 0,
  AVD_ENV_REQUISITE_NOREACTION = 1,
  AVD_ENV_REQUISITE_MIN_COUNT = 2,
  AVD_ENV_REQUISITE_MAX_COUNT = 3,
  AVD_ENV_REQUISITE_REACTION_MIN_COUNT = 4,
  AVD_ENV_REQUISITE_REACTION_MAX_COUNT = 5,
  AVD_ENV_REQUISITE_DIVIDE_ONLY = 6,
  AVD_ENV_REQUISITE_MIN_TOT_COUNT = 7,
  AVD_ENV_REQUISITE_MAX_TOT_COUNT = 8,
  AVD_ENV_REQUISITE_PARASITE_ONLY = 9,
  AVD_ENV_REQUISITE_CELLBOX = 10,
  AVD_ENV_REQUISITE_UNKNOWN = -1,
  AVD_ENV_PROCESS_RESOURCE = 0,
  AVD_ENV_PROCESS_VALUE = 1,
  AVD_ENV_PROCESS_TYPE = 2,
  AVD_ENV_PROCESS_MAX = 3,
  AVD_ENV_PROCESS_MIN = 4,
  AVD_ENV_PROCESS_FRAC = 5,
  AVD_ENV_PROCESS_KSUBM = 6,
  AVD_ENV_PROCESS_PRODUCT = 7,
  AVD_ENV_PROCESS_CONVERSION = 8,
  AVD_ENV_PROCESS_INST = 9,
  AVD_ENV_PROCESS_RANDOM = 10,
  AVD_ENV_PROCESS_LETHAL = 11,
  AVD_ENV_PROCESS_STERILIZE = 12,
  AVD_ENV_PROCESS_DEME = 13,
  AVD_ENV_PROCESS_GERMLINE = 14,
  AVD_ENV_PROCESS_DETECT = 15,
  AVD_ENV_PROCESS_THRESHOLD = 16,
  AVD_ENV_PROCESS_DETECTIONERROR = 17,
  AVD_ENV_PROCESS_STRING = 18,
  AVD_ENV_PROCESS_DEPLETABLE = 19,
  AVD_ENV_PROCESS_PHENPLASTBONUS = 20,
  AVD_ENV_PROCESS_INTERNAL = 21,
  AVD_ENV_PROCESS_UNKNOWN = -1,
  AVD_ENV_RES_INFLOW = 0,
  AVD_ENV_RES_OUTFLOW = 1,
  AVD_ENV_RES_INITIAL = 2,
  AVD_ENV_RES_GEOMETRY = 3,
  AVD_ENV_RES_CELLS = 4,
  AVD_ENV_RES_INFLOWX1 = 5,
  AVD_ENV_RES_INFLOWX2 = 6,
  AVD_ENV_RES_INFLOWY1 = 7,
  AVD_ENV_RES_INFLOWY2 = 8,
  AVD_ENV_RES_OUTFLOWX1 = 9,
  AVD_ENV_RES_OUTFLOWX2 = 10,
  AVD_ENV_RES_OUTFLOWY1 = 11,
  AVD_ENV_RES_OUTFLOWY2 = 12,
  AVD_ENV_RES_XDIFFUSE = 13,
  AVD_ENV_RES_XGRAVITY = 14,
  AVD_ENV_RES_YDIFFUSE = 15,
  AVD_ENV_RES_YGRAVITY = 16,
  AVD_ENV_RES_DEME = 17,
  AVD_ENV_RES_COLLECTABLE = 18,
  AVD_ENV_RES_ENERGY = 19,
  AVD_ENV_RES_HGT = 20,
  AVD_ENV_RES_UNKNOWN = -1,
  AVD_ENV_GRAD_PEAKX = 0,
  AVD_ENV_GRAD_PEAKY = 1,
  AVD_ENV_GRAD_HEIGHT = 2,
  AVD_ENV_GRAD_SPREAD = 3,
  AVD_ENV_GRAD_PLATEAU = 4,
  AVD_ENV_GRAD_DECAY = 5,
  AVD_ENV_GRAD_MAX_X = 6,
  AVD_ENV_GRAD_MAX_Y = 7,
  AVD_ENV_GRAD_MIN_X = 8,
  AVD_ENV_GRAD_MIN_Y = 9,
  AVD_ENV_GRAD_MOVE_A_SCALER = 10,
  AVD_ENV_GRAD_UPDATESTEP = 11,
  AVD_ENV_GRAD_HALO = 12,
  AVD_ENV_GRAD_HALO_INNER_RADIUS = 13,
  AVD_ENV_GRAD_HALO_ANCHOR_X = 14,
  AVD_ENV_GRAD_HALO_ANCHOR_Y = 15,
  AVD_ENV_GRAD_MOVE_SPEED = 16,
  AVD_ENV_GRAD_MOVE_RESISTANCE = 17,
  AVD_ENV_GRAD_HALO_WIDTH = 18,
  AVD_ENV_GRAD_PLATEAU_INFLOW = 19,
  AVD_ENV_GRAD_PLATEAU_OUTFLOW = 20,
  AVD_ENV_GRAD_CONE_INFLOW = 21,
  AVD_ENV_GRAD_CONE_OUTFLOW = 22,
  AVD_ENV_GRAD_GRADIENT_INFLOW = 23,
  AVD_ENV_GRAD_INITIAL = 24,
  AVD_ENV_GRAD_COMMON = 25,
  AVD_ENV_GRAD_FLOOR = 26,
  AVD_ENV_GRAD_HABITAT = 27,
  AVD_ENV_GRAD_MIN_SIZE = 28,
  AVD_ENV_GRAD_MAX_SIZE = 29,
  AVD_ENV_GRAD_CONFIG = 30,
  AVD_ENV_GRAD_COUNT = 31,
  AVD_ENV_GRAD_RESISTANCE = 32,
  AVD_ENV_GRAD_DAMAGE = 33,
  AVD_ENV_GRAD_DEADLY = 34,
  AVD_ENV_GRAD_PATH = 35,
  AVD_ENV_GRAD_HAMMER = 36,
  AVD_ENV_GRAD_THRESHOLD = 37,
  AVD_ENV_GRAD_REFUGE = 38,
  AVD_ENV_GRAD_UNKNOWN = -1,
  AVD_LANDSCAPE_DEAD = 0,
  AVD_LANDSCAPE_NEGATIVE = 1,
  AVD_LANDSCAPE_NEUTRAL = 2,
  AVD_LANDSCAPE_POSITIVE = 3,
  AVD_LANDSCAPE_EPI_DEAD = 0,
  AVD_LANDSCAPE_EPI_NEGATIVE = 1,
  AVD_LANDSCAPE_EPI_POSITIVE = 2,
  AVD_LANDSCAPE_EPI_NONE = 3,
  AVD_CPOP_FT_TRANSITION_NONE = 0,
  AVD_CPOP_FT_TRANSITION_PREY_TO_PRED = 1,
  AVD_CPOP_FT_TRANSITION_TOP_PRED_TO_PRED = 2,
  AVD_CPOP_FT_TRANSITION_PREY_TO_TOP_PRED = 3,
  AVD_CPOP_FT_TRANSITION_PRED_TO_TOP_PRED = 4,
  AVD_CPOP_FT_TRANSITION_PRED_TO_PREY = 5,
  AVD_CPOP_FT_TRANSITION_TOP_PRED_TO_PREY = 6,
  AVD_CPOP_MSG_BUFFER_DROP_OLDEST = 0,
  AVD_CPOP_MSG_BUFFER_DROP_NEW = 1,
  AVD_CPOP_MSG_BUFFER_INVALID = -1
};

AvidaRunningStatsHandle* avd_rs_new(void);
AvidaRunningStatsHandle* avd_rs_clone(const AvidaRunningStatsHandle* other);
void avd_rs_free(AvidaRunningStatsHandle* handle);

void avd_rs_clear(AvidaRunningStatsHandle* handle);
void avd_rs_push(AvidaRunningStatsHandle* handle, double x);

double avd_rs_n(const AvidaRunningStatsHandle* handle);
double avd_rs_mean(const AvidaRunningStatsHandle* handle);
double avd_rs_variance(const AvidaRunningStatsHandle* handle);
double avd_rs_std_deviation(const AvidaRunningStatsHandle* handle);
double avd_rs_std_error(const AvidaRunningStatsHandle* handle);
double avd_rs_skewness(const AvidaRunningStatsHandle* handle);
double avd_rs_kurtosis(const AvidaRunningStatsHandle* handle);

AvidaRunningAverageHandle* avd_ra_new(int window_size);
void avd_ra_free(AvidaRunningAverageHandle* handle);

void avd_ra_clear(AvidaRunningAverageHandle* handle);
void avd_ra_add(AvidaRunningAverageHandle* handle, double value);

double avd_ra_sum(const AvidaRunningAverageHandle* handle);
double avd_ra_sum_of_squares(const AvidaRunningAverageHandle* handle);
double avd_ra_average(const AvidaRunningAverageHandle* handle);
double avd_ra_variance(const AvidaRunningAverageHandle* handle);
double avd_ra_std_deviation(const AvidaRunningAverageHandle* handle);
double avd_ra_std_error(const AvidaRunningAverageHandle* handle);

AvidaDoubleSumHandle* avd_ds_new(void);
AvidaDoubleSumHandle* avd_ds_clone(const AvidaDoubleSumHandle* other);
void avd_ds_free(AvidaDoubleSumHandle* handle);

void avd_ds_clear(AvidaDoubleSumHandle* handle);
void avd_ds_add(AvidaDoubleSumHandle* handle, double value, double weight);
void avd_ds_subtract(AvidaDoubleSumHandle* handle, double value, double weight);

double avd_ds_count(const AvidaDoubleSumHandle* handle);
double avd_ds_sum(const AvidaDoubleSumHandle* handle);
double avd_ds_max(const AvidaDoubleSumHandle* handle);
double avd_ds_average(const AvidaDoubleSumHandle* handle);
double avd_ds_variance(const AvidaDoubleSumHandle* handle);
double avd_ds_std_deviation(const AvidaDoubleSumHandle* handle);
double avd_ds_std_error(const AvidaDoubleSumHandle* handle);

AvidaWeightedIndexHandle* avd_wi_new(int size);
AvidaWeightedIndexHandle* avd_wi_clone(const AvidaWeightedIndexHandle* other);
void avd_wi_free(AvidaWeightedIndexHandle* handle);
void avd_wi_set_weight(AvidaWeightedIndexHandle* handle, int id, double weight);
double avd_wi_get_weight(const AvidaWeightedIndexHandle* handle, int id);
double avd_wi_get_total_weight(const AvidaWeightedIndexHandle* handle);
int avd_wi_get_size(const AvidaWeightedIndexHandle* handle);
int avd_wi_find_position(const AvidaWeightedIndexHandle* handle, double position, int root_id);

AvidaOrderedWeightedIndexHandle* avd_owi_new(void);
AvidaOrderedWeightedIndexHandle* avd_owi_clone(const AvidaOrderedWeightedIndexHandle* other);
void avd_owi_free(AvidaOrderedWeightedIndexHandle* handle);
void avd_owi_set_weight(AvidaOrderedWeightedIndexHandle* handle, int value, double weight);
double avd_owi_get_weight(const AvidaOrderedWeightedIndexHandle* handle, int id);
int avd_owi_get_value(const AvidaOrderedWeightedIndexHandle* handle, int id);
double avd_owi_get_total_weight(const AvidaOrderedWeightedIndexHandle* handle);
int avd_owi_get_size(const AvidaOrderedWeightedIndexHandle* handle);
int avd_owi_find_position(const AvidaOrderedWeightedIndexHandle* handle, double position);

AvidaHistogramHandle* avd_hist_new(int max_bin, int min_bin);
void avd_hist_free(AvidaHistogramHandle* handle);
void avd_hist_resize(AvidaHistogramHandle* handle, int new_max, int new_min);
void avd_hist_clear(AvidaHistogramHandle* handle);
void avd_hist_insert(AvidaHistogramHandle* handle, int value, int count);
void avd_hist_remove(AvidaHistogramHandle* handle, int value);
void avd_hist_remove_bin(AvidaHistogramHandle* handle, int value);

double avd_hist_get_average(const AvidaHistogramHandle* handle);
double avd_hist_get_count_average(const AvidaHistogramHandle* handle);
int avd_hist_get_mode(const AvidaHistogramHandle* handle);
double avd_hist_get_variance(const AvidaHistogramHandle* handle);
double avd_hist_get_count_variance(const AvidaHistogramHandle* handle);
double avd_hist_get_std_dev(const AvidaHistogramHandle* handle);
double avd_hist_get_count_std_dev(const AvidaHistogramHandle* handle);
double avd_hist_get_entropy(const AvidaHistogramHandle* handle);
double avd_hist_get_norm_entropy(const AvidaHistogramHandle* handle);

int avd_hist_get_count(const AvidaHistogramHandle* handle);
int avd_hist_get_count_for_value(const AvidaHistogramHandle* handle, int value);
int avd_hist_get_total(const AvidaHistogramHandle* handle);
int avd_hist_get_min_bin(const AvidaHistogramHandle* handle);
int avd_hist_get_max_bin(const AvidaHistogramHandle* handle);
int avd_hist_get_num_bins(const AvidaHistogramHandle* handle);

AvidaRawBitArrayHandle* avd_rba_new(int num_bits);
AvidaRawBitArrayHandle* avd_rba_clone(const AvidaRawBitArrayHandle* other);
void avd_rba_free(AvidaRawBitArrayHandle* handle);
void avd_rba_resize(AvidaRawBitArrayHandle* handle, int old_bits, int new_bits);
void avd_rba_zero(AvidaRawBitArrayHandle* handle, int num_bits);
void avd_rba_ones(AvidaRawBitArrayHandle* handle, int num_bits);
int avd_rba_get_bit(const AvidaRawBitArrayHandle* handle, int index);
void avd_rba_set_bit(AvidaRawBitArrayHandle* handle, int index, int value);
int avd_rba_is_equal(const AvidaRawBitArrayHandle* left, const AvidaRawBitArrayHandle* right, int num_bits);
int avd_rba_count_bits(const AvidaRawBitArrayHandle* handle, int num_bits);
int avd_rba_count_bits2(const AvidaRawBitArrayHandle* handle, int num_bits);
int avd_rba_find_bit1(const AvidaRawBitArrayHandle* handle, int num_bits, int start_pos);
void avd_rba_not(AvidaRawBitArrayHandle* handle, int num_bits);
void avd_rba_and(AvidaRawBitArrayHandle* handle, const AvidaRawBitArrayHandle* other, int num_bits);
void avd_rba_or(AvidaRawBitArrayHandle* handle, const AvidaRawBitArrayHandle* other, int num_bits);
void avd_rba_nand(AvidaRawBitArrayHandle* handle, const AvidaRawBitArrayHandle* other, int num_bits);
void avd_rba_nor(AvidaRawBitArrayHandle* handle, const AvidaRawBitArrayHandle* other, int num_bits);
void avd_rba_xor(AvidaRawBitArrayHandle* handle, const AvidaRawBitArrayHandle* other, int num_bits);
void avd_rba_equ(AvidaRawBitArrayHandle* handle, const AvidaRawBitArrayHandle* other, int num_bits);
void avd_rba_shift(AvidaRawBitArrayHandle* handle, int num_bits, int shift_size);
void avd_rba_increment(AvidaRawBitArrayHandle* handle, int num_bits);

int avd_pkg_array_bool_value(int count);
int avd_pkg_array_int_value(int count);
double avd_pkg_array_double_value(void);
int avd_pkg_str_as_bool(const char* value);
int avd_pkg_str_as_int(const char* value);
double avd_pkg_str_as_double(const char* value);
char* avd_pkg_bool_to_string(int value);
char* avd_pkg_int_to_string(int value);
char* avd_pkg_double_to_string(double value);
char* avd_pkg_array_descriptor(int count);
char* avd_pkg_array_string_value(const char* const* entries, int count);
void avd_pkg_string_free(char* value);

AvidaTimeSeriesHandle* avd_tsr_new(void);
AvidaTimeSeriesHandle* avd_tsr_from_string(const char* serialized);
void avd_tsr_free(AvidaTimeSeriesHandle* handle);
int avd_tsr_len(const AvidaTimeSeriesHandle* handle);
int avd_tsr_update_at(const AvidaTimeSeriesHandle* handle, int index);
char* avd_tsr_value_as_cstr(const AvidaTimeSeriesHandle* handle, int index);
int avd_tsr_value_as_bool(const AvidaTimeSeriesHandle* handle, int index, int* out_value);
int avd_tsr_value_as_int(const AvidaTimeSeriesHandle* handle, int index, int* out_value);
int avd_tsr_value_as_double(const AvidaTimeSeriesHandle* handle, int index, double* out_value);
void avd_tsr_push_bool(AvidaTimeSeriesHandle* handle, int update, int value);
void avd_tsr_push_int(AvidaTimeSeriesHandle* handle, int update, int value);
void avd_tsr_push_double(AvidaTimeSeriesHandle* handle, int update, double value);
void avd_tsr_push_string(AvidaTimeSeriesHandle* handle, int update, const char* value);
char* avd_tsr_as_string(const AvidaTimeSeriesHandle* handle);
void avd_tsr_string_free(char* value);

int avd_provider_is_standard_id(const char* data_id);
int avd_provider_is_argumented_id(const char* data_id);
int avd_provider_split_argumented_id(const char* data_id, char** out_raw_id, char** out_argument);
int avd_provider_classify_id(const char* data_id, char** out_raw_id, char** out_argument);
void avd_provider_string_free(char* value);
double avd_tasklib_fractional_reward_bits(unsigned int supplied, unsigned int correct);
int avd_tasklib_is_logic3_or_math1_name(const char* task_name);
int avd_tasklib_is_math2_or_math3_name(const char* task_name);
int avd_tasklib_is_fibonacci_name(const char* task_name);
int avd_tasklib_is_matching_sequence_name(const char* task_name);
int avd_tasklib_is_load_based_name(const char* task_name);
double avd_tasklib_threshold_halflife_quality(long long diff, int threshold, double halflife_arg);
long long avd_tasklib_diff_scan_init(void);
long long avd_tasklib_diff_scan_update(long long current_min, long long candidate);
long long avd_tasklib_unary_math_input_diff(int input_value, long long test_output, int op_kind, double cast_precision);
long long avd_tasklib_binary_pair_input_diff(int lhs_value, int rhs_value, long long test_output, int op_kind);
double avd_tasklib_fib_check(int test_output, int fib_index);
int avd_tasklib_is_basic_name(const char* task_name);
int avd_tasklib_is_comm_name(const char* task_name);
int avd_tasklib_is_movement_name(const char* task_name);
int avd_tasklib_is_event_name(const char* task_name);
int avd_tasklib_is_altruism_name(const char* task_name);
int avd_task_compute_logic_id(int input0, int input1, int input2, int num_inputs, int output);
double avd_task_eval_logic(int task_type, int logic_id);
double avd_task_eval_echo(const int* inputs, int num_inputs, int output);
int avd_cpu_dispatch_family(int is_nop, int is_label, int is_promoter, int should_stall);
int avd_cpu_dispatch_counted_opcode(int opcode, int dispatch_family);
int avd_cpu_thread_change_kind(int num_threads_before, int thread_size_after);
int avd_cpu_should_die_max_executed(int max_executed, int time_used, int to_die);
int avd_cpu_should_suppress_no_promoter(int promoters_enabled, int no_active_promoter_effect, int promoter_index);
int avd_cpu_should_terminate_promoter(int promoter_inst_max, int promoter_inst_executed);
int avd_cpu_task_switch_penalty(int penalty_type, int num_new_unique_reactions, int penalty_per_switch);
typedef struct {
  int bits;
  unsigned int base;
  int offset;
  double value;
} AvidaMerit;

typedef struct {
  int stack[10];
  unsigned char stack_pointer;
} AvidaCpuStack;

AvidaCpuStack avd_cpu_stack_default(void);
void avd_cpu_stack_push(AvidaCpuStack* s, int value);
int avd_cpu_stack_pop(AvidaCpuStack* s);
int avd_cpu_stack_peek(const AvidaCpuStack* s);
int avd_cpu_stack_get(const AvidaCpuStack* s, int depth);
int avd_cpu_stack_top(const AvidaCpuStack* s);
void avd_cpu_stack_clear(AvidaCpuStack* s);
void avd_cpu_stack_flip(AvidaCpuStack* s);

typedef struct CodeLabel CodeLabel;
CodeLabel* avd_code_label_new(void);
void avd_code_label_free(CodeLabel* label);
CodeLabel* avd_code_label_clone(const CodeLabel* label);
void avd_code_label_add_nop(CodeLabel* label, int nop_num);
int avd_code_label_get_size(const CodeLabel* label);
int avd_code_label_get(const CodeLabel* label, int index);
void avd_code_label_clear(CodeLabel* label);
void avd_code_label_rotate(CodeLabel* label, int rot, int base);
int avd_code_label_eq(const CodeLabel* a, const CodeLabel* b);
int avd_code_label_find_sublabel(const CodeLabel* label, const CodeLabel* sub);
int avd_code_label_as_int(const CodeLabel* label, int base);

AvidaMerit avd_merit_new(double value);
AvidaMerit avd_merit_new_int(int value);
AvidaMerit avd_merit_default(void);
double avd_merit_get_double(const AvidaMerit* m);
unsigned int avd_merit_get_uint(const AvidaMerit* m);
int avd_merit_get_bit(const AvidaMerit* m, int bit_num);
int avd_merit_get_num_bits(const AvidaMerit* m);
double avd_merit_calc_fitness(const AvidaMerit* m, int gestation_time);
void avd_merit_set(AvidaMerit* m, double value);
void avd_merit_add_assign(AvidaMerit* m, const AvidaMerit* other);
void avd_merit_add_assign_double(AvidaMerit* m, double value);
AvidaMerit avd_merit_mul(const AvidaMerit* a, const AvidaMerit* b);
void avd_merit_mul_assign(AvidaMerit* m, const AvidaMerit* other);
void avd_merit_clear(AvidaMerit* m);
int avd_merit_gt(const AvidaMerit* a, const AvidaMerit* b);
int avd_merit_lt(const AvidaMerit* a, const AvidaMerit* b);
int avd_merit_eq(const AvidaMerit* a, const AvidaMerit* b);
int avd_merit_eq_double(const AvidaMerit* m, double val);
int avd_merit_ne_double(const AvidaMerit* m, double val);

typedef struct {
  AvidaMerit merit;
  double execution_ratio;
  double energy_store;
  int genome_length;
  int bonus_instruction_count;
  int copied_size;
  int executed_size;
  int gestation_time;
  int gestation_start;
  double fitness;
  double div_type;
  double cur_bonus;
  double cur_energy_bonus;
} PhenotypeCoreMetrics;

PhenotypeCoreMetrics avd_pheno_core_default(void);
AvidaMerit avd_pheno_get_merit(const PhenotypeCoreMetrics* p);
double avd_pheno_get_execution_ratio(const PhenotypeCoreMetrics* p);
double avd_pheno_get_energy_store(const PhenotypeCoreMetrics* p);
int avd_pheno_get_genome_length(const PhenotypeCoreMetrics* p);
int avd_pheno_get_bonus_instruction_count(const PhenotypeCoreMetrics* p);
int avd_pheno_get_copied_size(const PhenotypeCoreMetrics* p);
int avd_pheno_get_executed_size(const PhenotypeCoreMetrics* p);
int avd_pheno_get_gestation_time(const PhenotypeCoreMetrics* p);
int avd_pheno_get_gestation_start(const PhenotypeCoreMetrics* p);
double avd_pheno_get_fitness(const PhenotypeCoreMetrics* p);
double avd_pheno_get_div_type(const PhenotypeCoreMetrics* p);
double avd_pheno_get_cur_bonus(const PhenotypeCoreMetrics* p);
double avd_pheno_get_cur_energy_bonus(const PhenotypeCoreMetrics* p);
void avd_pheno_set_merit(PhenotypeCoreMetrics* p, AvidaMerit m);
void avd_pheno_set_execution_ratio(PhenotypeCoreMetrics* p, double v);
void avd_pheno_set_energy_store(PhenotypeCoreMetrics* p, double v);
void avd_pheno_set_genome_length(PhenotypeCoreMetrics* p, int v);
void avd_pheno_set_bonus_instruction_count(PhenotypeCoreMetrics* p, int v);
void avd_pheno_set_copied_size(PhenotypeCoreMetrics* p, int v);
void avd_pheno_set_executed_size(PhenotypeCoreMetrics* p, int v);
void avd_pheno_set_gestation_time(PhenotypeCoreMetrics* p, int v);
void avd_pheno_set_gestation_start(PhenotypeCoreMetrics* p, int v);
void avd_pheno_set_fitness(PhenotypeCoreMetrics* p, double v);
void avd_pheno_set_div_type(PhenotypeCoreMetrics* p, double v);
void avd_pheno_set_cur_bonus(PhenotypeCoreMetrics* p, double v);
void avd_pheno_set_cur_energy_bonus(PhenotypeCoreMetrics* p, double v);
int avd_pheno_calc_size_merit(const PhenotypeCoreMetrics* metrics, int base_merit_method, int base_const_merit, int cpu_cycles_used, int fitness_valley, int fitness_valley_start, int fitness_valley_stop, int merit_bonus_effect);

typedef struct {
  /* Section 5: Status Flags */
  int to_die;
  int to_delete;
  int make_random_resource;
  int is_injected;
  int is_clone;

  int is_donor_cur;
  int is_donor_last;
  int is_donor_rand;
  int is_donor_rand_last;
  int is_donor_null;
  int is_donor_null_last;
  int is_donor_kin;
  int is_donor_kin_last;
  int is_donor_edit;
  int is_donor_edit_last;
  int is_donor_gbg;
  int is_donor_gbg_last;
  int is_donor_truegb;
  int is_donor_truegb_last;
  int is_donor_threshgb;
  int is_donor_threshgb_last;
  int is_donor_quanta_threshgb;
  int is_donor_quanta_threshgb_last;
  int is_donor_shadedgb;
  int is_donor_shadedgb_last;

  int is_energy_requestor;
  int is_energy_donor;
  int is_energy_receiver;
  int has_used_donated_energy;
  int has_open_energy_request;

  int num_thresh_gb_donations;
  int num_thresh_gb_donations_last;
  int num_quanta_thresh_gb_donations;
  int num_quanta_thresh_gb_donations_last;
  int num_shaded_gb_donations;
  int num_shaded_gb_donations_last;
  int num_donations_locus;
  int num_donations_locus_last;

  int is_receiver;
  int is_receiver_last;
  int is_receiver_rand;
  int is_receiver_kin;
  int is_receiver_kin_last;
  int is_receiver_edit;
  int is_receiver_edit_last;
  int is_receiver_gbg;
  int is_receiver_truegb;
  int is_receiver_truegb_last;
  int is_receiver_threshgb;
  int is_receiver_threshgb_last;
  int is_receiver_quanta_threshgb;
  int is_receiver_quanta_threshgb_last;
  int is_receiver_shadedgb;
  int is_receiver_shadedgb_last;
  int is_receiver_gb_same_locus;
  int is_receiver_gb_same_locus_last;

  int is_modifier;
  int is_modified;
  int is_fertile;
  int is_mutated;
  int is_multi_thread;
  int parent_true;
  int parent_sex;
  int parent_cross_num;
  int born_parent_group;
  int kaboom_executed;
  int kaboom_executed2;

  /* Section 6: Child information */
  int copy_true;
  int divide_sex;
  int child_fertile;
  int last_child_fertile;
  int mate_select_id;
  int cross_num;
  int child_copied_size;
} PhenotypeStatusFlags;

PhenotypeStatusFlags avd_pheno_flags_default(void);

typedef struct {
  /* Section 2 in-progress extras */
  int cur_num_errors;
  int cur_num_donates;
  int trial_time_used;
  int trial_cpu_cycles_used;
  double last_child_germline_propensity;
  int mating_type;
  int mate_preference;
  int cur_mating_display_a;
  int cur_mating_display_b;

  /* Section 3: last divide scalars */
  double last_merit_base;
  double last_bonus;
  double last_energy_bonus;
  int last_num_errors;
  int last_num_donates;
  double last_fitness;
  int last_cpu_cycles_used;
  double cur_child_germline_propensity;
  int last_mating_display_a;
  int last_mating_display_b;

  /* Section 4: lifetime records */
  int num_divides_failed;
  int num_divides;
  int generation;
  int cpu_cycles_used;
  int time_used;
  int num_execs;
  int age;
  /* fault_desc (cString) stays in C++ */
  double neutral_metric;
  double life_fitness;
  int exec_time_born;
  double gmu_exec_time_born;
  int birth_update;
  int birth_cell_id;
  int av_birth_cell_id;
  int birth_group_id;
  int birth_forager_type;
  int last_task_id;
  int num_new_unique_reactions;
  double res_consumed;
  int is_germ_cell;
  int last_task_time;

  /* Section 7: set once */
  double permanent_germline_propensity;
} PhenotypeLifetimeData;

PhenotypeLifetimeData avd_pheno_lifetime_default(void);

/* Energy System — Slice 3 */
void avd_pheno_set_energy(PhenotypeCoreMetrics* core, double value, double energy_cap);
void avd_pheno_reduce_energy(PhenotypeCoreMetrics* core, double cost, double energy_cap);
int avd_pheno_get_discrete_energy_level(double energy_store, double max_energy, double high_pct, double low_pct);
double avd_pheno_convert_energy_to_merit(double energy, double fix_metabolic_rate, int num_cycles_exc_before_0_energy);
void avd_pheno_double_energy_usage(PhenotypeCoreMetrics* core);
void avd_pheno_halve_energy_usage(PhenotypeCoreMetrics* core);
void avd_pheno_default_energy_usage(PhenotypeCoreMetrics* core);
int avd_pheno_refresh_energy(PhenotypeCoreMetrics* core, double* energy_tobe_applied, int apply_energy_method, double energy_cap);
void avd_pheno_apply_to_energy_store(PhenotypeCoreMetrics* core, double* energy_tobe_applied, double* energy_testament, double energy_cap);
void avd_pheno_apply_donated_energy(PhenotypeCoreMetrics* core, PhenotypeStatusFlags* flags, double* energy_received_buffer, double* total_energy_applied, int* num_energy_applications, double energy_cap);
void avd_pheno_receive_donated_energy(PhenotypeStatusFlags* flags, double* energy_received_buffer, double* total_energy_received, int* num_energy_receptions, double donation);

int avd_script_get_runtime_type(int ltype, int rtype, int allow_str);
int avd_script_valid_arithmetic_type(int type_val, int allow_matrix);
int avd_script_valid_bitwise_type(int type_val);
int avd_landscape_fitness_category(double fitness, double neut_min, double neut_max);
int avd_landscape_epistasis_category(double mut1_fitness, double mut2_fitness, double combo_fitness);
int avd_cpu_should_update_test_resources(int res_method, int cpu_cycles_used, int ave_time_slice);
int avd_cpu_clamp_max_genome_size(int config_value, int absolute_max);
int avd_cpu_clamp_min_genome_size(int config_value, int absolute_min);
int avd_cpu_gradient_facing(int northerly, int easterly);
int avd_cpu_alloc_validity(int allocated_size, int old_size, int min_genome, int max_genome, int max_alloc_size, int max_old_size);
int avd_cpu_next_register(int default_register, int num_registers);
int avd_cpu_prev_register(int default_register, int num_registers);
int avd_cpu_unary_math_domain(int value, int threshold);
int avd_cpu_div_guard(int op1, int op2, int int_min);
int avd_popaction_deme_loop_start_index(int energy_enabled);
int avd_popaction_seed_deme_action(int energy_enabled, int injected_count);
int avd_popaction_normalize_cell_end(int cell_start, int cell_end);
int avd_popaction_is_valid_cell_range(int cell_start, int cell_end, int population_size);
int avd_popaction_is_valid_cell_range_with_stride(int cell_start, int cell_end, int population_size, int cell_stride);
int avd_popaction_is_missing_filename_token(int filename_size);
int avd_popaction_is_valid_well_mixed_cell_count(int cell_count, int population_size);
int avd_popaction_is_valid_single_cell_id(int cell_id, int population_size);
int avd_popaction_is_valid_group_cell_id(int cell_id, int population_size);
int avd_popaction_should_skip_parasite_injection(int only_if_parasites_extinct, int num_parasites);
int avd_popaction_is_missing_parasite_filename_token(int filename_size);
int avd_popaction_has_missing_parasite_pair_filenames(int genome_filename_size, int parasite_filename_size);
int avd_popaction_is_missing_parasite_label_token(int label_size);
int avd_popaction_is_missing_parasite_sequence_token(int sequence_size);
int avd_popaction_parasite_invalid_range_warning_kind(int action_kind);
int avd_popaction_parasite_warning_short_circuit_kind(int action_kind, int is_invalid_range);
int avd_popaction_parasite_missing_token_short_circuit_kind(int action_kind, int missing_filename, int missing_label, int missing_sequence);
int avd_popaction_parasite_missing_token_error_kind(int missing_token_kind);
int avd_printaction_instruction_filename_mode(int has_filename_token, int filename_empty);
int avd_printaction_instruction_output_sink_kind(int action_kind);
int avd_cpop_should_check_implicit_deme_repro(int num_demes);
int avd_cpop_should_run_speculative_deme_block(int num_demes);
int avd_cpop_should_update_deme_counters(int num_demes);
int avd_cpop_should_run_multi_deme_block(int num_demes);
int avd_cpop_deme_routing_short_circuit_kind(int routing_mode, int num_demes);
int avd_cpop_is_pred_prey_tracking_active(int pred_prey_switch);
int avd_cpop_forager_type_kind(int is_prey_ft, int is_top_pred_ft);
int avd_cpop_is_deadly_boundary(int deadly_boundaries, int geometry, int dest_x, int dest_y, int world_x, int world_y);
int avd_cpop_is_valid_prey_target(int forage_target, int parent_ft);
int avd_cpop_is_merit_bonus_enabled(int rewarded_instruction);
int avd_cpop_deme_reset_resources_kind(int config_value);
int avd_cpop_should_kill_rand_prey(int max_prey, int num_prey, int is_prey_ft);
int avd_cpop_should_kill_test_birth(int birth_method, int is_inject);
int avd_cpop_forage_target_transition(int new_ft, int old_ft);
int avd_cpop_is_birth_method_eldest(int birth_method);
int avd_cpop_is_divide_method_split(int divide_method);
int avd_cpop_is_generation_inc_both(int gen_inc_method);
int avd_cpop_is_divide_method_split_or_birth(int divide_method);
int avd_cpop_should_copy_parent_ft(int pred_prey_switch, int parent_ft, int forage_target);
int avd_cpop_should_kill_rand_pred(int parent_ft, int max_pred, int num_total_pred);
int avd_cpop_msg_buffer_overflow_action(int behavior);
int avd_cpop_is_msg_buffer_full(int buffer_size, int current_count);
int avd_analyze_relation_mask(const char* relation);
int avd_analyze_is_html_extension(const char* extension);
int avd_analyze_is_html_filename_token(const char* filename_token);
int avd_analyze_output_file_type_short_circuit_kind(int has_html_extension);
int avd_analyze_output_sink_short_circuit_kind(int is_cout_filename);
int avd_analyze_output_file_handle_mode_short_circuit_kind(int action_kind);
int avd_analyze_output_token_presence_short_circuit_kind(int remaining_arg_size);
int avd_analyze_file_type_token_short_circuit_kind(int has_text_token, int has_html_token);
int avd_analyze_apply_file_type_token_policy(int has_text_token, int has_html_token, int current_file_type, int text_file_type, int html_file_type);
int avd_rc_lookup_resource_index(const char* const* names, int count, const char* query);
double avd_rc_step_inflow(double inflow, double update_step);
double avd_rc_step_decay(double decay_rate, double update_step);
double avd_rc_inflow_precalc_next(double previous, double step_decay, double step_inflow);
double avd_rc_decay_precalc_next(double previous, double step_decay);
void avd_rc_fill_precalc_tables(double decay_rate, double inflow, double update_step, int precalc_distance, double* out_decay, double* out_inflow);
void avd_rc_fill_inflow_precalc_table(double decay_rate, double inflow, double update_step, int precalc_distance, double* out_inflow);
void avd_rc_fill_decay_precalc_table(double decay_rate, double update_step, int precalc_distance, double* out_decay);
double avd_rc_accumulate_update_time(double current, double delta);
double avd_rc_update_time_delta(double in_time);
int avd_rc_wrapper_global_only_flag(int wrapper_mode);
int avd_rc_num_steps(double update_time, double update_step);
int avd_rc_num_spatial_updates(int current_update, int previous_update);
double avd_rc_remainder_update_time(double update_time, double update_step, int num_steps);
double avd_rc_apply_nonspatial_steps(double current, const double* decay_precalc, const double* inflow_precalc, int precalc_distance, int num_steps);
int avd_rc_spatial_step_iterations(int num_updates);
int avd_rc_use_cell_list_branch(int cell_list_size);
int avd_rc_is_spatial_geometry(int geometry);
int avd_rc_dispatch_action(int is_spatial, int global_only);
int avd_rc_should_advance_last_updated(int global_only);
int avd_rc_read_path_kind(int geometry);
int avd_rc_setcell_write_path_kind(int geometry);
int avd_rc_setup_path_kind(int geometry);
int avd_rc_should_log_spatial_rectangles(int geometry);
int avd_rc_resize_cell_count(int world_x, int world_y);
int avd_rc_gradient_setter_count(void);
int avd_rc_gradient_setter_opcode(int index);
int avd_rc_gradient_scalar_setter_count(void);
int avd_rc_gradient_scalar_setter_opcode(int index);
int avd_rc_gradient_var_inflow_setter_count(void);
int avd_rc_gradient_var_inflow_setter_opcode(int index);
int avd_rc_predatory_setter_count(void);
int avd_rc_predatory_setter_opcode(int index);
int avd_rc_probabilistic_setter_count(void);
int avd_rc_probabilistic_setter_opcode(int index);
int avd_rc_peak_getter_count(void);
int avd_rc_peak_getter_opcode(int index);
int avd_rc_peak_getter_requires_update(int opcode);
int avd_src_normalize_span(int start, int end, int bound, int* out_start, int* out_end);
double avd_src_compute_flow_scalar(double elem1_amount, double elem2_amount, double inxdiffuse, double inydiffuse, double inxgravity, double inygravity, int xdist, int ydist, double dist);
int avd_src_compute_flow_pair_deltas(double elem1_amount, double elem2_amount, double inxdiffuse, double inydiffuse, double inxgravity, double inygravity, int xdist, int ydist, double dist, double* out_elem1_delta, double* out_elem2_delta);
double avd_src_source_per_cell(double amount, int x1, int x2, int y1, int y2);
double avd_src_sink_delta(double current_amount, double decay);
double avd_src_cell_outflow_delta(double current_amount, double outflow);
int avd_src_wrapped_elem_index(int x, int y, int world_x, int world_y);
int avd_src_cell_id_in_bounds_strict(int cell_id, int grid_size);
int avd_src_cell_id_in_bounds_legacy_setcell(int cell_id, int grid_size);
int avd_src_setpointer_entry(int cell_id, int world_x, int world_y, int geometry, int slot, int* out_elempt, int* out_xdist, int* out_ydist, double* out_dist);
int avd_src_state_fold(double amount, double delta, double* out_amount, double* out_delta);
double avd_src_sum_amounts(const double* values, int count);
int avd_src_rate_next_delta(double current_delta, double rate_in, double* out_delta);
int avd_src_reset_amount(double res_initial, double cell_initial, double* out_amount);
int avd_src_setcell_apply_initial(double amount, double delta, double cell_initial, double* out_amount, double* out_delta);
int avd_rh_select_entry_index(const int* updates, int count, int update, int exact);
double avd_rh_value_at_or_zero(const double* values, int count, int index);
int avd_stats_is_dual_task_filename(const char* filename);
int avd_stats_is_dual_internal_task_filename(const char* filename);
int avd_stats_is_spatial_resource(int geometry);
double avd_stats_task_quality_average(double quality, int count);
int avd_stats_is_wall_gradient(int is_gradient, int habitat);
int avd_stats_is_den_habitat(int habitat);
int avd_stats_is_gradient_resource(int is_gradient);
double avd_stats_safe_divide_or_default(int numerator, int denominator, double default_val);
int avd_stats_is_juvenile(int time_used, int juv_period);

typedef struct {
  double mut_prob;
  double ins_prob;
  double del_prob;
  double uniform_prob;
  double slip_prob;
} AvidaCopyMuts;

typedef struct {
  double ins_prob;
  double del_prob;
  double mut_prob;
  double uniform_prob;
  double slip_prob;
  double trans_prob;
  double lgt_prob;
  double divide_mut_prob;
  double divide_ins_prob;
  double divide_del_prob;
  double divide_uniform_prob;
  double divide_slip_prob;
  double divide_trans_prob;
  double divide_lgt_prob;
  double divide_poisson_mut_mean;
  double divide_poisson_ins_mean;
  double divide_poisson_del_mean;
  double divide_poisson_slip_mean;
  double divide_poisson_trans_mean;
  double divide_poisson_lgt_mean;
  double parent_mut_prob;
  double parent_ins_prob;
  double parent_del_prob;
} AvidaDivideMuts;

typedef struct {
  double ins_prob;
  double del_prob;
  double mut_prob;
} AvidaPointMuts;

typedef struct {
  double ins_prob;
  double del_prob;
  double mut_prob;
} AvidaInjectMuts;

typedef struct {
  double copy_mut_prob;
  double standard_dev;
} AvidaMetaMuts;

typedef struct {
  double death_prob;
} AvidaUpdateMuts;

typedef struct {
  AvidaCopyMuts copy;
  AvidaDivideMuts divide;
  AvidaPointMuts point;
  AvidaInjectMuts inject;
  AvidaMetaMuts meta;
  AvidaUpdateMuts update;
} AvidaMutationRates;

AvidaMutationRates avd_mutation_rates_default(void);
void avd_mutation_rates_clear(AvidaMutationRates* m);

int avd_env_process_type(const char* type_str);
int avd_env_phenplast_bonus_method(const char* method_str);
int avd_env_reaction_entry_type(const char* entry_str);
double avd_deme_base_merit(int method, double const_merit);
int avd_deme_should_join_germline_first(int selection_method);
double avd_deme_reaction_weight(double slope, int index);

int avd_env_gradient_var_kind(const char* var_name);
int avd_env_resource_var_kind(const char* var_name);
int avd_env_process_var_kind(const char* var_name);
int avd_env_cellbox_validate(int xx, int yy, int width, int height, int world_x, int world_y);
int avd_env_requisite_var_kind(const char* var_name);

int avd_env_gradient_update_action(int habitat, int is_probabilistic);
int avd_env_gradient_temp_height(double plateau, int height);
int avd_env_gradient_should_fillin(double move_a_scaler, double plateau_inflow, double plateau_outflow, double cone_inflow, double cone_outflow, double gradient_inflow, int just_reset);
int avd_env_geometry_type(const char* geometry_str);
int avd_sensor_normalize_search_type(int habitat_used, int search_type, int pred_experiment, int is_predator);
int avd_sensor_clamp_distance(int distance_sought, int max_dist);
int avd_sensor_max_distance(int look_dist_config, int world_x, int world_y);
int avd_sensor_clamp_id_sought(int id_sought);
int avd_env_parse_bool_string(const char* value_str);

int avd_event_parse_trigger(const char* token);
int avd_event_parse_timing(const char* timing, double* out_start, double* out_interval, double* out_stop);

/* Slice 5 Phase 1: task-count array bulk operations */
void avd_pheno_reset_int_array(int* data, int len);
void avd_pheno_copy_int_array(int* dst, const int* src, int len);
void avd_pheno_reset_double_array(double* data, int len);
void avd_pheno_copy_double_array(double* dst, const double* src, int len);
void avd_pheno_fill_int_array(int* data, int len, int value);
void avd_pheno_fill_double_array(double* data, int len, double value);
void avd_pheno_divide_snapshot_tasks(
    int* last_task_count, const int* cur_task_count, int task_count_len,
    int* last_host_tasks, const int* cur_host_tasks, int host_tasks_len,
    int* last_internal_task_count, const int* cur_internal_task_count, int internal_task_count_len,
    double* last_task_quality, const double* cur_task_quality, int task_quality_len,
    double* last_task_value, const double* cur_task_value, int task_value_len,
    double* last_internal_task_quality, const double* cur_internal_task_quality, int internal_task_quality_len
);

/* cBirthEntry scalar fields — Slice 1 of issue #49 */
typedef struct {
  int mating_type;
  int mating_display_a;
  int mating_display_b;
  int mate_preference;
  int group_id;
  double energy4_offspring;
  AvidaMerit merit;
  int timestamp;
} BirthEntryScalars;

BirthEntryScalars avd_birth_scalars_default(void);

typedef struct {
  int peakx;
  int peaky;
  int height;
  int spread;
  double plateau;
  int decay;
  int max_x;
  int max_y;
  int min_x;
  int min_y;
  double move_a_scaler;
  int updatestep;
  int halo;
  int halo_inner_radius;
  int halo_width;
  int halo_anchor_x;
  int halo_anchor_y;
  int move_speed;
  int move_resistance;
  double plateau_inflow;
  double plateau_outflow;
  double cone_inflow;
  double cone_outflow;
  double gradient_inflow;
  int is_plateau_common;
  double floor;
  int habitat;
  int min_size;
  int max_size;
  int config;
  int count;
  double initial_plat;
  double threshold;
  double damage;
  int geometry;
} GradientConfig;

GradientConfig avd_gradient_config_default(void);

typedef struct {
  int initial;
  double move_y_scaler;
  int counter;
  int move_counter;
  int topo_counter;
  int movesignx;
  int movesigny;
  int old_peakx;
  int old_peaky;
  int halo_dir;
  int changling;
  int just_reset;
  double past_height;
  double current_height;
  double ave_plat_cell_loss;
  double common_plat_height;
  int skip_moves;
  int skip_counter;
  double mean_plat_inflow;
  double var_plat_inflow;
  double pred_odds;
  int predator;
  double death_odds;
  int deadly;
  int path;
  int hammer;
  int guarded_juvs_per_adult;
  int probabilistic;
  int min_usedx;
  int min_usedy;
  int max_usedx;
  int max_usedy;
} GradientState;

GradientState avd_gradient_state_default(void);

/* ---- CpuRegisters ---- */

typedef struct {
  int reg[3];
} CpuRegisters;

CpuRegisters avd_cpu_reg_default(void);
int  avd_cpu_reg_get(const CpuRegisters* regs, int idx);
void avd_cpu_reg_set(CpuRegisters* regs, int idx, int val);
void avd_cpu_reg_reset(CpuRegisters* regs);

/* unary */
void avd_cpu_reg_inc(CpuRegisters* regs, int dst);
void avd_cpu_reg_dec(CpuRegisters* regs, int dst);
void avd_cpu_reg_zero(CpuRegisters* regs, int dst);
void avd_cpu_reg_one(CpuRegisters* regs, int dst);
void avd_cpu_reg_all1s(CpuRegisters* regs, int dst);
void avd_cpu_reg_neg(CpuRegisters* regs, int dst);
void avd_cpu_reg_square(CpuRegisters* regs, int dst);
void avd_cpu_reg_not(CpuRegisters* regs, int dst);
void avd_cpu_reg_shift_r(CpuRegisters* regs, int dst);
void avd_cpu_reg_shift_l(CpuRegisters* regs, int dst);
void avd_cpu_reg_bit1(CpuRegisters* regs, int dst);

/* binary */
void avd_cpu_reg_add(CpuRegisters* regs, int dst, int op1, int op2);
void avd_cpu_reg_sub(CpuRegisters* regs, int dst, int op1, int op2);
void avd_cpu_reg_mult(CpuRegisters* regs, int dst, int op1, int op2);
void avd_cpu_reg_nand(CpuRegisters* regs, int dst, int op1, int op2);
void avd_cpu_reg_and(CpuRegisters* regs, int dst, int op1, int op2);
void avd_cpu_reg_xor(CpuRegisters* regs, int dst, int op1, int op2);
void avd_cpu_reg_or(CpuRegisters* regs, int dst, int op1, int op2);

/* register ops */
void avd_cpu_reg_swap(CpuRegisters* regs, int r1, int r2);
void avd_cpu_reg_copy(CpuRegisters* regs, int dst, int src);
void avd_cpu_reg_order(CpuRegisters* regs, int r1, int r2);
void avd_cpu_reg_setbit(CpuRegisters* regs, int to_set, int bit_reg);
void avd_cpu_reg_clearbit(CpuRegisters* regs, int to_clear, int bit_reg);

double avd_task_eval_logic3in(int logic_id, int target);
double avd_task_eval_logic3in_3(int logic_id, int t1, int t2, int t3);
double avd_task_eval_logic3in_4(int logic_id, int t1, int t2, int t3, int t4);
double avd_task_eval_logic3in_6(int logic_id, int t1, int t2, int t3, int t4, int t5, int t6);
double avd_task_eval_math1in(const int* inputs, int num_inputs, int output, int op);
double avd_task_eval_math2in(const int* inputs, int num_inputs, int output, int op);
double avd_task_eval_math3in(const int* inputs, int num_inputs, int output, int op);
double avd_task_eval_simple_arith(const int* inputs, int num_inputs, int output, int op);

/* ---------- ConfigSnapshot FFI ---------- */
ConfigSnapshot avd_config_snapshot_default(void);

/* ---------- TaskContextSnapshot FFI ---------- */
TaskContextSnapshot avd_task_ctx_default(void);
double avd_task_ctx_dont_care(const TaskContextSnapshot* ctx);
double avd_task_ctx_exploded(const TaskContextSnapshot* ctx);
double avd_task_ctx_exploded2(const TaskContextSnapshot* ctx);
double avd_task_ctx_all_ones(const TaskContextSnapshot* ctx);
double avd_task_ctx_match_number(const TaskContextSnapshot* ctx);
double avd_task_ctx_opinion_is(const TaskContextSnapshot* ctx);
double avd_task_ctx_consume_target(const TaskContextSnapshot* ctx);
double avd_task_ctx_consume_target_logic(const TaskContextSnapshot* ctx, int task_type);
double avd_task_ctx_move_up_gradient(const TaskContextSnapshot* ctx);
double avd_task_ctx_move_neutral_gradient(const TaskContextSnapshot* ctx);
double avd_task_ctx_move_down_gradient(const TaskContextSnapshot* ctx);
double avd_task_ctx_move_not_up_gradient(const TaskContextSnapshot* ctx);
double avd_task_ctx_event_killed(const TaskContextSnapshot* ctx);
double avd_task_ctx_consume_target_echo(const TaskContextSnapshot* ctx);
double avd_task_ctx_collect_odd_cell(const TaskContextSnapshot* ctx);
double avd_task_ctx_produce_public_good(const TaskContextSnapshot* ctx);
double avd_task_eval_fibonacci_seq(int* seq, int* count, int output_value, int target_count, double penalty);
void avd_landscape_entropy_complexity(const int* site_count, int genome_size, int inst_set_size, double* out_entropy, double* out_complexity);
int avd_birth_is_valid_entry(int timestamp, int max_wait_time, int cur_update);
double avd_task_eval_ai_display_cost(int lyse_display);
double avd_task_eval_move_to_event(const int* event_ids, int num_events, int cell_data);
double avd_task_eval_royal_road(const int* buf, int buf_len, int length, int block_count);
double avd_task_eval_royal_road_wd(const int* buf, int buf_len, int length, int block_count, int width, int height);
double avd_task_eval_form_spatial_group(int ideal_size, int group_size);
double avd_task_eval_form_spatial_group_with_id(int ideal_size, int group_id, int desired_group_id, int group_size);
double avd_task_eval_comm_echo(int output_value, const int* neighbor_values, int count);
double avd_task_eval_comm_not(int output_value, const int* neighbor_values, int count);
double avd_task_eval_nand_res_dep(int logic_id, double pheromone_amount);
double avd_task_eval_nor_res_dep(int logic_id, double pheromone_amount);
double avd_task_eval_move_ft(int effective_cell_id, int prev_seen_cell_id, int forage_target, int desired_target);
double avd_task_eval_all_ones(const int* buf, int buf_len, int length);
double avd_task_eval_move_to_side(int cell_pos_x, int target_x);
double avd_task_eval_move(int effective_cell_id, int prev_seen_cell_id);
double avd_task_eval_move_to_target(int cell_data, int current_cell, int prev_target);

// Forward declaration needed for popif helpers
typedef struct cAvidaContext cAvidaContext;

// ---- cPopulationInterface deterministic helpers ----
int avd_popif_av_faced_cell(int old_cell_id, int facing, int x_size, int y_size, int num_demes, int world_geometry, cAvidaContext* ctx);
int avd_popif_rotate_av(int current_facing, int increment);
int avd_popif_av_num_neighbors(int cell_id, int x_size, int y_size, int num_demes, int world_geometry);
int avd_popif_message_send_policy(int drop_hit, int cell_occupied, int neural_networking, int use_avatars_2, int num_av_inputs, int self_communication, int has_other_input_av);

// ---- Organism/Cell/Population/Deme read-only FFI ----
// Opaque pointers — C++ retains ownership, Rust reads through these.
typedef struct cOrganism cOrganism;
typedef struct cPopulationCell cPopulationCell;
typedef struct cPopulation cPopulation;
typedef struct cWorld cWorld;
typedef struct cDeme cDeme;

// Organism accessors
double avd_org_get_fitness(cOrganism* org);
int avd_org_get_id(cOrganism* org);
int avd_org_get_lyse_display(cOrganism* org);
int avd_org_get_cell_data_org(cOrganism* org);
int avd_org_get_input_at(cOrganism* org, int index);
double avd_org_get_stored_energy(cOrganism* org);
int avd_org_is_fertile(cOrganism* org);
int avd_org_is_germline(cOrganism* org);
int avd_org_get_num_divides(cOrganism* org);
int avd_org_get_kaboom_executed(cOrganism* org);
int avd_org_get_reputation(cOrganism* org);
int avd_org_get_faced_dir(cOrganism* org);
int avd_org_get_northerly(cOrganism* org);
int avd_org_get_easterly(cOrganism* org);
int avd_org_get_neighbor_cell_contents(cOrganism* org);
int avd_org_get_faced_cell_data_org_id(cOrganism* org);
int avd_org_get_number_strings_on_hand(cOrganism* org, int type);
int avd_org_get_mating_type(cOrganism* org);
double avd_org_get_energy_in_buffer(cOrganism* org);
int avd_org_get_cell_y_position(cOrganism* org);
cOrganism* avd_org_get_neighbor(cOrganism* org);
int avd_org_is_neighbor_cell_occupied(cOrganism* org);
int avd_org_is_dead(cOrganism* org);
double avd_org_get_vitality(cOrganism* org);
int avd_org_get_discrete_energy_level(cOrganism* org);
int avd_org_get_cell_position_x(cOrganism* org);
int avd_org_get_opinion_only(cOrganism* org);
double avd_org_get_merit(cOrganism* org);
int avd_org_get_gestation_time(cOrganism* org);
int avd_org_get_genome_length(cOrganism* org);
int avd_org_get_forage_target(cOrganism* org);
int avd_org_get_cell_id(cOrganism* org);
double avd_org_get_cur_bonus(cOrganism* org);
int avd_org_get_generation(cOrganism* org);
int avd_org_get_time_used(cOrganism* org);
int avd_org_get_av_cell_id(cOrganism* org);
int avd_org_get_prev_seen_cell_id(cOrganism* org);
void avd_org_set_prev_seen_cell_id(cOrganism* org, int id);
int avd_org_has_opinion(cOrganism* org);
int avd_org_get_opinion_value(cOrganism* org);
cDeme* avd_org_get_deme(cOrganism* org);
// Phase 1c read-only (session 15-16)
int avd_org_get_reputation(cOrganism* org);
int avd_org_get_faced_dir(cOrganism* org);
int avd_org_get_northerly(cOrganism* org);
int avd_org_get_easterly(cOrganism* org);
int avd_org_get_neighbor_cell_contents(cOrganism* org);
int avd_org_get_faced_cell_data_org_id(cOrganism* org);
int avd_org_get_number_strings_on_hand(cOrganism* org, int type);
int avd_org_get_mating_type(cOrganism* org);
double avd_org_get_energy_in_buffer(cOrganism* org);
int avd_org_get_cell_y_position(cOrganism* org);
cOrganism* avd_org_get_neighbor(cOrganism* org);
int avd_org_is_neighbor_cell_occupied(cOrganism* org);
int avd_org_is_dead(cOrganism* org);
double avd_org_get_vitality(cOrganism* org);
int avd_org_get_discrete_energy_level(cOrganism* org);
int avd_org_get_cell_position_x(cOrganism* org);
int avd_org_get_opinion_only(cOrganism* org);
// Phase 2: mutable organism state
void avd_org_reduce_energy(cOrganism* org, double amount);
void avd_org_increase_energy_donated(cOrganism* org, double amount);
void avd_org_receive_donated_energy(cOrganism* org, double amount);
void avd_org_apply_donated_energy(cOrganism* org);
void avd_org_set_frac_energy_donating(cOrganism* org, double frac);
void avd_org_set_fertile(cOrganism* org, int value);
void avd_org_set_kaboom_executed(cOrganism* org, int value);
void avd_org_set_is_energy_donor(cOrganism* org);
void avd_org_set_is_energy_receiver(cOrganism* org);
void avd_org_set_cur_bonus(cOrganism* org, double value);
double avd_org_get_rbin(cOrganism* org, int index);
int avd_org_get_num_rbins(cOrganism* org);
void avd_org_add_to_rbin(cOrganism* org, int index, double amount);
void avd_org_set_rbin(cOrganism* org, int index, double value);
void avd_org_set_pheromone(cOrganism* org, int value);
void avd_org_set_is_energy_requestor(cOrganism* org);
void avd_org_increase_num_energy_requests(cOrganism* org);
void avd_org_set_has_open_energy_request(cOrganism* org);
void avd_org_clear_has_open_energy_request(cOrganism* org);
double avd_org_get_frac_energy_donating(cOrganism* org);
void avd_org_join_germline(cOrganism* org);
void avd_org_exit_germline(cOrganism* org);
void avd_org_toggle_pheromone(cOrganism* org);
void avd_org_repair_point_mut_on(cOrganism* org);
void avd_org_repair_point_mut_off(cOrganism* org);
void avd_org_clear_easterly(cOrganism* org);
void avd_org_clear_northerly(cOrganism* org);
void avd_org_set_lyse_display(cOrganism* org);
void avd_org_set_mate_preference(cOrganism* org, int pref);
int avd_org_get_cur_mating_display_a(cOrganism* org);
int avd_org_get_cur_mating_display_b(cOrganism* org);
void avd_org_set_cur_mating_display_a(cOrganism* org, int val);
void avd_org_set_cur_mating_display_b(cOrganism* org, int val);
void avd_org_set_cell_data(cOrganism* org, int data);
double avd_org_get_copy_mut_prob(cOrganism* org);
void avd_org_set_copy_mut_prob(cOrganism* org, double prob);
void avd_org_add_output(cOrganism* org, int val);

// Cell accessors
int avd_cell_is_occupied(cPopulationCell* cell);
cOrganism* avd_cell_get_organism(cPopulationCell* cell);
int avd_cell_get_id(cPopulationCell* cell);
int avd_cell_get_deme_id(cPopulationCell* cell);
int avd_cell_get_data(cPopulationCell* cell);
int avd_cell_get_x(cPopulationCell* cell);
int avd_cell_get_y(cPopulationCell* cell);

// Population accessors
cPopulationCell* avd_pop_get_cell(cPopulation* pop, int cell_id);
int avd_pop_get_size(cPopulation* pop);
int avd_pop_get_num_organisms(cPopulation* pop);
int avd_pop_get_num_demes(cPopulation* pop);
cDeme* avd_pop_get_deme(cPopulation* pop, int deme_id);
int avd_pop_get_num_in_group(cPopulation* pop, int group_id);

// Deme accessors
int avd_deme_get_id(cDeme* deme);
int avd_deme_get_size(cDeme* deme);
int avd_deme_get_width(cDeme* deme);
int avd_deme_get_height(cDeme* deme);
int avd_deme_get_cell_position_x(cDeme* deme, int cell_id);
int avd_deme_get_cell_position_y(cDeme* deme, int cell_id);
int avd_deme_get_relative_cell_id(cDeme* deme, int absolute_cell_id);
int avd_deme_get_num_events(cDeme* deme);
int avd_deme_get_cell_event_id(cDeme* deme, int event_idx);
int avd_deme_get_org_count(cDeme* deme);

// World accessors
cPopulation* avd_world_get_population(cWorld* world);
int avd_world_get_update(cWorld* world);

// ---- Hardware (CPU) FFI ----
// Opaque pointer: cHardwareBase* — always safe-cast to cHardwareCPU* inside.
typedef struct cHardwareBase cHardwareBase;

// Head accessors (head_id: 0=IP, 1=READ, 2=WRITE, 3=FLOW)
int avd_hw_get_head_position(cHardwareBase* hw, int head_id);
void avd_hw_set_head_position(cHardwareBase* hw, int head_id, int pos);
int avd_hw_get_ip_position(cHardwareBase* hw);
void avd_hw_set_ip_position(cHardwareBase* hw, int pos);
void avd_hw_advance_ip(cHardwareBase* hw);

// Stack accessors (active stack for current thread)
void avd_hw_stack_push(cHardwareBase* hw, int value);
int avd_hw_stack_pop(cHardwareBase* hw);
void avd_hw_switch_stack(cHardwareBase* hw);
void avd_hw_stack_flip(cHardwareBase* hw);
void avd_hw_stack_clear(cHardwareBase* hw);
int avd_hw_get_stack(cHardwareBase* hw, int depth, int stack_id);
int avd_hw_get_cur_stack_id(cHardwareBase* hw);

// Register accessors
int avd_hw_get_register(cHardwareBase* hw, int reg_id);
void avd_hw_set_register(cHardwareBase* hw, int reg_id, int value);
CpuRegisters* avd_hw_get_regs(cHardwareBase* hw);

// Register/nop helpers (FindModifiedRegister has IP side effects)
int avd_hw_find_modified_register(cHardwareBase* hw, int default_reg);
int avd_hw_find_modified_next_register(cHardwareBase* hw, int default_reg);
int avd_hw_find_next_register(int base_reg);
int avd_hw_find_modified_head(cHardwareBase* hw, int default_head);

// Memory accessors
int avd_hw_get_memory_size(cHardwareBase* hw);
int avd_hw_get_memory_inst(cHardwareBase* hw, int idx);
void avd_hw_set_memory_inst(cHardwareBase* hw, int idx, int inst_id);
int avd_hw_get_memory_flags(cHardwareBase* hw, int idx);
cOrganism* avd_hw_get_organism(cHardwareBase* hw);
int avd_hw_get_inst_set_size(cHardwareBase* hw);

// ---- cAvidaContext RNG accessors ----
int avd_ctx_random_p(cAvidaContext* ctx, double prob);
int avd_ctx_random_get_uint(cAvidaContext* ctx, unsigned int max);
double avd_ctx_random_get_double(cAvidaContext* ctx);
int avd_ctx_random_get_int(cAvidaContext* ctx, int max);
int avd_hw_get_inst_at_raw_pos(cHardwareBase* hw, int raw_pos);
int avd_hw_get_thread_id(cHardwareBase* hw);
int avd_hw_get_cycle_counter(cHardwareBase* hw);
void avd_hw_advance_head(cHardwareBase* hw, int head_id);
void avd_hw_set_cur_head(cHardwareBase* hw, int head_id);
void avd_hw_read_label(cHardwareBase* hw);
int avd_hw_get_label_as_int(cHardwareBase* hw, int mode);
int avd_hw_if_label_match(cHardwareBase* hw);
int avd_hw_if_label_direct_match(cHardwareBase* hw);
int avd_hw_search_label(cHardwareBase* hw, int direction);
int avd_hw_search_label_direct(cHardwareBase* hw, int direction);
int avd_hw_is_next_inst_nop(cHardwareBase* hw);
int avd_hw_get_label_size(cHardwareBase* hw);

// ---- CPU instruction handlers (Rust-backed) ----
// Stack operations
void avd_cpu_inst_push(cHardwareBase* hw, const CpuRegisters* regs, int reg_id);
void avd_cpu_inst_pop(cHardwareBase* hw, CpuRegisters* regs, int reg_id);
void avd_cpu_inst_switch_stack(cHardwareBase* hw);
void avd_cpu_inst_flip_stack(cHardwareBase* hw);
void avd_cpu_inst_head_pop(cHardwareBase* hw, int head_id);
// Flow control
int avd_cpu_inst_mov_head(cHardwareBase* hw, int head_id);
int avd_cpu_inst_jmp_head(cHardwareBase* hw, const CpuRegisters* regs, int head_id, int reg_id);
void avd_cpu_inst_get_head(cHardwareBase* hw, CpuRegisters* regs, int head_id, int reg_id);
void avd_cpu_inst_set_flow(cHardwareBase* hw, const CpuRegisters* regs, int reg_id);
// Conditional evaluators: return 1 if IP should advance (skip next), 0 otherwise.
int avd_cpu_inst_if_n_equ(const CpuRegisters* regs, int op1, int op2);
int avd_cpu_inst_if_a_not_eq_b(const CpuRegisters* regs);
int avd_cpu_inst_if_b_not_eq_c(const CpuRegisters* regs);
int avd_cpu_inst_if_a_not_eq_c(const CpuRegisters* regs);
int avd_cpu_inst_if_bit1(const CpuRegisters* regs, int op);
int avd_cpu_inst_if_consensus(const CpuRegisters* regs, int op);
int avd_cpu_inst_if_consensus24(const CpuRegisters* regs, int op);
int avd_cpu_inst_if_less_consensus(const CpuRegisters* regs, int op1, int op2);
int avd_cpu_inst_if_less_consensus24(const CpuRegisters* regs, int op1, int op2);
// Hardware-calling handlers
void avd_cpu_inst_return(cHardwareBase* hw);
void avd_cpu_inst_skip(cHardwareBase* hw);
void avd_cpu_inst_advance_head(cHardwareBase* hw, int head_id);
void avd_cpu_inst_set_head(cHardwareBase* hw, int head_id);
void avd_cpu_inst_mem_size(cHardwareBase* hw, int reg_id);
void avd_cpu_inst_thread_id(cHardwareBase* hw, int reg_id);
void avd_cpu_inst_get_cycles(cHardwareBase* hw, int reg_id);
int avd_cpu_inst_if_gr_equ(const CpuRegisters* regs, int op1, int op2);
int avd_cpu_inst_if_p(cAvidaContext* ctx, double skip_prob);
void avd_cpu_inst_set_num(cHardwareBase* hw, int reg_id);
void avd_cpu_inst_val_label(cHardwareBase* hw, int reg_id, int mode);
int avd_cpu_inst_if_label(cHardwareBase* hw);
int avd_cpu_inst_if_label_direct(cHardwareBase* hw);
void avd_cpu_inst_search_f(cHardwareBase* hw, CpuRegisters* regs);
void avd_cpu_inst_search_b(cHardwareBase* hw, CpuRegisters* regs);
void avd_cpu_inst_head_search(cHardwareBase* hw, CpuRegisters* regs);
void avd_cpu_inst_head_search_direct(cHardwareBase* hw, CpuRegisters* regs);
int avd_cpu_inst_head_push(cHardwareBase* hw, int head_id);
int avd_cpu_inst_if_label2(cHardwareBase* hw);
void avd_cpu_inst_transposon(cHardwareBase* hw);
void avd_cpu_inst_read_inst(cHardwareBase* hw, CpuRegisters* regs, int dst, int src);
void avd_cpu_inst_stack_read_inst(cHardwareBase* hw, const CpuRegisters* regs, int src);
int avd_cpu_inst_if_0(const CpuRegisters* regs, int op);
int avd_cpu_inst_if_not_0(const CpuRegisters* regs, int op);
int avd_cpu_inst_if_equ(const CpuRegisters* regs, int op1, int op2);
// Register operations
void avd_cpu_reg_mask(CpuRegisters* regs, int reg_id, int mask);
void avd_cpu_reg_nor(CpuRegisters* regs, int dst, int op1, int op2);
// BitConsensus value producers
void avd_cpu_inst_bit_consensus(CpuRegisters* regs, int dst, int src);
void avd_cpu_inst_bit_consensus24(CpuRegisters* regs, int dst, int src);
int avd_cpu_inst_if_less(const CpuRegisters* regs, int op1, int op2);
int avd_cpu_inst_if_grtr(const CpuRegisters* regs, int op1, int op2);
int avd_cpu_inst_if_less_equ(const CpuRegisters* regs, int op1, int op2);
int avd_cpu_inst_if_gr0(const CpuRegisters* regs, int op);
int avd_cpu_inst_if_gr_equ0(const CpuRegisters* regs, int op);
int avd_cpu_inst_if_less0(const CpuRegisters* regs, int op);
int avd_cpu_inst_if_ls_equ0(const CpuRegisters* regs, int op);
// Mutable organism handlers (Phase 2)
void avd_cpu_inst_sterilize(cHardwareBase* hw);
void avd_cpu_inst_phero_on(cHardwareBase* hw);
void avd_cpu_inst_phero_off(cHardwareBase* hw);
void avd_cpu_inst_request_energy_flag_on(cHardwareBase* hw);
void avd_cpu_inst_request_energy_flag_off(cHardwareBase* hw);
void avd_cpu_inst_increase_energy_donation(cHardwareBase* hw, double increment);
void avd_cpu_inst_decrease_energy_donation(cHardwareBase* hw, double increment);
void avd_cpu_inst_zero_easterly(cHardwareBase* hw);
void avd_cpu_inst_zero_northerly(cHardwareBase* hw);
void avd_cpu_inst_display_lyse(cHardwareBase* hw);
void avd_cpu_inst_set_mate_preference(cHardwareBase* hw, int pref);
void avd_cpu_inst_set_mating_display_a(cHardwareBase* hw, int reg_id);
void avd_cpu_inst_set_mating_display_b(cHardwareBase* hw, int reg_id);
void avd_cpu_inst_increment_mating_display_a(cHardwareBase* hw);
void avd_cpu_inst_increment_mating_display_b(cHardwareBase* hw);
void avd_cpu_inst_mark_cell_with_id(cHardwareBase* hw);
void avd_cpu_inst_mark_cell_with_vitality(cHardwareBase* hw);
void avd_cpu_inst_set_copy_mut(cHardwareBase* hw, int reg_id);
void avd_cpu_inst_io_buf_add(cHardwareBase* hw, int val);
int avd_cpu_inst_if_faced_energy(cHardwareBase* hw, int expected_level, int negate);
int avd_cpu_inst_get_faced_energy_level(cHardwareBase* hw, int reg_id);
void avd_cpu_inst_mod_copy_mut(cHardwareBase* hw, int reg_id);
void avd_cpu_inst_join_germline(cHardwareBase* hw);
void avd_cpu_inst_exit_germline(cHardwareBase* hw);
void avd_cpu_inst_phero_toggle(cHardwareBase* hw);
void avd_cpu_inst_repair_point_mut_on(cHardwareBase* hw);
void avd_cpu_inst_get_res_stored(cHardwareBase* hw, int reg_id, int resource_id);
// Organism-reading handlers
int avd_cpu_inst_if_germ(cHardwareBase* hw);
int avd_cpu_inst_if_soma(cHardwareBase* hw);
void avd_cpu_inst_get_id(cHardwareBase* hw, int reg_id);
int avd_cpu_inst_if_fertile(cHardwareBase* hw);
int avd_cpu_inst_if_not_fertile(cHardwareBase* hw);
void avd_cpu_inst_get_cell_id(cHardwareBase* hw, int reg_id);
void avd_cpu_inst_get_energy_level(cHardwareBase* hw, int reg_id);
void avd_cpu_inst_get_reputation(cHardwareBase* hw, int reg_id);
void avd_cpu_inst_get_direction_off_north(cHardwareBase* hw, int reg_id);
void avd_cpu_inst_get_northerly(cHardwareBase* hw, int reg_id);
void avd_cpu_inst_get_easterly(cHardwareBase* hw, int reg_id);
void avd_cpu_inst_read_cell_data(cHardwareBase* hw, int reg_id);
void avd_cpu_inst_read_faced_cell_data_org_id(cHardwareBase* hw, int reg_id);
void avd_cpu_inst_get_raw_materials(cHardwareBase* hw, int reg_id, int material_type);
void avd_cpu_inst_get_cell_position_y(cHardwareBase* hw, int reg_id);
int avd_cpu_inst_if_opinion_set(cHardwareBase* hw);
int avd_cpu_inst_if_opinion_not_set(cHardwareBase* hw);
int avd_cpu_inst_if_event_in_cell(cHardwareBase* hw);
int avd_cpu_inst_if_facing_event_cell(cHardwareBase* hw);
int avd_cpu_inst_if_mating_type_female(cHardwareBase* hw);
int avd_cpu_inst_if_mating_type_male(cHardwareBase* hw);
int avd_cpu_inst_if_mating_type_juvenile(cHardwareBase* hw);
int avd_cpu_inst_if_energy_not_in_buffer(cHardwareBase* hw);
int avd_cpu_inst_nop_pre(cHardwareBase* hw);
int avd_cpu_inst_nop_post(cHardwareBase* hw);
// Neighbor-reading handlers (return 0 on failure, 1 on success)
int avd_cpu_inst_get_faced_org_id(cHardwareBase* hw, int reg_id);
int avd_cpu_inst_get_faced_vitality_diff(cHardwareBase* hw, int reg_id);
// Energy conditionals
int avd_cpu_inst_if_energy_low(cHardwareBase* hw);
int avd_cpu_inst_if_energy_not_low(cHardwareBase* hw);
int avd_cpu_inst_if_energy_high(cHardwareBase* hw);
int avd_cpu_inst_if_energy_not_high(cHardwareBase* hw);
int avd_cpu_inst_if_energy_med(cHardwareBase* hw);
int avd_cpu_inst_get_energy_level_guarded(cHardwareBase* hw, int reg_id);
int avd_cpu_inst_get_cell_position_x(cHardwareBase* hw, int reg_id);
int avd_cpu_inst_get_cell_position_y_guarded(cHardwareBase* hw, int reg_id);
void avd_cpu_inst_get_opinion_only(cHardwareBase* hw, int reg_id);
// I/O organism FFI (Phase 3: DoOutput/DoInput/GetNextInput)
void avd_org_do_output(cOrganism* org, cAvidaContext* ctx, int value);
int avd_org_get_next_input(cOrganism* org);
void avd_org_do_input(cOrganism* org, int value);
// I/O CPU instruction handlers (Phase 3)
void avd_cpu_inst_task_io(cHardwareBase* hw, cAvidaContext* ctx, CpuRegisters* regs, int reg_id);
void avd_cpu_inst_task_input(cHardwareBase* hw, CpuRegisters* regs, int reg_id);
void avd_cpu_inst_task_output(cHardwareBase* hw, cAvidaContext* ctx, CpuRegisters* regs, int reg_id);
void avd_cpu_inst_task_stack_get(cHardwareBase* hw);
void avd_cpu_inst_task_stack_load(cHardwareBase* hw);
// Organism state transitions (Phase 4: I/O reset, death)
void avd_org_reset_inputs(cOrganism* org, cAvidaContext* ctx);
void avd_org_clear_input(cOrganism* org);
void avd_org_die(cOrganism* org, cAvidaContext* ctx);
int avd_org_get_cpu_cycles_used(cOrganism* org);
// I/O variant instruction handlers (Phase 4)
void avd_cpu_inst_task_io_bonus_cost(cHardwareBase* hw, cAvidaContext* ctx, CpuRegisters* regs, int reg_id, double bonus_cost);
void avd_cpu_inst_task_io_feedback(cHardwareBase* hw, cAvidaContext* ctx, CpuRegisters* regs, int reg_id);
void avd_cpu_inst_task_put_reset_inputs(cHardwareBase* hw, cAvidaContext* ctx, CpuRegisters* regs, int reg_id);
void avd_cpu_inst_task_get2(cHardwareBase* hw, cAvidaContext* ctx, CpuRegisters* regs, int reg_id);
void avd_cpu_inst_die(cHardwareBase* hw, cAvidaContext* ctx);
void avd_cpu_inst_prob_die(cHardwareBase* hw, cAvidaContext* ctx, CpuRegisters* regs, int reg_id, double config_prob);

/* ---------- Task_MatchStr FFI ---------- */
double avd_task_eval_match_str(
    const int* output_buf, int output_len,
    const int* received_buf, int received_len,
    const unsigned char* pattern, int pattern_len,
    int partial, int binary);

/* ---------- Task_Optimize FFI ---------- */
typedef struct OptimizeResult {
    double quality;
    double fx;
} OptimizeResult;

OptimizeResult avd_task_eval_optimize(
    const int* output_buf, int output_len, int output_capacity,
    int function, int binary, int varlength, int numvars,
    double basepow, double max_fx, double min_fx,
    double thresh, double thresh_max,
    const unsigned char* string_to_match, int string_len);

/* ---------- Task_SortInputs FFI ---------- */
double avd_task_eval_sort_inputs(
    const int* output_buf, int output_len,
    const int* input_buf, int input_len,
    int size, int direction, int contiguous, double halflife);
// Faced energy conditionals
int avd_cpu_inst_if_faced_energy_low(cHardwareBase* hw);
int avd_cpu_inst_if_faced_energy_not_low(cHardwareBase* hw);
int avd_cpu_inst_if_faced_energy_high(cHardwareBase* hw);
int avd_cpu_inst_if_faced_energy_not_high(cHardwareBase* hw);
int avd_cpu_inst_if_faced_energy_med(cHardwareBase* hw);
int avd_cpu_inst_if_faced_energy_less(cHardwareBase* hw, double epsilon);
int avd_cpu_inst_if_faced_energy_more(cHardwareBase* hw, double epsilon);
// Faced energy request handlers
int avd_cpu_inst_if_faced_energy_request_on(cHardwareBase* hw);
int avd_cpu_inst_if_faced_energy_request_off(cHardwareBase* hw);
void avd_cpu_inst_get_energy_request_status(cHardwareBase* hw, int reg_id);
int avd_cpu_inst_get_faced_energy_request_status(cHardwareBase* hw, int reg_id);
// avd_cpu_inst_get_faced_energy_level declared above (returns int)
// Energy in buffer
int avd_cpu_inst_if_energy_in_buffer(cHardwareBase* hw);
// World-reading handlers
void avd_cpu_inst_get_update(cHardwareBase* hw, int reg_id, int update_val);
// GetTimeUsed
void avd_cpu_inst_get_time_used(cHardwareBase* hw, int reg_id);
// GetCellPosition (x+y into two registers)
void avd_cpu_inst_get_cell_position(cHardwareBase* hw, int xreg, int yreg);
// Poison
void avd_cpu_inst_poison(cHardwareBase* hw, double penalty);
// Pose (reputation increment)
void avd_cpu_inst_pose(cHardwareBase* hw);
// GetNeighborsReputation
void avd_cpu_inst_get_neighbors_reputation(cHardwareBase* hw, int reg_id);
// RepairPointMutOff
void avd_cpu_inst_repair_point_mut_off(cHardwareBase* hw);
// Flash info handlers
int avd_cpu_inst_if_recvd_flash(cHardwareBase* hw);
void avd_cpu_inst_flash_info(cHardwareBase* hw, int bx, int cx);
void avd_cpu_inst_flash_info_b(cHardwareBase* hw, int bx);
void avd_cpu_inst_reset_flash_info(cHardwareBase* hw);
// Fixed-register stack ops
void avd_cpu_inst_pop_fixed(cHardwareBase* hw, int reg_id);
void avd_cpu_inst_push_fixed(cHardwareBase* hw, int reg_id);
// IfDonor
int avd_cpu_inst_if_donor(cHardwareBase* hw);
// ReadFacedCellData (vitality diff)
void avd_cpu_inst_read_faced_cell_data(cHardwareBase* hw, int reg_id);
// ReadFacedCellDataFreshness
void avd_cpu_inst_read_faced_cell_data_freshness(cHardwareBase* hw, int reg_id, int current_update);
// GetDistanceFromDiagonal
void avd_cpu_inst_get_distance_from_diagonal(cHardwareBase* hw, int reg_id, int pos_x, int pos_y);

// Batch D6–D8: Opinion, Send, Donation handlers
void avd_cpu_inst_set_opinion(cHardwareBase* hw, int reg_id);
void avd_cpu_inst_clear_opinion(cHardwareBase* hw);
int  avd_cpu_inst_get_opinion(cHardwareBase* hw, int opinion_reg, int age_reg, int current_update);
void avd_cpu_inst_send(cHardwareBase* hw, int reg_id);
int  avd_cpu_inst_donate_null(cHardwareBase* hw, cAvidaContext* ctx, int max_donates, double merit_given);
int  avd_cpu_inst_donate_facing(cHardwareBase* hw, cAvidaContext* ctx, int max_donates);
int  avd_cpu_inst_donate_energy_faced(cHardwareBase* hw, cAvidaContext* ctx, double loss_pct, int update_metabolic, int sharing_method);
int  avd_cpu_inst_donate_energy_faced_amount(cHardwareBase* hw, cAvidaContext* ctx, double fixed_amount, double loss_pct, int update_metabolic, int sharing_method);
int  avd_cpu_inst_receive_donated_energy(cHardwareBase* hw, cAvidaContext* ctx, int update_metabolic);
void avd_cpu_inst_update_metabolic_rate(cHardwareBase* hw, cAvidaContext* ctx);

// Phase 5: Kaboom/Kazi organism FFI
void avd_org_kaboom(cOrganism* org, cAvidaContext* ctx, int distance);
// Phase 5: Stats wrappers (Lyse counters — take cWorld* opaque pointer)
void avd_stats_inc_kaboom(cWorld* world);
void avd_stats_inc_dont_explode(cWorld* world);
void avd_stats_inc_perc_lyse(cWorld* world, double perc);
void avd_stats_inc_sum_cpus(cWorld* world, int cpu_cycles);
// Phase 5: Messaging FFI
int avd_org_send_message_regs(cOrganism* org, cAvidaContext* ctx, int label, int data, int msg_type);
int avd_org_retrieve_message(cOrganism* org, int* out_label, int* out_data, int log_enabled, cWorld* world);
int avd_org_receive_value(cOrganism* org);
// Phase 5: Kazi/Lyse/SmartExplode instruction handlers
// kazi_generic: handles Kazi, Kazi1-5. C++ passes config values + reg_value.
void avd_cpu_inst_kazi_generic(
    cHardwareBase* hw, cAvidaContext* ctx, CpuRegisters* regs, int reg_id,
    double kaboom_prob, int kaboom_hamming, int max_genome_size);
// lyse: C++ passes config + world ptr for stats.
void avd_cpu_inst_lyse(
    cHardwareBase* hw, cAvidaContext* ctx, CpuRegisters* regs, int reg_id,
    double kaboom_prob, cWorld* world);
// smart_explode: C++ passes config + world ptr for stats.
void avd_cpu_inst_smart_explode(
    cHardwareBase* hw, cAvidaContext* ctx, CpuRegisters* regs, int reg_id,
    double kaboom_prob, int kaboom_hamming, cWorld* world);
// Phase 5: Message instruction handlers
int avd_cpu_inst_send_message(
    cHardwareBase* hw, cAvidaContext* ctx, CpuRegisters* regs, int reg_id, int msg_type);
int avd_cpu_inst_retrieve_message(
    cHardwareBase* hw, CpuRegisters* regs, int log_enabled, cWorld* world);
// Phase 5: Receive (value, not message)
void avd_cpu_inst_receive(cHardwareBase* hw, CpuRegisters* regs, int reg_id);
// Batch D9: Cell data, group, opinion-to-value, donate-res-to-deme
void avd_cpu_inst_collect_cell_data(cHardwareBase* hw, int reg_id);
void avd_cpu_inst_set_opinion_to_value(cHardwareBase* hw, cAvidaContext* ctx, int value);
void avd_cpu_inst_number_orgs_in_my_group(cHardwareBase* hw, int reg_id);
void avd_cpu_inst_number_orgs_in_group(cHardwareBase* hw, int group_reg, int result_reg);
void avd_cpu_inst_donate_res_to_deme(cHardwareBase* hw);
// Movement FFI (Phase 5)
int avd_org_move(cOrganism* org, cAvidaContext* ctx);
void avd_org_rotate(cOrganism* org, cAvidaContext* ctx, int direction);
int avd_org_get_neighborhood_size(cOrganism* org);
int avd_org_get_facing(cOrganism* org);

// Movement instruction handlers (Rust-backed)
void avd_cpu_inst_rotate_left_one(cHardwareBase* hw, cAvidaContext* ctx);
void avd_cpu_inst_rotate_right_one(cHardwareBase* hw, cAvidaContext* ctx);
void avd_cpu_inst_rotate_unoccupied_cell(cHardwareBase* hw, cAvidaContext* ctx, int reg_used);
void avd_cpu_inst_rotate_occupied_cell(cHardwareBase* hw, cAvidaContext* ctx, int reg_used);
void avd_cpu_inst_rotate_next_occupied_cell(cHardwareBase* hw, cAvidaContext* ctx, int reg_used);
void avd_cpu_inst_rotate_next_unoccupied_cell(cHardwareBase* hw, cAvidaContext* ctx, int reg_used);
void avd_cpu_inst_rotate_home(cHardwareBase* hw, cAvidaContext* ctx);
void avd_cpu_inst_rotate_event_cell(cHardwareBase* hw, cAvidaContext* ctx, int reg_used);
int  avd_cpu_inst_move(cHardwareBase* hw, cAvidaContext* ctx, int reg_used);

// Trivial/Reset/SA handlers (Rust-backed)
int  avd_cpu_inst_nop_true(void);
void avd_cpu_inst_reset(cHardwareBase* hw, CpuRegisters* regs);
void avd_cpu_inst_aggressive_sa(cHardwareBase* hw, cAvidaContext* ctx, double kaboom_prob, int kaboom_hamming, double kaboom_effect);
void avd_cpu_inst_cooperative_sa(cHardwareBase* hw, cAvidaContext* ctx, double kaboom_prob, int kaboom_hamming, double kaboom_effect);
void avd_cpu_inst_set_head_modified(cHardwareBase* hw);
void avd_cpu_inst_advance_head_modified(cHardwareBase* hw);
int  avd_cpu_inst_if_label_via_ffi(cHardwareBase* hw);
int  avd_cpu_inst_if_label_direct_via_ffi(cHardwareBase* hw);
void avd_cpu_inst_receive_modified(cHardwareBase* hw, CpuRegisters* regs);
int  avd_cpu_inst_send_message_modified(cHardwareBase* hw, cAvidaContext* ctx, const CpuRegisters* regs, int msg_type);
// Organism FFI: Kaboom with effect factor
void avd_org_kaboom_with_effect(cOrganism* org, cAvidaContext* ctx, int distance, double effect);
// Organism FFI: Flash / Neighborhood / Broadcast
void avd_org_send_flash(cOrganism* org, cAvidaContext* ctx);
void avd_org_load_neighborhood(cOrganism* org, cAvidaContext* ctx);
int  avd_org_has_neighborhood_changed(cOrganism* org, cAvidaContext* ctx);
int  avd_org_broadcast_message(cOrganism* org, cAvidaContext* ctx, int label, int data, int depth);
// Rust-backed: Flash / Neighborhood / Broadcast instruction handlers
void avd_cpu_inst_flash(cHardwareBase* hw, cAvidaContext* ctx);
void avd_cpu_inst_get_neighborhood(cHardwareBase* hw, cAvidaContext* ctx);
int  avd_cpu_inst_if_neighborhood_changed(cHardwareBase* hw, cAvidaContext* ctx);
int  avd_cpu_inst_broadcast_x(cHardwareBase* hw, cAvidaContext* ctx, const CpuRegisters* regs, int reg_id, int depth);
void avd_cpu_inst_tumble(cHardwareBase* hw, cAvidaContext* ctx);
int  avd_cpu_inst_if_neighbor_event_in_unoccupied(cHardwareBase* hw, cAvidaContext* ctx);
// Organism FFI: BcastAlarmMSG
int  avd_org_bcast_alarm_msg(cOrganism* org, cAvidaContext* ctx, int jump_label, int bcast_range);
// Rust-backed: Alarm MSG generic handler
int  avd_cpu_inst_alarm_msg_generic(cHardwareBase* hw, cAvidaContext* ctx, const CpuRegisters* regs, int reg_id, int use_bit_consensus, int bcast_range);
int  avd_cpu_inst_get_attack_odds(cHardwareBase* hw, int reg_id);
// -- Task_MatchProdStr helpers --
// Returns packed i64: (max_num_matched * 1000 + num_real).
// binary==0: bit-mode comparison of test_output; binary!=0: element-wise.
int64_t avd_task_match_prod_str_core(
    const unsigned char* string_ptr, int string_len,
    const int* output_buf, int output_len,
    int binary);

// Compute bonus from match results.
double avd_task_match_prod_str_bonus(
    int max_num_matched, int string_len, int num_real,
    int partial, double mypow);

// -- Task_SGPathTraversal helper --
// Compute traversal quality from history and state grid.
double avd_task_sg_path_traversal_quality(
    const int* history_ptr, int history_len,
    const int* grid_states_ptr, int grid_len,
    int poison_state, int poison_touch_count,
    int target_count);

// -- BirthChamber merit blending --
typedef struct {
    double merit0;
    double merit1;
    int swap;
} MeritBlendResult;

MeritBlendResult avd_birth_blend_merits(
    double merit0, double merit1,
    double cut_frac, double stay_frac);

typedef struct {
    int start0;
    int end0;
    int start1;
    int end1;
    double cut_frac;
    double stay_frac;
} RecombRegion;

RecombRegion avd_birth_recomb_region(
    double frac_a, double frac_b,
    int genome0_size, int genome1_size);

// Resource sensing + cell data FFI
int avd_org_sense_resource_x(cOrganism* org, cAvidaContext* ctx, int cell_id, int res_id);
int avd_org_get_faced_cell_id(cOrganism* org);
int avd_hw_get_last_cell_data_valid(cHardwareBase* hw);
int avd_hw_get_last_cell_data_value(cHardwareBase* hw);
void avd_cpu_inst_sense_resource_x(cHardwareBase* hw, cAvidaContext* ctx, int reg_id, int res_id, int use_faced);
int avd_cpu_inst_if_cell_data_changed(cHardwareBase* hw);

// Memory-edit / copy-mutation hardware FFI (cHardwareFFI.cc)
void avd_hw_memory_set_flags(cHardwareBase* hw, int idx, int mask);
void avd_hw_memory_clear_flags(cHardwareBase* hw, int idx, int mask);
void avd_hw_memory_insert(cHardwareBase* hw, int idx, int inst_id);
void avd_hw_memory_remove(cHardwareBase* hw, int idx);
int avd_hw_get_nop_mod(cHardwareBase* hw, int inst_id);
int avd_hw_is_nop(cHardwareBase* hw, int inst_id);
int avd_hw_get_random_inst(cHardwareBase* hw, cAvidaContext* ctx);
int avd_hw_check_no_mut_list(cHardwareBase* hw, int idx);
int avd_hw_get_label_nop(cHardwareBase* hw, int idx);
int avd_hw_get_cur_head(cHardwareBase* hw);
void avd_hw_read_inst(cHardwareBase* hw, int inst_id);
void avd_hw_set_advance_ip(cHardwareBase* hw, int advance);
int avd_hw_get_slip_read_head(cHardwareBase* hw);
int avd_hw_fork_thread(cHardwareBase* hw);
int avd_hw_kill_thread(cHardwareBase* hw);
void avd_hw_fault(cHardwareBase* hw, int fault_loc, int fault_type, const char* desc);
int avd_org_test_copy(cOrganism* org, cAvidaContext* ctx, int kind);
double avd_org_get_copy_prob(cOrganism* org, int kind);
void avd_hw_do_uniform_copy_mutation(cHardwareBase* hw, cAvidaContext* ctx, int head_id);
void avd_hw_do_slip_mutation(cHardwareBase* hw, cAvidaContext* ctx, int pos);
int avd_hw_allocate_main(cHardwareBase* hw, cAvidaContext* ctx, int size);
int avd_hw_divide_main(cHardwareBase* hw, cAvidaContext* ctx, int div_point, int extra_lines,
                       double mut_multiplier, int kind);
void avd_hw_adjust_heads(cHardwareBase* hw);
int avd_hw_adjust_head(cHardwareBase* hw, int head_id);
int avd_hw_find_inst_op(cHardwareBase* hw, const char* name);
void avd_org_set_divide_sex(cOrganism* org, int divide_sex);
void avd_org_set_cross_num(cOrganism* org, int cross_num);
void avd_org_set_mate_select_id(cOrganism* org, int select_id);
int avd_org_born_parent_group(cOrganism* org);

// Copy / head / jump / thread / alloc handlers implemented in Rust (cpu_memory_instructions.rs)
int avd_cpu_inst_if_x(cHardwareBase* hw, int greater);
int avd_cpu_inst_jump_label(cHardwareBase* hw, int direction);
int avd_cpu_inst_call(cHardwareBase* hw);
// cond: 0 = always, 1 = only if BX != 0, 2 = only if BX == 0.
int avd_cpu_inst_throw(cHardwareBase* hw, int catch_op, int cond);
int avd_cpu_inst_goto(cHardwareBase* hw, int label_op, int cond);
void avd_cpu_inst_copy(cHardwareBase* hw, cAvidaContext* ctx);
void avd_cpu_inst_write_inst(cHardwareBase* hw, cAvidaContext* ctx, int src_reg);
void avd_cpu_inst_stack_write_inst(cHardwareBase* hw, cAvidaContext* ctx, int dst_reg);
void avd_cpu_inst_compare(cHardwareBase* hw, cAvidaContext* ctx, int dst_reg);
int avd_cpu_inst_if_n_cpy(cHardwareBase* hw, cAvidaContext* ctx);
void avd_cpu_inst_head_read(cHardwareBase* hw, cAvidaContext* ctx);
void avd_cpu_inst_head_write(cHardwareBase* hw, cAvidaContext* ctx);
void avd_cpu_inst_head_copy(cHardwareBase* hw, cAvidaContext* ctx, double reduction);
void avd_cpu_inst_head_copy_if_resource(cHardwareBase* hw, cAvidaContext* ctx);
void avd_cpu_inst_fork_thread(cHardwareBase* hw);
int avd_cpu_inst_fork_thread_label(cHardwareBase* hw, int cond);
void avd_cpu_inst_kill_thread(cHardwareBase* hw);
int avd_cpu_inst_allocate(cHardwareBase* hw, cAvidaContext* ctx);
int avd_cpu_inst_max_alloc(cHardwareBase* hw, cAvidaContext* ctx, double offspring_size_range,
                           int max_genome_length, int move_write_head);
int avd_cpu_inst_divide(cHardwareBase* hw, cAvidaContext* ctx, int mode);
int avd_cpu_inst_head_divide(cHardwareBase* hw, cAvidaContext* ctx, int kind, double mut_multiplier,
                             int tolerance_window);
int avd_cpu_inst_head_divide_sex(cHardwareBase* hw, cAvidaContext* ctx, int mode, int tolerance_window);
void avd_cpu_inst_if_end(cHardwareBase* hw, int cmp, int x_reg, int y_reg);
void avd_cpu_inst_else(cHardwareBase* hw);

#ifdef __cplusplus
}

// C++-only helper: populate a ConfigSnapshot from cAvidaConfig.
class cAvidaConfig;
void avd_populate_config_snapshot(ConfigSnapshot* snap, const cAvidaConfig& cfg);

// C++-only helper: populate a TaskContextSnapshot from cTaskContext.
class cTaskContext;
void avd_populate_task_context(TaskContextSnapshot* snap, cTaskContext& ctx);
#endif

#endif
