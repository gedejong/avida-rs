//! Registration data and task identity for `cTaskLib::AddTask`.
//!
//! GENERATED from the `NewTask(...)` calls in `cTaskLib::AddTask`
//! (avida-core/source/main/cTaskLib.cc) and the `Logic3in` target ids; do not
//! edit by hand. The `Load_*` families (tasks with arguments) are registered by
//! hand in `task_lib.rs`.

use crate::eval_logic::LogicOp;

/// Which evaluator a task entry runs: the Rust replacement for the C++
/// `tTaskTest` member-function pointer. Several registered names share an
/// evaluator (`echo` and `echo_dup`, ...), exactly as in C++.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskKind {
    /// `Task_Echo`
    Echo,
    /// `Task_Add`
    Add,
    /// `Task_Add3`
    Add3,
    /// `Task_Sub`
    Sub,
    /// `Task_DontCare`
    DontCare,
    /// `Task_Not` ... `Task_Equ`: a boolean logic function by logic id.
    Logic(LogicOp),
    /// `Task_Nand_ResourceDependent`
    NandResDep,
    /// `Task_Nor_ResourceDependent`
    NorResDep,
    /// `Task_XorMax`
    XorMax,
    /// `Task_Logic3in_*`: matches when the logic id is any of the targets.
    Logic3(&'static [i32]),
    /// `Task_Math1in_*`
    Math1(Math1Op),
    /// `Task_Math2in_*`
    Math2(Math2Op),
    /// `Task_Math3in_*`
    Math3(Math3Op),
    /// `Task_Fib1` .. `Task_Fib10`: the Fibonacci number at 1-based index
    /// (1, 2, 4, 5, ... 10; there is no `Task_Fib3`).
    Fib(u8),
    /// `Task_MatchStr`
    MatchStr,
    /// `Task_MatchProdStr`. QUIRK: `Load_MatchProdStr` registers
    /// `Task_MatchStr`, so this kind is never produced by registration; the
    /// evaluator is ported for completeness.
    MatchProdStr,
    /// `Task_MatchNumber`
    MatchNumber,
    /// `Task_SortInputs`
    SortInputs,
    /// `Task_FibonacciSequence`
    FibonacciSequence,
    /// `Task_Mult`
    Mult,
    /// `Task_Div`
    Div,
    /// `Task_Log`, `Task_Log2`, `Task_Log10`, `Task_Sqrt`, `Task_Sine`, `Task_Cosine`
    Unary(UnaryOp),
    /// `Task_Optimize`
    Optimize,
    /// `Task_CommEcho`
    CommEcho,
    /// `Task_CommNot`
    CommNot,
    /// `Task_MoveUpGradient`
    MoveUpGradient,
    /// `Task_MoveNeutralGradient`
    MoveNeutralGradient,
    /// `Task_MoveDownGradient`
    MoveDownGradient,
    /// `Task_MoveNotUpGradient`
    MoveNotUpGradient,
    /// `Task_MoveToRightSide`
    MoveToRightSide,
    /// `Task_MoveToLeftSide`
    MoveToLeftSide,
    /// `Task_Move`
    Move,
    /// `Task_MoveToTarget`
    MoveToTarget,
    /// `Task_MoveToMovementEvent`
    MoveToMovementEvent,
    /// `Task_MoveBetweenMovementEvent`
    MoveBetweenMovementEvent,
    /// `Task_MoveToEvent`
    MoveToEvent,
    /// `Task_EventKilled`
    EventKilled,
    /// `Task_SGPathTraversal`
    SGPathTraversal,
    /// `Task_CreatePerfectStrings`
    CreatePerfectStrings,
    /// `Task_FormSpatialGroup`
    FormSpatialGroup,
    /// `Task_FormSpatialGroupWithID`
    FormSpatialGroupWithID,
    /// `Task_LiveOnPatchRes`
    LiveOnPatchRes,
    /// `Task_CollectOddCell`
    CollectOddCell,
    /// `Task_ConsumeTarget`
    ConsumeTarget,
    /// `Task_ConsumeTargetEcho`
    ConsumeTargetEcho,
    /// `Task_ConsumeTargetNand` ... `Task_ConsumeTargetEqu`
    ConsumeTargetLogic(LogicOp),
    /// `Task_MoveFT`
    MoveFT,
    /// `Task_Exploded`
    Exploded,
    /// `Task_Exploded2`
    Exploded2,
    /// `Task_ConsumePublicGood`
    ConsumePublicGood,
    /// `Task_AIDisplayCost`
    AIDisplayCost,
    /// `Task_ProducePublicGood`
    ProducePublicGood,
    /// `Task_AllOnes`
    AllOnes,
    /// `Task_RoyalRoad`
    RoyalRoad,
    /// `Task_RoyalRoadWithDitches`
    RoyalRoadWithDitches,
    /// `Task_OpinionIs`
    OpinionIs,
}

/// The unary math tasks that share `avd_tasklib_unary_math_input_diff` logic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    /// `Task_Log`
    Log,
    /// `Task_Log2`
    Log2,
    /// `Task_Log10`
    Log10,
    /// `Task_Sqrt`
    Sqrt,
    /// `Task_Sine`
    Sine,
    /// `Task_Cosine`
    Cosine,
}

/// Operation selector for the `Task_Math1in_*` family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Math1Op {
    /// `Task_Math1in_AA`
    AA,
    /// `Task_Math1in_AB`
    AB,
    /// `Task_Math1in_AC`
    AC,
    /// `Task_Math1in_AD`
    AD,
    /// `Task_Math1in_AE`
    AE,
    /// `Task_Math1in_AF`
    AF,
    /// `Task_Math1in_AG`
    AG,
    /// `Task_Math1in_AH`
    AH,
    /// `Task_Math1in_AI`
    AI,
    /// `Task_Math1in_AJ`
    AJ,
    /// `Task_Math1in_AK`
    AK,
    /// `Task_Math1in_AL`
    AL,
    /// `Task_Math1in_AM`
    AM,
    /// `Task_Math1in_AN`
    AN,
    /// `Task_Math1in_AO`
    AO,
    /// `Task_Math1in_AP`
    AP,
    /// `Task_Math1in_AS`
    AS,
}

/// Operation selector for the `Task_Math2in_*` family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Math2Op {
    /// `Task_Math2in_AA`
    AA,
    /// `Task_Math2in_AB`
    AB,
    /// `Task_Math2in_AC`
    AC,
    /// `Task_Math2in_AD`
    AD,
    /// `Task_Math2in_AE`
    AE,
    /// `Task_Math2in_AF`
    AF,
    /// `Task_Math2in_AG`
    AG,
    /// `Task_Math2in_AH`
    AH,
    /// `Task_Math2in_AI`
    AI,
    /// `Task_Math2in_AJ`
    AJ,
    /// `Task_Math2in_AK`
    AK,
    /// `Task_Math2in_AL`
    AL,
    /// `Task_Math2in_AM`
    AM,
    /// `Task_Math2in_AN`
    AN,
    /// `Task_Math2in_AO`
    AO,
    /// `Task_Math2in_AP`
    AP,
    /// `Task_Math2in_AQ`
    AQ,
    /// `Task_Math2in_AR`
    AR,
    /// `Task_Math2in_AS`
    AS,
    /// `Task_Math2in_AT`
    AT,
    /// `Task_Math2in_AU`
    AU,
    /// `Task_Math2in_AV`
    AV,
    /// `Task_Math2in_AX`
    AX,
    /// `Task_Math2in_AY`
    AY,
    /// `Task_Math2in_AZ`
    AZ,
    /// `Task_Math2in_AAA`
    AAA,
}

/// Operation selector for the `Task_Math3in_*` family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Math3Op {
    /// `Task_Math3in_AA`
    AA,
    /// `Task_Math3in_AB`
    AB,
    /// `Task_Math3in_AC`
    AC,
    /// `Task_Math3in_AD`
    AD,
    /// `Task_Math3in_AE`
    AE,
    /// `Task_Math3in_AF`
    AF,
    /// `Task_Math3in_AG`
    AG,
    /// `Task_Math3in_AH`
    AH,
    /// `Task_Math3in_AI`
    AI,
    /// `Task_Math3in_AJ`
    AJ,
    /// `Task_Math3in_AK`
    AK,
    /// `Task_Math3in_AL`
    AL,
    /// `Task_Math3in_AM`
    AM,
}

static LOGIC3_AA: [i32; 1] = [1];
static LOGIC3_AB: [i32; 1] = [22];
static LOGIC3_AC: [i32; 1] = [23];
static LOGIC3_AD: [i32; 1] = [104];
static LOGIC3_AE: [i32; 1] = [105];
static LOGIC3_AF: [i32; 1] = [126];
static LOGIC3_AG: [i32; 1] = [127];
static LOGIC3_AH: [i32; 1] = [128];
static LOGIC3_AI: [i32; 1] = [129];
static LOGIC3_AJ: [i32; 1] = [150];
static LOGIC3_AK: [i32; 1] = [151];
static LOGIC3_AL: [i32; 1] = [232];
static LOGIC3_AM: [i32; 1] = [233];
static LOGIC3_AN: [i32; 1] = [254];
static LOGIC3_AO: [i32; 3] = [2, 4, 16];
static LOGIC3_AP: [i32; 3] = [6, 18, 20];
static LOGIC3_AQ: [i32; 3] = [7, 19, 21];
static LOGIC3_AR: [i32; 3] = [8, 32, 64];
static LOGIC3_AS: [i32; 3] = [9, 33, 65];
static LOGIC3_AT: [i32; 3] = [14, 50, 84];
static LOGIC3_AU: [i32; 3] = [24, 36, 66];
static LOGIC3_AV: [i32; 3] = [25, 37, 67];
static LOGIC3_AW: [i32; 3] = [30, 54, 86];
static LOGIC3_AX: [i32; 3] = [31, 55, 87];
static LOGIC3_AY: [i32; 3] = [40, 72, 96];
static LOGIC3_AZ: [i32; 3] = [41, 73, 97];
static LOGIC3_BA: [i32; 3] = [42, 76, 112];
static LOGIC3_BB: [i32; 3] = [43, 77, 113];
static LOGIC3_BC: [i32; 3] = [61, 91, 103];
static LOGIC3_BD: [i32; 3] = [62, 94, 118];
static LOGIC3_BE: [i32; 3] = [106, 108, 120];
static LOGIC3_BF: [i32; 3] = [107, 109, 121];
static LOGIC3_BG: [i32; 3] = [110, 122, 124];
static LOGIC3_BH: [i32; 3] = [111, 123, 125];
static LOGIC3_BI: [i32; 3] = [130, 132, 144];
static LOGIC3_BJ: [i32; 3] = [131, 133, 145];
static LOGIC3_BK: [i32; 3] = [134, 146, 148];
static LOGIC3_BL: [i32; 3] = [135, 147, 149];
static LOGIC3_BM: [i32; 3] = [137, 161, 193];
static LOGIC3_BN: [i32; 3] = [142, 178, 212];
static LOGIC3_BO: [i32; 3] = [143, 179, 213];
static LOGIC3_BP: [i32; 3] = [152, 164, 194];
static LOGIC3_BQ: [i32; 3] = [158, 182, 214];
static LOGIC3_BR: [i32; 3] = [159, 183, 215];
static LOGIC3_BS: [i32; 3] = [168, 200, 224];
static LOGIC3_BT: [i32; 3] = [169, 201, 225];
static LOGIC3_BU: [i32; 3] = [171, 205, 241];
static LOGIC3_BV: [i32; 3] = [188, 218, 230];
static LOGIC3_BW: [i32; 3] = [189, 219, 231];
static LOGIC3_BX: [i32; 3] = [190, 222, 246];
static LOGIC3_BY: [i32; 3] = [191, 223, 247];
static LOGIC3_BZ: [i32; 3] = [234, 236, 248];
static LOGIC3_CA: [i32; 3] = [235, 237, 249];
static LOGIC3_CB: [i32; 3] = [239, 251, 253];
static LOGIC3_CC: [i32; 6] = [11, 13, 35, 49, 69, 81];
static LOGIC3_CD: [i32; 6] = [26, 28, 38, 52, 70, 82];
static LOGIC3_CE: [i32; 6] = [27, 29, 39, 53, 71, 83];
static LOGIC3_CF: [i32; 6] = [44, 56, 74, 88, 98, 100];
static LOGIC3_CG: [i32; 6] = [45, 57, 75, 89, 99, 101];
static LOGIC3_CH: [i32; 6] = [46, 58, 78, 92, 114, 116];
static LOGIC3_CI: [i32; 6] = [47, 59, 79, 93, 115, 117];
static LOGIC3_CJ: [i32; 6] = [138, 140, 162, 176, 196, 208];
static LOGIC3_CK: [i32; 6] = [139, 141, 163, 177, 197, 209];
static LOGIC3_CL: [i32; 6] = [154, 156, 166, 180, 198, 210];
static LOGIC3_CM: [i32; 6] = [155, 157, 167, 181, 199, 211];
static LOGIC3_CN: [i32; 6] = [172, 184, 202, 216, 226, 228];
static LOGIC3_CO: [i32; 6] = [173, 185, 203, 217, 227, 229];
static LOGIC3_CP: [i32; 6] = [174, 186, 206, 220, 242, 244];

/// `REQ_NEIGHBOR_INPUT`
pub(crate) const REQ_NEIGHBOR_INPUT: u32 = 1;
/// `REQ_NEIGHBOR_OUTPUT`
pub(crate) const REQ_NEIGHBOR_OUTPUT: u32 = 2;

/// One `name == "..." -> NewTask(name, desc, fn, reqs)` registration.
pub(crate) struct SimpleTask {
    pub name: &'static str,
    pub desc: &'static str,
    pub kind: TaskKind,
    pub reqs: u32,
}

/// Every task registered without arguments, in `AddTask` source order.
pub(crate) static SIMPLE_TASKS: &[SimpleTask] = &[
    SimpleTask {
        name: "echo",
        desc: "Echo",
        kind: TaskKind::Echo,
        reqs: 0,
    },
    SimpleTask {
        name: "echo_dup",
        desc: "Echo_dup",
        kind: TaskKind::Echo,
        reqs: 0,
    },
    SimpleTask {
        name: "add",
        desc: "Add",
        kind: TaskKind::Add,
        reqs: 0,
    },
    SimpleTask {
        name: "add3",
        desc: "Add3",
        kind: TaskKind::Add3,
        reqs: 0,
    },
    SimpleTask {
        name: "sub",
        desc: "Sub",
        kind: TaskKind::Sub,
        reqs: 0,
    },
    SimpleTask {
        name: "dontcare",
        desc: "DontCare",
        kind: TaskKind::DontCare,
        reqs: 0,
    },
    SimpleTask {
        name: "not",
        desc: "Not",
        kind: TaskKind::Logic(LogicOp::Not),
        reqs: 0,
    },
    SimpleTask {
        name: "not_dup",
        desc: "Not_dup",
        kind: TaskKind::Logic(LogicOp::Not),
        reqs: 0,
    },
    SimpleTask {
        name: "nand",
        desc: "Nand",
        kind: TaskKind::Logic(LogicOp::Nand),
        reqs: 0,
    },
    SimpleTask {
        name: "nand_dup",
        desc: "Nand_dup",
        kind: TaskKind::Logic(LogicOp::Nand),
        reqs: 0,
    },
    SimpleTask {
        name: "and",
        desc: "And",
        kind: TaskKind::Logic(LogicOp::And),
        reqs: 0,
    },
    SimpleTask {
        name: "and_dup",
        desc: "And_dup",
        kind: TaskKind::Logic(LogicOp::And),
        reqs: 0,
    },
    SimpleTask {
        name: "orn",
        desc: "OrNot",
        kind: TaskKind::Logic(LogicOp::OrNot),
        reqs: 0,
    },
    SimpleTask {
        name: "orn_dup",
        desc: "OrNot_dup",
        kind: TaskKind::Logic(LogicOp::OrNot),
        reqs: 0,
    },
    SimpleTask {
        name: "or",
        desc: "Or",
        kind: TaskKind::Logic(LogicOp::Or),
        reqs: 0,
    },
    SimpleTask {
        name: "or_dup",
        desc: "Or_dup",
        kind: TaskKind::Logic(LogicOp::Or),
        reqs: 0,
    },
    SimpleTask {
        name: "andn",
        desc: "AndNot",
        kind: TaskKind::Logic(LogicOp::AndNot),
        reqs: 0,
    },
    SimpleTask {
        name: "andn_dup",
        desc: "AndNot_dup",
        kind: TaskKind::Logic(LogicOp::AndNot),
        reqs: 0,
    },
    SimpleTask {
        name: "nor",
        desc: "Nor",
        kind: TaskKind::Logic(LogicOp::Nor),
        reqs: 0,
    },
    SimpleTask {
        name: "nor_dup",
        desc: "Nor_dup",
        kind: TaskKind::Logic(LogicOp::Nor),
        reqs: 0,
    },
    SimpleTask {
        name: "xor",
        desc: "Xor",
        kind: TaskKind::Logic(LogicOp::Xor),
        reqs: 0,
    },
    SimpleTask {
        name: "xor_dup",
        desc: "Xor_dup",
        kind: TaskKind::Logic(LogicOp::Xor),
        reqs: 0,
    },
    SimpleTask {
        name: "equ",
        desc: "Equals",
        kind: TaskKind::Logic(LogicOp::Equ),
        reqs: 0,
    },
    SimpleTask {
        name: "equ_dup",
        desc: "Equals_dup",
        kind: TaskKind::Logic(LogicOp::Equ),
        reqs: 0,
    },
    SimpleTask {
        name: "xor-max",
        desc: "Xor-max",
        kind: TaskKind::XorMax,
        reqs: 0,
    },
    SimpleTask {
        name: "nand-resourceDependent",
        desc: "Nand-resourceDependent",
        kind: TaskKind::NandResDep,
        reqs: 0,
    },
    SimpleTask {
        name: "nor-resourceDependent",
        desc: "Nor-resourceDependent",
        kind: TaskKind::NorResDep,
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3AA",
        desc: "Logic 3AA (A+B+C == 0)",
        kind: TaskKind::Logic3(&LOGIC3_AA),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3AB",
        desc: "Logic 3AB (A+B+C == 1)",
        kind: TaskKind::Logic3(&LOGIC3_AB),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3AC",
        desc: "Logic 3AC (A+B+C <= 1)",
        kind: TaskKind::Logic3(&LOGIC3_AC),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3AD",
        desc: "Logic 3AD (A+B+C == 2)",
        kind: TaskKind::Logic3(&LOGIC3_AD),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3AE",
        desc: "Logic 3AE (A+B+C == 0,2)",
        kind: TaskKind::Logic3(&LOGIC3_AE),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3AF",
        desc: "Logic 3AF (A+B+C == 1,2)",
        kind: TaskKind::Logic3(&LOGIC3_AF),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3AG",
        desc: "Logic 3AG (A+B+C <= 2)",
        kind: TaskKind::Logic3(&LOGIC3_AG),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3AH",
        desc: "Logic 3AH (A+B+C == 3)",
        kind: TaskKind::Logic3(&LOGIC3_AH),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3AI",
        desc: "Logic 3AI (A+B+C == 0,3)",
        kind: TaskKind::Logic3(&LOGIC3_AI),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3AJ",
        desc: "Logic 3AJ (A+B+C == 1,3) XOR",
        kind: TaskKind::Logic3(&LOGIC3_AJ),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3AK",
        desc: "Logic 3AK (A+B+C != 2)",
        kind: TaskKind::Logic3(&LOGIC3_AK),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3AL",
        desc: "Logic 3AL (A+B+C >= 2)",
        kind: TaskKind::Logic3(&LOGIC3_AL),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3AM",
        desc: "Logic 3AM (A+B+C != 1)",
        kind: TaskKind::Logic3(&LOGIC3_AM),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3AN",
        desc: "Logic 3AN (A+B+C != 0)",
        kind: TaskKind::Logic3(&LOGIC3_AN),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3AO",
        desc: "Logic 3AO (A & ~B & ~C) [3]",
        kind: TaskKind::Logic3(&LOGIC3_AO),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3AP",
        desc: "Logic 3AP (A^B & ~C)  [3]",
        kind: TaskKind::Logic3(&LOGIC3_AP),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3AQ",
        desc: "Logic 3AQ (A==B & ~C) [3]",
        kind: TaskKind::Logic3(&LOGIC3_AQ),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3AR",
        desc: "Logic 3AR (A & B & ~C) [3]",
        kind: TaskKind::Logic3(&LOGIC3_AR),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3AS",
        desc: "Logic 3AS",
        kind: TaskKind::Logic3(&LOGIC3_AS),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3AT",
        desc: "Logic 3AT",
        kind: TaskKind::Logic3(&LOGIC3_AT),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3AU",
        desc: "Logic 3AU",
        kind: TaskKind::Logic3(&LOGIC3_AU),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3AV",
        desc: "Logic 3AV",
        kind: TaskKind::Logic3(&LOGIC3_AV),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3AW",
        desc: "Logic 3AW",
        kind: TaskKind::Logic3(&LOGIC3_AW),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3AX",
        desc: "Logic 3AX",
        kind: TaskKind::Logic3(&LOGIC3_AX),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3AY",
        desc: "Logic 3AY",
        kind: TaskKind::Logic3(&LOGIC3_AY),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3AZ",
        desc: "Logic 3AZ",
        kind: TaskKind::Logic3(&LOGIC3_AZ),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3BA",
        desc: "Logic 3BA",
        kind: TaskKind::Logic3(&LOGIC3_BA),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3BB",
        desc: "Logic 3BB",
        kind: TaskKind::Logic3(&LOGIC3_BB),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3BC",
        desc: "Logic 3BC",
        kind: TaskKind::Logic3(&LOGIC3_BC),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3BD",
        desc: "Logic 3BD",
        kind: TaskKind::Logic3(&LOGIC3_BD),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3BE",
        desc: "Logic 3BE",
        kind: TaskKind::Logic3(&LOGIC3_BE),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3BF",
        desc: "Logic 3BF",
        kind: TaskKind::Logic3(&LOGIC3_BF),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3BG",
        desc: "Logic 3BG",
        kind: TaskKind::Logic3(&LOGIC3_BG),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3BH",
        desc: "Logic 3BH",
        kind: TaskKind::Logic3(&LOGIC3_BH),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3BI",
        desc: "Logic 3BI",
        kind: TaskKind::Logic3(&LOGIC3_BI),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3BJ",
        desc: "Logic 3BJ",
        kind: TaskKind::Logic3(&LOGIC3_BJ),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3BK",
        desc: "Logic 3BK",
        kind: TaskKind::Logic3(&LOGIC3_BK),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3BL",
        desc: "Logic 3BL",
        kind: TaskKind::Logic3(&LOGIC3_BL),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3BM",
        desc: "Logic 3BM",
        kind: TaskKind::Logic3(&LOGIC3_BM),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3BN",
        desc: "Logic 3BN",
        kind: TaskKind::Logic3(&LOGIC3_BN),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3BO",
        desc: "Logic 3BO",
        kind: TaskKind::Logic3(&LOGIC3_BO),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3BP",
        desc: "Logic 3BP",
        kind: TaskKind::Logic3(&LOGIC3_BP),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3BQ",
        desc: "Logic 3BQ",
        kind: TaskKind::Logic3(&LOGIC3_BQ),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3BR",
        desc: "Logic 3BR",
        kind: TaskKind::Logic3(&LOGIC3_BR),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3BS",
        desc: "Logic 3BS",
        kind: TaskKind::Logic3(&LOGIC3_BS),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3BT",
        desc: "Logic 3BT",
        kind: TaskKind::Logic3(&LOGIC3_BT),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3BU",
        desc: "Logic 3BU",
        kind: TaskKind::Logic3(&LOGIC3_BU),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3BV",
        desc: "Logic 3BV",
        kind: TaskKind::Logic3(&LOGIC3_BV),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3BW",
        desc: "Logic 3BW",
        kind: TaskKind::Logic3(&LOGIC3_BW),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3BX",
        desc: "Logic 3BX",
        kind: TaskKind::Logic3(&LOGIC3_BX),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3BY",
        desc: "Logic 3BY",
        kind: TaskKind::Logic3(&LOGIC3_BY),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3BZ",
        desc: "Logic 3BZ",
        kind: TaskKind::Logic3(&LOGIC3_BZ),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3CA",
        desc: "Logic 3CA",
        kind: TaskKind::Logic3(&LOGIC3_CA),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3CB",
        desc: "Logic 3CB",
        kind: TaskKind::Logic3(&LOGIC3_CB),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3CC",
        desc: "Logic 3CC",
        kind: TaskKind::Logic3(&LOGIC3_CC),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3CD",
        desc: "Logic 3CD",
        kind: TaskKind::Logic3(&LOGIC3_CD),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3CE",
        desc: "Logic 3CE",
        kind: TaskKind::Logic3(&LOGIC3_CE),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3CF",
        desc: "Logic 3CF",
        kind: TaskKind::Logic3(&LOGIC3_CF),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3CG",
        desc: "Logic 3CG",
        kind: TaskKind::Logic3(&LOGIC3_CG),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3CH",
        desc: "Logic 3CH",
        kind: TaskKind::Logic3(&LOGIC3_CH),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3CI",
        desc: "Logic 3CI",
        kind: TaskKind::Logic3(&LOGIC3_CI),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3CJ",
        desc: "Logic 3CJ",
        kind: TaskKind::Logic3(&LOGIC3_CJ),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3CK",
        desc: "Logic 3CK",
        kind: TaskKind::Logic3(&LOGIC3_CK),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3CL",
        desc: "Logic 3CL",
        kind: TaskKind::Logic3(&LOGIC3_CL),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3CM",
        desc: "Logic 3CM",
        kind: TaskKind::Logic3(&LOGIC3_CM),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3CN",
        desc: "Logic 3CN",
        kind: TaskKind::Logic3(&LOGIC3_CN),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3CO",
        desc: "Logic 3CO",
        kind: TaskKind::Logic3(&LOGIC3_CO),
        reqs: 0,
    },
    SimpleTask {
        name: "logic_3CP",
        desc: "Logic 3CP",
        kind: TaskKind::Logic3(&LOGIC3_CP),
        reqs: 0,
    },
    SimpleTask {
        name: "math_1AA",
        desc: "Math 1AA (2X)",
        kind: TaskKind::Math1(Math1Op::AA),
        reqs: 0,
    },
    SimpleTask {
        name: "math_1AB",
        desc: "Math 1AB (2X/3)",
        kind: TaskKind::Math1(Math1Op::AB),
        reqs: 0,
    },
    SimpleTask {
        name: "math_1AC",
        desc: "Math 1AC (5X/4)",
        kind: TaskKind::Math1(Math1Op::AC),
        reqs: 0,
    },
    SimpleTask {
        name: "math_1AD",
        desc: "Math 1AD (X^2)",
        kind: TaskKind::Math1(Math1Op::AD),
        reqs: 0,
    },
    SimpleTask {
        name: "math_1AE",
        desc: "Math 1AE (X^3)",
        kind: TaskKind::Math1(Math1Op::AE),
        reqs: 0,
    },
    SimpleTask {
        name: "math_1AF",
        desc: "Math 1AF (sqrt(X))",
        kind: TaskKind::Math1(Math1Op::AF),
        reqs: 0,
    },
    SimpleTask {
        name: "math_1AG",
        desc: "Math 1AG (log(X))",
        kind: TaskKind::Math1(Math1Op::AG),
        reqs: 0,
    },
    SimpleTask {
        name: "math_1AH",
        desc: "Math 1AH (X^2+X^3)",
        kind: TaskKind::Math1(Math1Op::AH),
        reqs: 0,
    },
    SimpleTask {
        name: "math_1AI",
        desc: "Math 1AI (X^2+sqrt(X))",
        kind: TaskKind::Math1(Math1Op::AI),
        reqs: 0,
    },
    SimpleTask {
        name: "math_1AJ",
        desc: "Math 1AJ (abs(X))",
        kind: TaskKind::Math1(Math1Op::AJ),
        reqs: 0,
    },
    SimpleTask {
        name: "math_1AK",
        desc: "Math 1AK (X-5)",
        kind: TaskKind::Math1(Math1Op::AK),
        reqs: 0,
    },
    SimpleTask {
        name: "math_1AL",
        desc: "Math 1AL (-X)",
        kind: TaskKind::Math1(Math1Op::AL),
        reqs: 0,
    },
    SimpleTask {
        name: "math_1AM",
        desc: "Math 1AM (5X)",
        kind: TaskKind::Math1(Math1Op::AM),
        reqs: 0,
    },
    SimpleTask {
        name: "math_1AN",
        desc: "Math 1AN (X/4)",
        kind: TaskKind::Math1(Math1Op::AN),
        reqs: 0,
    },
    SimpleTask {
        name: "math_1AO",
        desc: "Math 1AO (X-6)",
        kind: TaskKind::Math1(Math1Op::AO),
        reqs: 0,
    },
    SimpleTask {
        name: "math_1AP",
        desc: "Math 1AP (X-7)",
        kind: TaskKind::Math1(Math1Op::AP),
        reqs: 0,
    },
    SimpleTask {
        name: "math_1AS",
        desc: "Math 1AS (3Y)",
        kind: TaskKind::Math1(Math1Op::AS),
        reqs: 0,
    },
    SimpleTask {
        name: "math_2AA",
        desc: "Math 2AA (sqrt(X+Y))",
        kind: TaskKind::Math2(Math2Op::AA),
        reqs: 0,
    },
    SimpleTask {
        name: "math_2AB",
        desc: "Math 2AB ((X+Y)^2)",
        kind: TaskKind::Math2(Math2Op::AB),
        reqs: 0,
    },
    SimpleTask {
        name: "math_2AC",
        desc: "Math 2AC (X%Y)",
        kind: TaskKind::Math2(Math2Op::AC),
        reqs: 0,
    },
    SimpleTask {
        name: "math_2AD",
        desc: "Math 2AD (3X/2+5Y/4)",
        kind: TaskKind::Math2(Math2Op::AD),
        reqs: 0,
    },
    SimpleTask {
        name: "math_2AE",
        desc: "Math 2AE (abs(X-5)+abs(Y-6))",
        kind: TaskKind::Math2(Math2Op::AE),
        reqs: 0,
    },
    SimpleTask {
        name: "math_2AF",
        desc: "Math 2AF (XY-X/Y)",
        kind: TaskKind::Math2(Math2Op::AF),
        reqs: 0,
    },
    SimpleTask {
        name: "math_2AG",
        desc: "Math 2AG ((X-Y)^2)",
        kind: TaskKind::Math2(Math2Op::AG),
        reqs: 0,
    },
    SimpleTask {
        name: "math_2AH",
        desc: "Math 2AH (X^2+Y^2)",
        kind: TaskKind::Math2(Math2Op::AH),
        reqs: 0,
    },
    SimpleTask {
        name: "math_2AI",
        desc: "Math 2AI (X^2+Y^3)",
        kind: TaskKind::Math2(Math2Op::AI),
        reqs: 0,
    },
    SimpleTask {
        name: "math_2AJ",
        desc: "Math 2AJ ((sqrt(X)+Y)/(X-7))",
        kind: TaskKind::Math2(Math2Op::AJ),
        reqs: 0,
    },
    SimpleTask {
        name: "math_2AK",
        desc: "Math 2AK (log(|X/Y|))",
        kind: TaskKind::Math2(Math2Op::AK),
        reqs: 0,
    },
    SimpleTask {
        name: "math_2AL",
        desc: "Math 2AL (log(|X|)/Y)",
        kind: TaskKind::Math2(Math2Op::AL),
        reqs: 0,
    },
    SimpleTask {
        name: "math_2AM",
        desc: "Math 2AM (X/log(|Y|))",
        kind: TaskKind::Math2(Math2Op::AM),
        reqs: 0,
    },
    SimpleTask {
        name: "math_2AN",
        desc: "Math 2AN (X+Y)",
        kind: TaskKind::Math2(Math2Op::AN),
        reqs: 0,
    },
    SimpleTask {
        name: "math_2AO",
        desc: "Math 2AO (X-Y)",
        kind: TaskKind::Math2(Math2Op::AO),
        reqs: 0,
    },
    SimpleTask {
        name: "math_2AP",
        desc: "Math 2AP (X/Y)",
        kind: TaskKind::Math2(Math2Op::AP),
        reqs: 0,
    },
    SimpleTask {
        name: "math_2AQ",
        desc: "Math 2AQ (XY)",
        kind: TaskKind::Math2(Math2Op::AQ),
        reqs: 0,
    },
    SimpleTask {
        name: "math_2AR",
        desc: "Math 2AR (sqrt(X)+sqrt(Y))",
        kind: TaskKind::Math2(Math2Op::AR),
        reqs: 0,
    },
    SimpleTask {
        name: "math_2AS",
        desc: "Math 2AS (X+2Y)",
        kind: TaskKind::Math2(Math2Op::AS),
        reqs: 0,
    },
    SimpleTask {
        name: "math_2AT",
        desc: "Math 2AT (X+3Y)",
        kind: TaskKind::Math2(Math2Op::AT),
        reqs: 0,
    },
    SimpleTask {
        name: "math_2AU",
        desc: "Math 2AU (2X+3Y)",
        kind: TaskKind::Math2(Math2Op::AU),
        reqs: 0,
    },
    SimpleTask {
        name: "math_2AV",
        desc: "Math 2AV (XY^2)",
        kind: TaskKind::Math2(Math2Op::AV),
        reqs: 0,
    },
    SimpleTask {
        name: "math_2AX",
        desc: "Math 2AX (X+3Y)",
        kind: TaskKind::Math2(Math2Op::AX),
        reqs: 0,
    },
    SimpleTask {
        name: "math_2AY",
        desc: "Math 2AY (2A+B)",
        kind: TaskKind::Math2(Math2Op::AY),
        reqs: 0,
    },
    SimpleTask {
        name: "math_2AZ",
        desc: "Math 2AZ (4A+6B)",
        kind: TaskKind::Math2(Math2Op::AZ),
        reqs: 0,
    },
    SimpleTask {
        name: "math_2AAA",
        desc: "Math 2AAA (3A-2B)",
        kind: TaskKind::Math2(Math2Op::AAA),
        reqs: 0,
    },
    SimpleTask {
        name: "math_3AA",
        desc: "Math 3AA (X^2+Y^2+Z^2)",
        kind: TaskKind::Math3(Math3Op::AA),
        reqs: 0,
    },
    SimpleTask {
        name: "math_3AB",
        desc: "Math 3AB (sqrt(X)+sqrt(Y)+sqrt(Z))",
        kind: TaskKind::Math3(Math3Op::AB),
        reqs: 0,
    },
    SimpleTask {
        name: "math_3AC",
        desc: "Math 3AC (X+2Y+3Z)",
        kind: TaskKind::Math3(Math3Op::AC),
        reqs: 0,
    },
    SimpleTask {
        name: "math_3AD",
        desc: "Math 3AD (XY^2+Z^3)",
        kind: TaskKind::Math3(Math3Op::AD),
        reqs: 0,
    },
    SimpleTask {
        name: "math_3AE",
        desc: "Math 3AE ((X%Y)*Z)",
        kind: TaskKind::Math3(Math3Op::AE),
        reqs: 0,
    },
    SimpleTask {
        name: "math_3AF",
        desc: "Math 3AF ((X+Y)^2+sqrt(Y+Z))",
        kind: TaskKind::Math3(Math3Op::AF),
        reqs: 0,
    },
    SimpleTask {
        name: "math_3AG",
        desc: "Math 3AG ((XY)%(YZ))",
        kind: TaskKind::Math3(Math3Op::AG),
        reqs: 0,
    },
    SimpleTask {
        name: "math_3AH",
        desc: "Math 3AH (X+Y+Z)",
        kind: TaskKind::Math3(Math3Op::AH),
        reqs: 0,
    },
    SimpleTask {
        name: "math_3AI",
        desc: "Math 3AI (-X-Y-Z)",
        kind: TaskKind::Math3(Math3Op::AI),
        reqs: 0,
    },
    SimpleTask {
        name: "math_3AJ",
        desc: "Math 3AJ ((X-Y)^2+(Y-Z)^2+(Z-X)^2)",
        kind: TaskKind::Math3(Math3Op::AJ),
        reqs: 0,
    },
    SimpleTask {
        name: "math_3AK",
        desc: "Math 3AK ((X+Y)^2+(Y+Z)^2+(Z+X)^2)",
        kind: TaskKind::Math3(Math3Op::AK),
        reqs: 0,
    },
    SimpleTask {
        name: "math_3AL",
        desc: "Math 3AL ((X-Y)^2+(X-Z)^2)",
        kind: TaskKind::Math3(Math3Op::AL),
        reqs: 0,
    },
    SimpleTask {
        name: "math_3AM",
        desc: "Math 3AM ((X+Y)^2+(Y+Z)^2)",
        kind: TaskKind::Math3(Math3Op::AM),
        reqs: 0,
    },
    SimpleTask {
        name: "fib_1",
        desc: "First Fib number (0)",
        kind: TaskKind::Fib(1),
        reqs: 0,
    },
    SimpleTask {
        name: "fib_2",
        desc: "Second and Third Fib number (1)",
        kind: TaskKind::Fib(2),
        reqs: 0,
    },
    SimpleTask {
        name: "fib_4",
        desc: "Fourth Fib number (2)",
        kind: TaskKind::Fib(4),
        reqs: 0,
    },
    SimpleTask {
        name: "fib_5",
        desc: "Fifth Fib number (3)",
        kind: TaskKind::Fib(5),
        reqs: 0,
    },
    SimpleTask {
        name: "fib_6",
        desc: "Sixth Fib number (5)",
        kind: TaskKind::Fib(6),
        reqs: 0,
    },
    SimpleTask {
        name: "fib_7",
        desc: "Seventh Fib number (8)",
        kind: TaskKind::Fib(7),
        reqs: 0,
    },
    SimpleTask {
        name: "fib_8",
        desc: "Eighth Fib number (13)",
        kind: TaskKind::Fib(8),
        reqs: 0,
    },
    SimpleTask {
        name: "fib_9",
        desc: "Ninth Fib number (21)",
        kind: TaskKind::Fib(9),
        reqs: 0,
    },
    SimpleTask {
        name: "fib_10",
        desc: "Tenth Fib number (34)",
        kind: TaskKind::Fib(10),
        reqs: 0,
    },
    SimpleTask {
        name: "comm_echo",
        desc: "Echo of Neighbor's Input",
        kind: TaskKind::CommEcho,
        reqs: REQ_NEIGHBOR_INPUT,
    },
    SimpleTask {
        name: "comm_not",
        desc: "Not of Neighbor's Input",
        kind: TaskKind::CommNot,
        reqs: REQ_NEIGHBOR_INPUT,
    },
    SimpleTask {
        name: "move_up_gradient",
        desc: "Move up gradient",
        kind: TaskKind::MoveUpGradient,
        reqs: 0,
    },
    SimpleTask {
        name: "move_neutral_gradient",
        desc: "Move neutral gradient",
        kind: TaskKind::MoveNeutralGradient,
        reqs: 0,
    },
    SimpleTask {
        name: "move_down_gradient",
        desc: "Move down gradient",
        kind: TaskKind::MoveDownGradient,
        reqs: 0,
    },
    SimpleTask {
        name: "move_not_up_gradient",
        desc: "Move not up gradient",
        kind: TaskKind::MoveNotUpGradient,
        reqs: 0,
    },
    SimpleTask {
        name: "move_to_right_side",
        desc: "Move to right side",
        kind: TaskKind::MoveToRightSide,
        reqs: 0,
    },
    SimpleTask {
        name: "move_to_left_side",
        desc: "Move to left side",
        kind: TaskKind::MoveToLeftSide,
        reqs: 0,
    },
    SimpleTask {
        name: "move",
        desc: "Successfully Moved",
        kind: TaskKind::Move,
        reqs: 0,
    },
    SimpleTask {
        name: "movetotarget",
        desc: "Move to a target area",
        kind: TaskKind::MoveToTarget,
        reqs: 0,
    },
    SimpleTask {
        name: "movetoevent",
        desc: "Move to a target area",
        kind: TaskKind::MoveToMovementEvent,
        reqs: 0,
    },
    SimpleTask {
        name: "movebetweenevent",
        desc: "Move to a target area",
        kind: TaskKind::MoveBetweenMovementEvent,
        reqs: 0,
    },
    SimpleTask {
        name: "perfect_strings",
        desc: "Produce and store perfect strings",
        kind: TaskKind::CreatePerfectStrings,
        reqs: 0,
    },
    SimpleTask {
        name: "move_to_event",
        desc: "Moved into cell containing event",
        kind: TaskKind::MoveToEvent,
        reqs: 0,
    },
    SimpleTask {
        name: "event_killed",
        desc: "Killed event",
        kind: TaskKind::EventKilled,
        reqs: 0,
    },
    SimpleTask {
        name: "exploded",
        desc: "Organism exploded",
        kind: TaskKind::Exploded,
        reqs: 0,
    },
    SimpleTask {
        name: "exploded2",
        desc: "Organism exploded",
        kind: TaskKind::Exploded2,
        reqs: 0,
    },
    SimpleTask {
        name: "consume-public-good",
        desc: "Public good consumed",
        kind: TaskKind::ConsumePublicGood,
        reqs: 0,
    },
    SimpleTask {
        name: "ai-display-cost",
        desc: "Autoinducer cost paid",
        kind: TaskKind::AIDisplayCost,
        reqs: 0,
    },
    SimpleTask {
        name: "produce-public-good",
        desc: "Public good produced",
        kind: TaskKind::ProducePublicGood,
        reqs: 0,
    },
];
