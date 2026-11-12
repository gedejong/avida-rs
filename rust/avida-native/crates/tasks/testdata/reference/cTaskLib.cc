/*
 *  cTaskLib.cc
 *  Avida
 *
 *  Called "task_lib.cc" prior to 12/5/05.
 *  Copyright 1999-2011 Michigan State University. All rights reserved.
 *  Copyright 1993-2003 California Institute of Technology.
 *
 *
 *  This file is part of Avida.
 *
 *  Avida is free software; you can redistribute it and/or modify it under the terms of the GNU Lesser General Public License
 *  as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
 *
 *  Avida is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of
 *  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the GNU Lesser General Public License for more details.
 *
 *  You should have received a copy of the GNU Lesser General Public License along with Avida.
 *  If not, see <http://www.gnu.org/licenses/>.
 *
 */

#include "cTaskLib.h"

#include "apto/platform.h"

#include "cArgSchema.h"
#include "cDeme.h"
#include "cEnvironment.h"
#include "cEnvReqs.h"
#include "cTaskState.h"
#include "cPopulation.h"
#include "cPopulationCell.h"
#include "cOrgMessagePredicate.h"
#include "cOrgMovementPredicate.h"
#include "cStateGrid.h"
#include "cUserFeedback.h"
#include "rust/running_stats_ffi.h"

#include <cstdlib>
#include <cmath>
#include <climits>
#include <iomanip>
#include <set>

// Various workarounds for Visual Studio shortcomings
#if APTO_PLATFORM(WINDOWS)
# define llabs(x) _abs64(x)
# define log2(x) (log(x)/log(2.0))
#endif

// Various workarounds for FreeBSD
#if APTO_PLATFORM(FREEBSD)
# define log2(x) (log(x)/log(2.0))
#endif

static const double dCastPrecision = 100000.0;

// Boolean logic task type constants (must match Rust avd_task_eval_logic)
static const int AVD_TASK_NOT    = 0;
static const int AVD_TASK_NAND   = 1;
static const int AVD_TASK_AND    = 2;
static const int AVD_TASK_ORNOT  = 3;
static const int AVD_TASK_OR     = 4;
static const int AVD_TASK_ANDNOT = 5;
static const int AVD_TASK_NOR    = 6;
static const int AVD_TASK_XOR    = 7;
static const int AVD_TASK_EQU    = 8;

// Math1in operation codes (must match Rust avd_task_eval_math1in)
static const int MATH1IN_AA = 0;
static const int MATH1IN_AB = 1;
static const int MATH1IN_AC = 2;
static const int MATH1IN_AD = 3;
static const int MATH1IN_AE = 4;
static const int MATH1IN_AF = 5;
static const int MATH1IN_AG = 6;
static const int MATH1IN_AH = 7;
static const int MATH1IN_AI = 8;
static const int MATH1IN_AJ = 9;
static const int MATH1IN_AK = 10;
static const int MATH1IN_AL = 11;
static const int MATH1IN_AM = 12;
static const int MATH1IN_AN = 13;
static const int MATH1IN_AO = 14;
static const int MATH1IN_AP = 15;
static const int MATH1IN_AS = 16;

// Math2in operation codes (must match Rust avd_task_eval_math2in)
static const int MATH2IN_AA  = 0;
static const int MATH2IN_AB  = 1;
static const int MATH2IN_AC  = 2;
static const int MATH2IN_AD  = 3;
static const int MATH2IN_AE  = 4;
static const int MATH2IN_AF  = 5;
static const int MATH2IN_AG  = 6;
static const int MATH2IN_AH  = 7;
static const int MATH2IN_AI  = 8;
static const int MATH2IN_AJ  = 9;
static const int MATH2IN_AK  = 10;
static const int MATH2IN_AL  = 11;
static const int MATH2IN_AM  = 12;
static const int MATH2IN_AN  = 13;
static const int MATH2IN_AO  = 14;
static const int MATH2IN_AP  = 15;
static const int MATH2IN_AQ  = 16;
static const int MATH2IN_AR  = 17;
static const int MATH2IN_AS  = 18;
static const int MATH2IN_AT  = 19;
static const int MATH2IN_AU  = 20;
static const int MATH2IN_AV  = 21;
static const int MATH2IN_AX  = 22;
static const int MATH2IN_AY  = 23;
static const int MATH2IN_AZ  = 24;
static const int MATH2IN_AAA = 25;

// Math3in operation codes (must match Rust avd_task_eval_math3in)
static const int MATH3IN_AA  = 0;
static const int MATH3IN_AB  = 1;
static const int MATH3IN_AC  = 2;
static const int MATH3IN_AD  = 3;
static const int MATH3IN_AE  = 4;
static const int MATH3IN_AF  = 5;
static const int MATH3IN_AG  = 6;
static const int MATH3IN_AH  = 7;
static const int MATH3IN_AI  = 8;
static const int MATH3IN_AJ  = 9;
static const int MATH3IN_AK  = 10;
static const int MATH3IN_AL  = 11;
static const int MATH3IN_AM  = 12;

// Simple arithmetic operation codes (must match Rust avd_task_eval_simple_arith)
static const int ARITH_ADD  = 0;
static const int ARITH_ADD3 = 1;
static const int ARITH_SUB  = 2;

// Helper: extract contiguous input array from tBuffer for Rust FFI
static inline int extract_inputs(const tBuffer<int>& buf, int* out, int max_n) {
  int n = buf.GetNumStored();
  if (n > max_n) n = max_n;
  for (int i = 0; i < n; i++) out[i] = buf[i];
  return n;
}


cTaskLib::~cTaskLib()
{
  for (int i = 0; i < task_array.GetSize(); i++) delete task_array[i];
}

inline double cTaskLib::FractionalReward(unsigned int supplied, unsigned int correct)
{
  return avd_tasklib_fractional_reward_bits(supplied, correct);
}

cTaskEntry* cTaskLib::AddTask(const cString& name, const cString& info, cEnvReqs& envreqs, Feedback& feedback)
{
  // Determine if this task is already in the active library.
  for (int i = 0; i < task_array.GetSize(); i++) {
    assert(task_array[i] != NULL);
    if (task_array[i]->GetName() == name && !task_array[i]->HasArguments()) return task_array[i];
  }
  
  // Match up this name to its corresponding task
  const int start_size = task_array.GetSize();
  
  // The following if blocks are grouped based on class of task.  Chaining too
  // many if block causes problems block nesting depth in Visual Studio.net 2003.
  
  if (avd_tasklib_is_basic_name((const char*) name) != 0) {
  if (name == "echo")      NewTask(name, "Echo", &cTaskLib::Task_Echo);
  else if (name == "echo_dup")  NewTask(name, "Echo_dup",  &cTaskLib::Task_Echo);
  else if (name == "add")  NewTask(name, "Add",  &cTaskLib::Task_Add);
  else if (name == "add3")  NewTask(name, "Add3",  &cTaskLib::Task_Add3);  
  else if (name == "sub")  NewTask(name, "Sub",  &cTaskLib::Task_Sub);
  // @WRE DontCare task always succeeds.
  else if (name == "dontcare")  NewTask(name, "DontCare", &cTaskLib::Task_DontCare);
  
  // All 1- and 2-Input Logic Functions
  if (name == "not") NewTask(name, "Not", &cTaskLib::Task_Not);
  else if (name == "not_dup") NewTask(name, "Not_dup", &cTaskLib::Task_Not);
  else if (name == "nand") NewTask(name, "Nand", &cTaskLib::Task_Nand);
  else if (name == "nand_dup") NewTask(name, "Nand_dup", &cTaskLib::Task_Nand);
  else if (name == "and") NewTask(name, "And", &cTaskLib::Task_And);
  else if (name == "and_dup") NewTask(name, "And_dup", &cTaskLib::Task_And);
  else if (name == "orn") NewTask(name, "OrNot", &cTaskLib::Task_OrNot);
  else if (name == "orn_dup") NewTask(name, "OrNot_dup", &cTaskLib::Task_OrNot);
  else if (name == "or") NewTask(name, "Or", &cTaskLib::Task_Or);
  else if (name == "or_dup") NewTask(name, "Or_dup", &cTaskLib::Task_Or);
  else if (name == "andn") NewTask(name, "AndNot", &cTaskLib::Task_AndNot);
  else if (name == "andn_dup") NewTask(name, "AndNot_dup", &cTaskLib::Task_AndNot);
  else if (name == "nor") NewTask(name, "Nor", &cTaskLib::Task_Nor);
  else if (name == "nor_dup") NewTask(name, "Nor_dup", &cTaskLib::Task_Nor);
  else if (name == "xor") NewTask(name, "Xor", &cTaskLib::Task_Xor);
  else if (name == "xor_dup") NewTask(name, "Xor_dup", &cTaskLib::Task_Xor);
  else if (name == "equ") NewTask(name, "Equals", &cTaskLib::Task_Equ);
  else if (name == "equ_dup") NewTask(name, "Equals_dup", &cTaskLib::Task_Equ);
  
  else if (name == "xor-max") NewTask(name, "Xor-max", &cTaskLib::Task_XorMax);
	// resoruce dependent version
  else if (name == "nand-resourceDependent") NewTask(name, "Nand-resourceDependent", &cTaskLib::Task_Nand_ResourceDependent);
  else if (name == "nor-resourceDependent") NewTask(name, "Nor-resourceDependent", &cTaskLib::Task_Nor_ResourceDependent);
  } // end basic name gate

  // All 3-Input Logic Functions + Arbitrary 1-Input Math Tasks
  // Registration-family gating keeps this large deterministic name chain isolated.
  if (avd_tasklib_is_logic3_or_math1_name((const char*) name) != 0) {
  if (name == "logic_3AA")      NewTask(name, "Logic 3AA (A+B+C == 0)", &cTaskLib::Task_Logic3in_AA);
  else if (name == "logic_3AB") NewTask(name, "Logic 3AB (A+B+C == 1)", &cTaskLib::Task_Logic3in_AB);
  else if (name == "logic_3AC") NewTask(name, "Logic 3AC (A+B+C <= 1)", &cTaskLib::Task_Logic3in_AC);
  else if (name == "logic_3AD") NewTask(name, "Logic 3AD (A+B+C == 2)", &cTaskLib::Task_Logic3in_AD);
  else if (name == "logic_3AE") NewTask(name, "Logic 3AE (A+B+C == 0,2)", &cTaskLib::Task_Logic3in_AE);
  else if (name == "logic_3AF") NewTask(name, "Logic 3AF (A+B+C == 1,2)", &cTaskLib::Task_Logic3in_AF);
  else if (name == "logic_3AG") NewTask(name, "Logic 3AG (A+B+C <= 2)", &cTaskLib::Task_Logic3in_AG);
  else if (name == "logic_3AH") NewTask(name, "Logic 3AH (A+B+C == 3)", &cTaskLib::Task_Logic3in_AH);
  else if (name == "logic_3AI") NewTask(name, "Logic 3AI (A+B+C == 0,3)", &cTaskLib::Task_Logic3in_AI);
  else if (name == "logic_3AJ") NewTask(name, "Logic 3AJ (A+B+C == 1,3) XOR", &cTaskLib::Task_Logic3in_AJ);
  else if (name == "logic_3AK") NewTask(name, "Logic 3AK (A+B+C != 2)", &cTaskLib::Task_Logic3in_AK);
  else if (name == "logic_3AL") NewTask(name, "Logic 3AL (A+B+C >= 2)", &cTaskLib::Task_Logic3in_AL);
  else if (name == "logic_3AM") NewTask(name, "Logic 3AM (A+B+C != 1)", &cTaskLib::Task_Logic3in_AM);
  else if (name == "logic_3AN") NewTask(name, "Logic 3AN (A+B+C != 0)", &cTaskLib::Task_Logic3in_AN);
  else if (name == "logic_3AO") NewTask(name, "Logic 3AO (A & ~B & ~C) [3]", &cTaskLib::Task_Logic3in_AO);
  else if (name == "logic_3AP") NewTask(name, "Logic 3AP (A^B & ~C)  [3]", &cTaskLib::Task_Logic3in_AP);
  else if (name == "logic_3AQ") NewTask(name, "Logic 3AQ (A==B & ~C) [3]", &cTaskLib::Task_Logic3in_AQ);
  else if (name == "logic_3AR") NewTask(name, "Logic 3AR (A & B & ~C) [3]", &cTaskLib::Task_Logic3in_AR);
  else if (name == "logic_3AS") NewTask(name, "Logic 3AS", &cTaskLib::Task_Logic3in_AS);
  else if (name == "logic_3AT") NewTask(name, "Logic 3AT", &cTaskLib::Task_Logic3in_AT);
  else if (name == "logic_3AU") NewTask(name, "Logic 3AU", &cTaskLib::Task_Logic3in_AU);
  else if (name == "logic_3AV") NewTask(name, "Logic 3AV", &cTaskLib::Task_Logic3in_AV);
  else if (name == "logic_3AW") NewTask(name, "Logic 3AW", &cTaskLib::Task_Logic3in_AW);
  else if (name == "logic_3AX") NewTask(name, "Logic 3AX", &cTaskLib::Task_Logic3in_AX);
  else if (name == "logic_3AY") NewTask(name, "Logic 3AY", &cTaskLib::Task_Logic3in_AY);
  else if (name == "logic_3AZ") NewTask(name, "Logic 3AZ", &cTaskLib::Task_Logic3in_AZ);
  else if (name == "logic_3BA") NewTask(name, "Logic 3BA", &cTaskLib::Task_Logic3in_BA);
  else if (name == "logic_3BB") NewTask(name, "Logic 3BB", &cTaskLib::Task_Logic3in_BB);
  else if (name == "logic_3BC") NewTask(name, "Logic 3BC", &cTaskLib::Task_Logic3in_BC);
  else if (name == "logic_3BD") NewTask(name, "Logic 3BD", &cTaskLib::Task_Logic3in_BD);
  else if (name == "logic_3BE") NewTask(name, "Logic 3BE", &cTaskLib::Task_Logic3in_BE);
  else if (name == "logic_3BF") NewTask(name, "Logic 3BF", &cTaskLib::Task_Logic3in_BF);
  else if (name == "logic_3BG") NewTask(name, "Logic 3BG", &cTaskLib::Task_Logic3in_BG);
  else if (name == "logic_3BH") NewTask(name, "Logic 3BH", &cTaskLib::Task_Logic3in_BH);
  else if (name == "logic_3BI") NewTask(name, "Logic 3BI", &cTaskLib::Task_Logic3in_BI);
  else if (name == "logic_3BJ") NewTask(name, "Logic 3BJ", &cTaskLib::Task_Logic3in_BJ);
  else if (name == "logic_3BK") NewTask(name, "Logic 3BK", &cTaskLib::Task_Logic3in_BK);
  else if (name == "logic_3BL") NewTask(name, "Logic 3BL", &cTaskLib::Task_Logic3in_BL);
  else if (name == "logic_3BM") NewTask(name, "Logic 3BM", &cTaskLib::Task_Logic3in_BM);
  else if (name == "logic_3BN") NewTask(name, "Logic 3BN", &cTaskLib::Task_Logic3in_BN);
  else if (name == "logic_3BO") NewTask(name, "Logic 3BO", &cTaskLib::Task_Logic3in_BO);
  else if (name == "logic_3BP") NewTask(name, "Logic 3BP", &cTaskLib::Task_Logic3in_BP);
  else if (name == "logic_3BQ") NewTask(name, "Logic 3BQ", &cTaskLib::Task_Logic3in_BQ);
  else if (name == "logic_3BR") NewTask(name, "Logic 3BR", &cTaskLib::Task_Logic3in_BR);
  else if (name == "logic_3BS") NewTask(name, "Logic 3BS", &cTaskLib::Task_Logic3in_BS);
  else if (name == "logic_3BT") NewTask(name, "Logic 3BT", &cTaskLib::Task_Logic3in_BT);
  else if (name == "logic_3BU") NewTask(name, "Logic 3BU", &cTaskLib::Task_Logic3in_BU);
  else if (name == "logic_3BV") NewTask(name, "Logic 3BV", &cTaskLib::Task_Logic3in_BV);
  else if (name == "logic_3BW") NewTask(name, "Logic 3BW", &cTaskLib::Task_Logic3in_BW);
  else if (name == "logic_3BX") NewTask(name, "Logic 3BX", &cTaskLib::Task_Logic3in_BX);
  else if (name == "logic_3BY") NewTask(name, "Logic 3BY", &cTaskLib::Task_Logic3in_BY);
  else if (name == "logic_3BZ") NewTask(name, "Logic 3BZ", &cTaskLib::Task_Logic3in_BZ);
  else if (name == "logic_3CA") NewTask(name, "Logic 3CA", &cTaskLib::Task_Logic3in_CA);
  else if (name == "logic_3CB") NewTask(name, "Logic 3CB", &cTaskLib::Task_Logic3in_CB);
  else if (name == "logic_3CC") NewTask(name, "Logic 3CC", &cTaskLib::Task_Logic3in_CC);
  else if (name == "logic_3CD") NewTask(name, "Logic 3CD", &cTaskLib::Task_Logic3in_CD);
  else if (name == "logic_3CE") NewTask(name, "Logic 3CE", &cTaskLib::Task_Logic3in_CE);
  else if (name == "logic_3CF") NewTask(name, "Logic 3CF", &cTaskLib::Task_Logic3in_CF);
  else if (name == "logic_3CG") NewTask(name, "Logic 3CG", &cTaskLib::Task_Logic3in_CG);
  else if (name == "logic_3CH") NewTask(name, "Logic 3CH", &cTaskLib::Task_Logic3in_CH);
  else if (name == "logic_3CI") NewTask(name, "Logic 3CI", &cTaskLib::Task_Logic3in_CI);
  else if (name == "logic_3CJ") NewTask(name, "Logic 3CJ", &cTaskLib::Task_Logic3in_CJ);
  else if (name == "logic_3CK") NewTask(name, "Logic 3CK", &cTaskLib::Task_Logic3in_CK);
  else if (name == "logic_3CL") NewTask(name, "Logic 3CL", &cTaskLib::Task_Logic3in_CL);
  else if (name == "logic_3CM") NewTask(name, "Logic 3CM", &cTaskLib::Task_Logic3in_CM);
  else if (name == "logic_3CN") NewTask(name, "Logic 3CN", &cTaskLib::Task_Logic3in_CN);
  else if (name == "logic_3CO") NewTask(name, "Logic 3CO", &cTaskLib::Task_Logic3in_CO);
  else if (name == "logic_3CP") NewTask(name, "Logic 3CP", &cTaskLib::Task_Logic3in_CP);
  
  // Arbitrary 1-Input Math Tasks
  else if (name == "math_1AA") NewTask(name, "Math 1AA (2X)", &cTaskLib::Task_Math1in_AA);
  else if (name == "math_1AB") NewTask(name, "Math 1AB (2X/3)", &cTaskLib::Task_Math1in_AB);  
  else if (name == "math_1AC") NewTask(name, "Math 1AC (5X/4)", &cTaskLib::Task_Math1in_AC);  
  else if (name == "math_1AD") NewTask(name, "Math 1AD (X^2)", &cTaskLib::Task_Math1in_AD);  
  else if (name == "math_1AE") NewTask(name, "Math 1AE (X^3)", &cTaskLib::Task_Math1in_AE);  
  else if (name == "math_1AF") NewTask(name, "Math 1AF (sqrt(X))", &cTaskLib::Task_Math1in_AF);  
  else if (name == "math_1AG") NewTask(name, "Math 1AG (log(X))", &cTaskLib::Task_Math1in_AG);  
  else if (name == "math_1AH") NewTask(name, "Math 1AH (X^2+X^3)", &cTaskLib::Task_Math1in_AH);  
  else if (name == "math_1AI") NewTask(name, "Math 1AI (X^2+sqrt(X))", &cTaskLib::Task_Math1in_AI);  
  else if (name == "math_1AJ") NewTask(name, "Math 1AJ (abs(X))", &cTaskLib::Task_Math1in_AJ);  
  else if (name == "math_1AK") NewTask(name, "Math 1AK (X-5)", &cTaskLib::Task_Math1in_AK);  
  else if (name == "math_1AL") NewTask(name, "Math 1AL (-X)", &cTaskLib::Task_Math1in_AL);  
  else if (name == "math_1AM") NewTask(name, "Math 1AM (5X)", &cTaskLib::Task_Math1in_AM);  
  else if (name == "math_1AN") NewTask(name, "Math 1AN (X/4)", &cTaskLib::Task_Math1in_AN);  
  else if (name == "math_1AO") NewTask(name, "Math 1AO (X-6)", &cTaskLib::Task_Math1in_AO);  
  else if (name == "math_1AP") NewTask(name, "Math 1AP (X-7)", &cTaskLib::Task_Math1in_AP);
  else if (name == "math_1AS") NewTask(name, "Math 1AS (3Y)", &cTaskLib::Task_Math1in_AS);
  }
  
  // Arbitrary 2-Input + 3-Input Math Tasks
  // Registration-family gating keeps these large deterministic chains isolated.
  if (avd_tasklib_is_math2_or_math3_name((const char*) name) != 0) {
  if (name == "math_2AA") NewTask(name, "Math 2AA (sqrt(X+Y))", &cTaskLib::Task_Math2in_AA);  
  else if (name == "math_2AB") NewTask(name, "Math 2AB ((X+Y)^2)", &cTaskLib::Task_Math2in_AB);  
  else if (name == "math_2AC") NewTask(name, "Math 2AC (X%Y)", &cTaskLib::Task_Math2in_AC);  
  else if (name == "math_2AD") NewTask(name, "Math 2AD (3X/2+5Y/4)", &cTaskLib::Task_Math2in_AD);  
  else if (name == "math_2AE") NewTask(name, "Math 2AE (abs(X-5)+abs(Y-6))", &cTaskLib::Task_Math2in_AE);  
  else if (name == "math_2AF") NewTask(name, "Math 2AF (XY-X/Y)", &cTaskLib::Task_Math2in_AF);  
  else if (name == "math_2AG") NewTask(name, "Math 2AG ((X-Y)^2)", &cTaskLib::Task_Math2in_AG);  
  else if (name == "math_2AH") NewTask(name, "Math 2AH (X^2+Y^2)", &cTaskLib::Task_Math2in_AH);  
  else if (name == "math_2AI") NewTask(name, "Math 2AI (X^2+Y^3)", &cTaskLib::Task_Math2in_AI);
  else if (name == "math_2AJ") NewTask(name, "Math 2AJ ((sqrt(X)+Y)/(X-7))", &cTaskLib::Task_Math2in_AJ);
  else if (name == "math_2AK") NewTask(name, "Math 2AK (log(|X/Y|))", &cTaskLib::Task_Math2in_AK);
  else if (name == "math_2AL") NewTask(name, "Math 2AL (log(|X|)/Y)", &cTaskLib::Task_Math2in_AL);
  else if (name == "math_2AM") NewTask(name, "Math 2AM (X/log(|Y|))", &cTaskLib::Task_Math2in_AM);
  else if (name == "math_2AN") NewTask(name, "Math 2AN (X+Y)", &cTaskLib::Task_Math2in_AN);
  else if (name == "math_2AO") NewTask(name, "Math 2AO (X-Y)", &cTaskLib::Task_Math2in_AO);
  else if (name == "math_2AP") NewTask(name, "Math 2AP (X/Y)", &cTaskLib::Task_Math2in_AP);
  else if (name == "math_2AQ") NewTask(name, "Math 2AQ (XY)", &cTaskLib::Task_Math2in_AQ);
  else if (name == "math_2AR") NewTask(name, "Math 2AR (sqrt(X)+sqrt(Y))", &cTaskLib::Task_Math2in_AR);
  else if (name == "math_2AS") NewTask(name, "Math 2AS (X+2Y)", &cTaskLib::Task_Math2in_AS);
  else if (name == "math_2AT") NewTask(name, "Math 2AT (X+3Y)", &cTaskLib::Task_Math2in_AT);
  else if (name == "math_2AU") NewTask(name, "Math 2AU (2X+3Y)", &cTaskLib::Task_Math2in_AU);
  else if (name == "math_2AV") NewTask(name, "Math 2AV (XY^2)", &cTaskLib::Task_Math2in_AV);
  else if (name == "math_2AX") NewTask(name, "Math 2AX (X+3Y)", &cTaskLib::Task_Math2in_AX);
  else if (name == "math_2AY") NewTask(name, "Math 2AY (2A+B)", &cTaskLib::Task_Math2in_AY);
  else if (name == "math_2AZ") NewTask(name, "Math 2AZ (4A+6B)", &cTaskLib::Task_Math2in_AZ);
  else if (name == "math_2AAA") NewTask(name, "Math 2AAA (3A-2B)", &cTaskLib::Task_Math2in_AAA);
  
  // Arbitrary 3-Input Math Tasks
  if (name == "math_3AA")      NewTask(name, "Math 3AA (X^2+Y^2+Z^2)", &cTaskLib::Task_Math3in_AA);  
  else if (name == "math_3AB") NewTask(name, "Math 3AB (sqrt(X)+sqrt(Y)+sqrt(Z))", &cTaskLib::Task_Math3in_AB);  
  else if (name == "math_3AC") NewTask(name, "Math 3AC (X+2Y+3Z)", &cTaskLib::Task_Math3in_AC);  
  else if (name == "math_3AD") NewTask(name, "Math 3AD (XY^2+Z^3)", &cTaskLib::Task_Math3in_AD);  
  else if (name == "math_3AE") NewTask(name, "Math 3AE ((X%Y)*Z)", &cTaskLib::Task_Math3in_AE);  
  else if (name == "math_3AF") NewTask(name, "Math 3AF ((X+Y)^2+sqrt(Y+Z))", &cTaskLib::Task_Math3in_AF);
  else if (name == "math_3AG") NewTask(name, "Math 3AG ((XY)%(YZ))", &cTaskLib::Task_Math3in_AG);  
  else if (name == "math_3AH") NewTask(name, "Math 3AH (X+Y+Z)", &cTaskLib::Task_Math3in_AH);  
  else if (name == "math_3AI") NewTask(name, "Math 3AI (-X-Y-Z)", &cTaskLib::Task_Math3in_AI);  
  else if (name == "math_3AJ") NewTask(name, "Math 3AJ ((X-Y)^2+(Y-Z)^2+(Z-X)^2)", &cTaskLib::Task_Math3in_AJ);  
  else if (name == "math_3AK") NewTask(name, "Math 3AK ((X+Y)^2+(Y+Z)^2+(Z+X)^2)", &cTaskLib::Task_Math3in_AK);  
  else if (name == "math_3AL") NewTask(name, "Math 3AL ((X-Y)^2+(X-Z)^2)", &cTaskLib::Task_Math3in_AL);  
  else if (name == "math_3AM") NewTask(name, "Math 3AM ((X+Y)^2+(Y+Z)^2)", &cTaskLib::Task_Math3in_AM);  
  }

  // Fibonacci individual tasks
  if (avd_tasklib_is_fibonacci_name((const char*) name) != 0) {
  if (name == "fib_1") NewTask(name, "First Fib number (0)", &cTaskLib::Task_Fib1);
  else if (name == "fib_2") NewTask(name, "Second and Third Fib number (1)", &cTaskLib::Task_Fib2);
  else if (name == "fib_4") NewTask(name, "Fourth Fib number (2)", &cTaskLib::Task_Fib4);
  else if (name == "fib_5") NewTask(name, "Fifth Fib number (3)", &cTaskLib::Task_Fib5);
  else if (name == "fib_6") NewTask(name, "Sixth Fib number (5)", &cTaskLib::Task_Fib6);
  else if (name == "fib_7") NewTask(name, "Seventh Fib number (8)", &cTaskLib::Task_Fib7);
  else if (name == "fib_8") NewTask(name, "Eighth Fib number (13)", &cTaskLib::Task_Fib8);
  else if (name == "fib_9") NewTask(name, "Ninth Fib number (21)", &cTaskLib::Task_Fib9);
  else if (name == "fib_10") NewTask(name, "Tenth Fib number (34)", &cTaskLib::Task_Fib10);
  }
  
  // Matching + sequence tasks (load-based)
  if (avd_tasklib_is_matching_sequence_name((const char*) name) != 0) {
  if (name == "matchstr") Load_MatchStr(name, info, envreqs, feedback);
  else if (name == "match_number") Load_MatchNumber(name, info, envreqs, feedback);
  else if (name == "matchprodstr") Load_MatchProdStr(name, info, envreqs, feedback);
  else if (name == "sort_inputs") Load_SortInputs(name, info, envreqs, feedback);
  else if (name == "fibonacci_seq") Load_FibonacciSequence(name, info, envreqs, feedback);
  }
  
  // Load-based helper task families.
  if (avd_tasklib_is_load_based_name((const char*) name) != 0) {
  if (name == "mult")       Load_Mult(name, info, envreqs, feedback);
  else if (name == "div")   Load_Div(name, info, envreqs, feedback);
  else if (name == "log")   Load_Log(name, info, envreqs, feedback);
  else if (name == "log2")  Load_Log2(name, info, envreqs, feedback);
  else if (name == "log10") Load_Log10(name, info, envreqs, feedback);
  else if (name == "sqrt")  Load_Sqrt(name, info, envreqs, feedback);
  else if (name == "sine")  Load_Sine(name, info, envreqs, feedback);
  else if (name == "cosine") Load_Cosine(name, info, envreqs, feedback);
  else if (name == "optimize") Load_Optimize(name, info, envreqs, feedback);
  else if (name == "sg_path_traversal") Load_SGPathTraversal(name, info, envreqs, feedback);
  else if (name == "form-group") Load_FormSpatialGroup(name, info, envreqs, feedback);
  else if (name == "form-group-id") Load_FormSpatialGroupWithID(name, info, envreqs, feedback);
  else if (name == "live-on-patch-id") Load_LiveOnPatchRes(name, info, envreqs, feedback);
  else if (name == "collect-odd-cell") Load_CollectOddCell(name, info, envreqs, feedback);
  else if (name == "eat-target") Load_ConsumeTarget(name, info, envreqs, feedback);
  else if (name == "eat-target-echo") Load_ConsumeTargetEcho(name, info, envreqs, feedback);
  else if (name == "eat-target-nand") Load_ConsumeTargetNand(name, info, envreqs, feedback);
  else if (name == "eat-target-and") Load_ConsumeTargetAnd(name, info, envreqs, feedback);
  else if (name == "eat-target-orn") Load_ConsumeTargetOrn(name, info, envreqs, feedback);
  else if (name == "eat-target-or") Load_ConsumeTargetOr(name, info, envreqs, feedback);
  else if (name == "eat-target-andn") Load_ConsumeTargetAndn(name, info, envreqs, feedback);
  else if (name == "eat-target-nor") Load_ConsumeTargetNor(name, info, envreqs, feedback);
  else if (name == "eat-target-xor") Load_ConsumeTargetXor(name, info, envreqs, feedback);
  else if (name == "eat-target-equ") Load_ConsumeTargetEqu(name, info, envreqs, feedback);
  else if (name == "move-ft") Load_MoveFT(name, info, envreqs, feedback);
  }

  // Communication Tasks
  if (avd_tasklib_is_comm_name((const char*) name) != 0) {
  if (name == "comm_echo") {
    NewTask(name, "Echo of Neighbor's Input", &cTaskLib::Task_CommEcho, REQ_NEIGHBOR_INPUT);
  } else if (name == "comm_not") {
    NewTask(name, "Not of Neighbor's Input", &cTaskLib::Task_CommNot, REQ_NEIGHBOR_INPUT);
  }
  } // end comm name gate
  
  // Movement Tasks
  if (avd_tasklib_is_movement_name((const char*) name) != 0) {
  if (name == "move_up_gradient") NewTask(name, "Move up gradient", &cTaskLib::Task_MoveUpGradient);
  else if (name == "move_neutral_gradient") NewTask(name, "Move neutral gradient", &cTaskLib::Task_MoveNeutralGradient);
  else if (name == "move_down_gradient") NewTask(name, "Move down gradient", &cTaskLib::Task_MoveDownGradient);
  else if (name == "move_not_up_gradient") NewTask(name, "Move not up gradient", &cTaskLib::Task_MoveNotUpGradient);
  else if (name == "move_to_right_side") NewTask(name, "Move to right side", &cTaskLib::Task_MoveToRightSide);
  else if (name == "move_to_left_side") NewTask(name, "Move to left side", &cTaskLib::Task_MoveToLeftSide);
  // BDC Movement Tasks
  else if (name == "move") NewTask(name, "Successfully Moved", &cTaskLib::Task_Move);
  else if (name == "movetotarget") NewTask(name, "Move to a target area", &cTaskLib::Task_MoveToTarget);
  else if (name == "movetoevent") NewTask(name, "Move to a target area", &cTaskLib::Task_MoveToMovementEvent);
  else if (name == "movebetweenevent") NewTask(name, "Move to a target area", &cTaskLib::Task_MoveBetweenMovementEvent);

  // reputation based tasks
  else if (name == "perfect_strings") NewTask(name, "Produce and store perfect strings", &cTaskLib::Task_CreatePerfectStrings);
  } // end movement name gate

  // event tasks
  if (avd_tasklib_is_event_name((const char*) name) != 0) {
  if (name == "move_to_event") NewTask(name, "Moved into cell containing event", &cTaskLib::Task_MoveToEvent);
  else if (name == "event_killed") NewTask(name, "Killed event", &cTaskLib::Task_EventKilled);
  } // end event name gate
  
  //Altruism
  if (avd_tasklib_is_altruism_name((const char*) name) != 0) {
  if (name == "exploded") NewTask(name, "Organism exploded", &cTaskLib::Task_Exploded);
  else if (name == "exploded2") NewTask(name, "Organism exploded", &cTaskLib::Task_Exploded2);
  else if (name == "consume-public-good") NewTask(name, "Public good consumed", &cTaskLib::Task_ConsumePublicGood);
  else if (name == "ai-display-cost") NewTask(name, "Autoinducer cost paid", &cTaskLib::Task_AIDisplayCost);
  else if (name == "produce-public-good") NewTask(name, "Public good produced", &cTaskLib::Task_ProducePublicGood);
  } // end altruism name gate

  // String matching
  if (name == "all-ones") Load_AllOnes(name, info, envreqs, feedback);
  else if (name == "royal-road") Load_RoyalRoad(name, info, envreqs, feedback);
  else if (name == "royal-road-wd") Load_RoyalRoadWithDitches(name, info, envreqs, feedback);
  
  // Division of labor
  if (name == "opinion_is")  Load_OpinionIs(name, info, envreqs, feedback);
  
  // Make sure we have actually found a task  
  if (task_array.GetSize() == start_size) {
    feedback.Error("unknown/unprocessed task entry '%s'", (const char*)name);
    return NULL;
  }
  
  // And return the found task.
  return task_array[start_size];
}

void cTaskLib::NewTask(const cString& name, const cString& desc, tTaskTest task_fun, int reqs, cArgContainer* args)
{
  if (reqs & REQ_NEIGHBOR_INPUT) use_neighbor_input = true;
  if (reqs & REQ_NEIGHBOR_OUTPUT) use_neighbor_output = true;
  
  const int id = task_array.GetSize();
  task_array.Resize(id + 1);
  task_array[id] = new cTaskEntry(name, desc, id, task_fun, args);
}


void cTaskLib::SetupTests(cTaskContext& ctx) const
{
  const tBuffer<int>& input_buffer = ctx.GetInputBuffer();
  const int num_inputs = input_buffer.GetNumStored();
  int test_inputs[3];
  for (int i = 0; i < 3; i++) {
    test_inputs[i] = (num_inputs > i) ? input_buffer[i] : 0;
  }

  int test_output = 0;
  if (ctx.GetOutputBuffer().GetNumStored()) test_output = ctx.GetOutputBuffer()[0];

  ctx.SetLogicId(avd_task_compute_logic_id(test_inputs[0], test_inputs[1], test_inputs[2], num_inputs, test_output));
}


double cTaskLib::Task_Echo(cTaskContext& ctx) const
{
  const tBuffer<int>& input_buffer = ctx.GetInputBuffer();
  const int num_inputs = input_buffer.GetNumStored();
  const int test_output = ctx.GetOutputBuffer()[0];
  // tBuffer is a circular buffer; extract into contiguous array for Rust.
  int inputs[16]; // Avida buffers are typically 3; 16 covers all practical cases.
  const int n = (num_inputs < 16) ? num_inputs : 16;
  for (int i = 0; i < n; i++) inputs[i] = input_buffer[i];
  double result = avd_task_eval_echo(inputs, n, test_output);
  if (result > 0.0) {
    assert(ctx.GetLogicId() == 170 || ctx.GetLogicId() == 204 || ctx.GetLogicId() == 240);
  }
  return result;
}


double cTaskLib::Task_Add(cTaskContext& ctx) const
{
  int inputs[3]; int n = extract_inputs(ctx.GetInputBuffer(), inputs, 3);
  return avd_task_eval_simple_arith(inputs, n, ctx.GetOutputBuffer()[0], ARITH_ADD);
}


double cTaskLib::Task_Add3(cTaskContext& ctx) const
{
  int inputs[3]; int n = extract_inputs(ctx.GetInputBuffer(), inputs, 3);
  return avd_task_eval_simple_arith(inputs, n, ctx.GetOutputBuffer()[0], ARITH_ADD3);
}


double cTaskLib::Task_Sub(cTaskContext& ctx) const
{
  int inputs[3]; int n = extract_inputs(ctx.GetInputBuffer(), inputs, 3);
  return avd_task_eval_simple_arith(inputs, n, ctx.GetOutputBuffer()[0], ARITH_SUB);
}

// @WRE DontCare task always succeeds.
double cTaskLib::Task_DontCare(cTaskContext& ctx) const
{
  ctx.EnsureSnapshot();
  return avd_task_ctx_dont_care(&ctx.GetSnapshot());
}

double cTaskLib::Task_Not(cTaskContext& ctx) const
{
  return avd_task_eval_logic(AVD_TASK_NOT, ctx.GetLogicId());
}

double cTaskLib::Task_Nand(cTaskContext& ctx) const
{
  return avd_task_eval_logic(AVD_TASK_NAND, ctx.GetLogicId());
}

double cTaskLib::Task_And(cTaskContext& ctx) const
{
  return avd_task_eval_logic(AVD_TASK_AND, ctx.GetLogicId());
}

double cTaskLib::Task_OrNot(cTaskContext& ctx) const
{
  return avd_task_eval_logic(AVD_TASK_ORNOT, ctx.GetLogicId());
}

double cTaskLib::Task_Or(cTaskContext& ctx) const
{
  return avd_task_eval_logic(AVD_TASK_OR, ctx.GetLogicId());
}

double cTaskLib::Task_AndNot(cTaskContext& ctx) const
{
  return avd_task_eval_logic(AVD_TASK_ANDNOT, ctx.GetLogicId());
}

double cTaskLib::Task_Nor(cTaskContext& ctx) const
{
  return avd_task_eval_logic(AVD_TASK_NOR, ctx.GetLogicId());
}

double cTaskLib::Task_Xor(cTaskContext& ctx) const
{
  return avd_task_eval_logic(AVD_TASK_XOR, ctx.GetLogicId());
}

double cTaskLib::Task_Equ(cTaskContext& ctx) const
{
  return avd_task_eval_logic(AVD_TASK_EQU, ctx.GetLogicId());
}


double cTaskLib::Task_Nand_ResourceDependent(cTaskContext& ctx) const
{
  const int logic_id = ctx.GetLogicId();
  // Resolve pheromone resource amount — C++ handles resource lookup
  double pher_amount = 0.0;
  const cResourceLib& resLib = m_world->GetEnvironment().GetResourceLib();
  const cResourceCount& resource_count = m_world->GetPopulation().GetResourceCount();
  if (resource_count.GetSize() > 0) {
    cResource* res = resLib.GetResource("pheromone");
    if (res && strncmp(resource_count.GetResName(res->GetID()), "pheromone", 9) == 0) {
      const AvidaArray<double>& ra = ctx.GetOrganism()->GetOrgInterface().GetResources(m_world->GetDefaultContext());
      pher_amount = ra[res->GetID()];
    }
  }
  return avd_task_eval_nand_res_dep(logic_id, pher_amount);
}


double cTaskLib::Task_Nor_ResourceDependent(cTaskContext& ctx) const
{
  const int logic_id = ctx.GetLogicId();
  // Resolve pheromone resource amount — C++ handles resource lookup
  double pher_amount = 0.0;
  const cResourceLib& resLib = m_world->GetEnvironment().GetResourceLib();
  const cResourceCount& resource_count = m_world->GetPopulation().GetResourceCount();
  if (resource_count.GetSize() > 0) {
    cResource* res = resLib.GetResource("pheromone");
    if (res && strncmp(resource_count.GetResName(res->GetID()), "pheromone", 9) == 0) {
      const AvidaArray<double>& ra = ctx.GetOrganism()->GetOrgInterface().GetResources(m_world->GetDefaultContext());
      pher_amount = ra[res->GetID()];
    }
  }
  return avd_task_eval_nor_res_dep(logic_id, pher_amount);
}


double cTaskLib::Task_Logic3in_AA(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in(ctx.GetLogicId(), 1);
}

double cTaskLib::Task_Logic3in_AB(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in(ctx.GetLogicId(), 22);
}

double cTaskLib::Task_Logic3in_AC(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in(ctx.GetLogicId(), 23);
}

double cTaskLib::Task_Logic3in_AD(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in(ctx.GetLogicId(), 104);
}

double cTaskLib::Task_Logic3in_AE(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in(ctx.GetLogicId(), 105);
}

double cTaskLib::Task_Logic3in_AF(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in(ctx.GetLogicId(), 126);
}

double cTaskLib::Task_Logic3in_AG(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in(ctx.GetLogicId(), 127);
}

double cTaskLib::Task_Logic3in_AH(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in(ctx.GetLogicId(), 128);
}

double cTaskLib::Task_Logic3in_AI(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in(ctx.GetLogicId(), 129);
}

double cTaskLib::Task_Logic3in_AJ(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in(ctx.GetLogicId(), 150);
}

double cTaskLib::Task_Logic3in_AK(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in(ctx.GetLogicId(), 151);
}

double cTaskLib::Task_Logic3in_AL(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in(ctx.GetLogicId(), 232);
}

double cTaskLib::Task_Logic3in_AM(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in(ctx.GetLogicId(), 233);
}

double cTaskLib::Task_Logic3in_AN(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in(ctx.GetLogicId(), 254);
}

double cTaskLib::Task_Logic3in_AO(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in_3(ctx.GetLogicId(), 2, 4, 16);
}

double cTaskLib::Task_Logic3in_AP(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in_3(ctx.GetLogicId(), 6, 18, 20);
}

double cTaskLib::Task_Logic3in_AQ(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in_3(ctx.GetLogicId(), 7, 19, 21);
}

double cTaskLib::Task_Logic3in_AR(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in_3(ctx.GetLogicId(), 8, 32, 64);
}

double cTaskLib::Task_Logic3in_AS(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in_3(ctx.GetLogicId(), 9, 33, 65);
}

double cTaskLib::Task_Logic3in_AT(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in_3(ctx.GetLogicId(), 14, 50, 84);
}

double cTaskLib::Task_Logic3in_AU(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in_3(ctx.GetLogicId(), 24, 36, 66);
}

double cTaskLib::Task_Logic3in_AV(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in_3(ctx.GetLogicId(), 25, 37, 67);
}

double cTaskLib::Task_Logic3in_AW(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in_3(ctx.GetLogicId(), 30, 54, 86);
}

double cTaskLib::Task_Logic3in_AX(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in_3(ctx.GetLogicId(), 31, 55, 87);
}

double cTaskLib::Task_Logic3in_AY(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in_3(ctx.GetLogicId(), 40, 72, 96);
}

double cTaskLib::Task_Logic3in_AZ(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in_3(ctx.GetLogicId(), 41, 73, 97);
}

double cTaskLib::Task_Logic3in_BA(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in_3(ctx.GetLogicId(), 42, 76, 112);
}

double cTaskLib::Task_Logic3in_BB(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in_3(ctx.GetLogicId(), 43, 77, 113);
}

double cTaskLib::Task_Logic3in_BC(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in_3(ctx.GetLogicId(), 61, 91, 103);
}

double cTaskLib::Task_Logic3in_BD(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in_3(ctx.GetLogicId(), 62, 94, 118);
}

double cTaskLib::Task_Logic3in_BE(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in_3(ctx.GetLogicId(), 106, 108, 120);
}

double cTaskLib::Task_Logic3in_BF(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in_3(ctx.GetLogicId(), 107, 109, 121);
}

double cTaskLib::Task_Logic3in_BG(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in_3(ctx.GetLogicId(), 110, 122, 124);
}

double cTaskLib::Task_Logic3in_BH(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in_3(ctx.GetLogicId(), 111, 123, 125);
}

double cTaskLib::Task_Logic3in_BI(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in_3(ctx.GetLogicId(), 130, 132, 144);
}

double cTaskLib::Task_Logic3in_BJ(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in_3(ctx.GetLogicId(), 131, 133, 145);
}

double cTaskLib::Task_Logic3in_BK(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in_3(ctx.GetLogicId(), 134, 146, 148);
}

double cTaskLib::Task_Logic3in_BL(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in_3(ctx.GetLogicId(), 135, 147, 149);
}

double cTaskLib::Task_Logic3in_BM(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in_3(ctx.GetLogicId(), 137, 161, 193);
}

double cTaskLib::Task_Logic3in_BN(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in_3(ctx.GetLogicId(), 142, 178, 212);
}

double cTaskLib::Task_Logic3in_BO(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in_3(ctx.GetLogicId(), 143, 179, 213);
}

double cTaskLib::Task_Logic3in_BP(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in_3(ctx.GetLogicId(), 152, 164, 194);
}

double cTaskLib::Task_Logic3in_BQ(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in_3(ctx.GetLogicId(), 158, 182, 214);
}

double cTaskLib::Task_Logic3in_BR(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in_3(ctx.GetLogicId(), 159, 183, 215);
}

double cTaskLib::Task_Logic3in_BS(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in_3(ctx.GetLogicId(), 168, 200, 224);
}

double cTaskLib::Task_Logic3in_BT(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in_3(ctx.GetLogicId(), 169, 201, 225);
}

double cTaskLib::Task_Logic3in_BU(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in_3(ctx.GetLogicId(), 171, 205, 241);
}

double cTaskLib::Task_Logic3in_BV(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in_3(ctx.GetLogicId(), 188, 218, 230);
}

double cTaskLib::Task_Logic3in_BW(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in_3(ctx.GetLogicId(), 189, 219, 231);
}

double cTaskLib::Task_Logic3in_BX(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in_3(ctx.GetLogicId(), 190, 222, 246);
}

double cTaskLib::Task_Logic3in_BY(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in_3(ctx.GetLogicId(), 191, 223, 247);
}

double cTaskLib::Task_Logic3in_BZ(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in_3(ctx.GetLogicId(), 234, 236, 248);
}

double cTaskLib::Task_Logic3in_CA(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in_3(ctx.GetLogicId(), 235, 237, 249);
}

double cTaskLib::Task_Logic3in_CB(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in_3(ctx.GetLogicId(), 239, 251, 253);
}

double cTaskLib::Task_Logic3in_CC(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in_6(ctx.GetLogicId(), 11, 13, 35, 49, 69, 81);
}

double cTaskLib::Task_Logic3in_CD(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in_6(ctx.GetLogicId(), 26, 28, 38, 52, 70, 82);
}

double cTaskLib::Task_Logic3in_CE(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in_6(ctx.GetLogicId(), 27, 29, 39, 53, 71, 83);
}

double cTaskLib::Task_Logic3in_CF(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in_6(ctx.GetLogicId(), 44, 56, 74, 88, 98, 100);
}

double cTaskLib::Task_Logic3in_CG(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in_6(ctx.GetLogicId(), 45, 57, 75, 89, 99, 101);
}

double cTaskLib::Task_Logic3in_CH(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in_6(ctx.GetLogicId(), 46, 58, 78, 92, 114, 116);
}

double cTaskLib::Task_Logic3in_CI(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in_6(ctx.GetLogicId(), 47, 59, 79, 93, 115, 117);
}

double cTaskLib::Task_Logic3in_CJ(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in_6(ctx.GetLogicId(), 138, 140, 162, 176, 196, 208);
}

double cTaskLib::Task_Logic3in_CK(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in_6(ctx.GetLogicId(), 139, 141, 163, 177, 197, 209);
}

double cTaskLib::Task_Logic3in_CL(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in_6(ctx.GetLogicId(), 154, 156, 166, 180, 198, 210);
}

double cTaskLib::Task_Logic3in_CM(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in_6(ctx.GetLogicId(), 155, 157, 167, 181, 199, 211);
}

double cTaskLib::Task_Logic3in_CN(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in_6(ctx.GetLogicId(), 172, 184, 202, 216, 226, 228);
}

double cTaskLib::Task_Logic3in_CO(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in_6(ctx.GetLogicId(), 173, 185, 203, 217, 227, 229);
}

double cTaskLib::Task_Logic3in_CP(cTaskContext& ctx) const
{
  return avd_task_eval_logic3in_6(ctx.GetLogicId(), 174, 186, 206, 220, 242, 244);
}

double cTaskLib::Task_Math1in_AA(cTaskContext& ctx) const //(2X)
{
  int inputs[3]; int n = extract_inputs(ctx.GetInputBuffer(), inputs, 3);
  return avd_task_eval_math1in(inputs, n, ctx.GetOutputBuffer()[0], MATH1IN_AA);
}

double cTaskLib::Task_Math1in_AB(cTaskContext& ctx) const //(2X/3)
{
  int inputs[3]; int n = extract_inputs(ctx.GetInputBuffer(), inputs, 3);
  return avd_task_eval_math1in(inputs, n, ctx.GetOutputBuffer()[0], MATH1IN_AB);
}

double cTaskLib::Task_Math1in_AC(cTaskContext& ctx) const //(5X/4)
{
  int inputs[3]; int n = extract_inputs(ctx.GetInputBuffer(), inputs, 3);
  return avd_task_eval_math1in(inputs, n, ctx.GetOutputBuffer()[0], MATH1IN_AC);
}

double cTaskLib::Task_Math1in_AD(cTaskContext& ctx) const //(X^2)
{
  int inputs[3]; int n = extract_inputs(ctx.GetInputBuffer(), inputs, 3);
  return avd_task_eval_math1in(inputs, n, ctx.GetOutputBuffer()[0], MATH1IN_AD);
}

double cTaskLib::Task_Math1in_AE(cTaskContext& ctx) const //(X^3)
{
  int inputs[3]; int n = extract_inputs(ctx.GetInputBuffer(), inputs, 3);
  return avd_task_eval_math1in(inputs, n, ctx.GetOutputBuffer()[0], MATH1IN_AE);
}

double cTaskLib::Task_Math1in_AF(cTaskContext& ctx) const //(sqrt(X)
{
  int inputs[3]; int n = extract_inputs(ctx.GetInputBuffer(), inputs, 3);
  return avd_task_eval_math1in(inputs, n, ctx.GetOutputBuffer()[0], MATH1IN_AF);
}

double cTaskLib::Task_Math1in_AG(cTaskContext& ctx) const //(log(X))
{
  int inputs[3]; int n = extract_inputs(ctx.GetInputBuffer(), inputs, 3);
  return avd_task_eval_math1in(inputs, n, ctx.GetOutputBuffer()[0], MATH1IN_AG);
}

double cTaskLib::Task_Math1in_AH(cTaskContext& ctx) const //(X^2+X^3)
{
  int inputs[3]; int n = extract_inputs(ctx.GetInputBuffer(), inputs, 3);
  return avd_task_eval_math1in(inputs, n, ctx.GetOutputBuffer()[0], MATH1IN_AH);
}

double cTaskLib::Task_Math1in_AI(cTaskContext& ctx) const // (X^2 + sqrt(X))
{
  int inputs[3]; int n = extract_inputs(ctx.GetInputBuffer(), inputs, 3);
  return avd_task_eval_math1in(inputs, n, ctx.GetOutputBuffer()[0], MATH1IN_AI);
}

double cTaskLib::Task_Math1in_AJ(cTaskContext& ctx) const // abs(X)
{
  int inputs[3]; int n = extract_inputs(ctx.GetInputBuffer(), inputs, 3);
  return avd_task_eval_math1in(inputs, n, ctx.GetOutputBuffer()[0], MATH1IN_AJ);
}

double cTaskLib::Task_Math1in_AK(cTaskContext& ctx) const //(X-5)
{
  int inputs[3]; int n = extract_inputs(ctx.GetInputBuffer(), inputs, 3);
  return avd_task_eval_math1in(inputs, n, ctx.GetOutputBuffer()[0], MATH1IN_AK);
}

double cTaskLib::Task_Math1in_AL(cTaskContext& ctx) const //(-X)
{
  int inputs[3]; int n = extract_inputs(ctx.GetInputBuffer(), inputs, 3);
  return avd_task_eval_math1in(inputs, n, ctx.GetOutputBuffer()[0], MATH1IN_AL);
}

double cTaskLib::Task_Math1in_AM(cTaskContext& ctx) const //(5X)
{
  int inputs[3]; int n = extract_inputs(ctx.GetInputBuffer(), inputs, 3);
  return avd_task_eval_math1in(inputs, n, ctx.GetOutputBuffer()[0], MATH1IN_AM);
}

double cTaskLib::Task_Math1in_AN(cTaskContext& ctx) const //(X/4)
{
  int inputs[3]; int n = extract_inputs(ctx.GetInputBuffer(), inputs, 3);
  return avd_task_eval_math1in(inputs, n, ctx.GetOutputBuffer()[0], MATH1IN_AN);
}

double cTaskLib::Task_Math1in_AO(cTaskContext& ctx) const //(X-6)
{
  int inputs[3]; int n = extract_inputs(ctx.GetInputBuffer(), inputs, 3);
  return avd_task_eval_math1in(inputs, n, ctx.GetOutputBuffer()[0], MATH1IN_AO);
}

double cTaskLib::Task_Math1in_AP(cTaskContext& ctx) const //(X-7)
{
  int inputs[3]; int n = extract_inputs(ctx.GetInputBuffer(), inputs, 3);
  return avd_task_eval_math1in(inputs, n, ctx.GetOutputBuffer()[0], MATH1IN_AP);
}

double cTaskLib::Task_Math1in_AS(cTaskContext& ctx) const //3Y
{
  int inputs[3]; int n = extract_inputs(ctx.GetInputBuffer(), inputs, 3);
  return avd_task_eval_math1in(inputs, n, ctx.GetOutputBuffer()[0], MATH1IN_AS);
}

double cTaskLib::Task_Math2in_AA(cTaskContext& ctx) const //(sqrt(X+Y))
{
  int inputs[3]; int n = extract_inputs(ctx.GetInputBuffer(), inputs, 3);
  return avd_task_eval_math2in(inputs, n, ctx.GetOutputBuffer()[0], MATH2IN_AA);
}

double cTaskLib::Task_Math2in_AB(cTaskContext& ctx) const  //((X+Y)^2)
{
  int inputs[3]; int n = extract_inputs(ctx.GetInputBuffer(), inputs, 3);
  return avd_task_eval_math2in(inputs, n, ctx.GetOutputBuffer()[0], MATH2IN_AB);
}

double cTaskLib::Task_Math2in_AC(cTaskContext& ctx) const //(X%Y)
{
  int inputs[3]; int n = extract_inputs(ctx.GetInputBuffer(), inputs, 3);
  return avd_task_eval_math2in(inputs, n, ctx.GetOutputBuffer()[0], MATH2IN_AC);
}

double cTaskLib::Task_Math2in_AD(cTaskContext& ctx) const //(3X/2+5Y/4)
{
  int inputs[3]; int n = extract_inputs(ctx.GetInputBuffer(), inputs, 3);
  return avd_task_eval_math2in(inputs, n, ctx.GetOutputBuffer()[0], MATH2IN_AD);
}

double cTaskLib::Task_Math2in_AE(cTaskContext& ctx) const //(abs(X-5)+abs(Y-6))
{
  int inputs[3]; int n = extract_inputs(ctx.GetInputBuffer(), inputs, 3);
  return avd_task_eval_math2in(inputs, n, ctx.GetOutputBuffer()[0], MATH2IN_AE);
}

double cTaskLib::Task_Math2in_AF(cTaskContext& ctx) const //(XY-X/Y)
{
  int inputs[3]; int n = extract_inputs(ctx.GetInputBuffer(), inputs, 3);
  return avd_task_eval_math2in(inputs, n, ctx.GetOutputBuffer()[0], MATH2IN_AF);
}

double cTaskLib::Task_Math2in_AG(cTaskContext& ctx) const //((X-Y)^2)
{
  int inputs[3]; int n = extract_inputs(ctx.GetInputBuffer(), inputs, 3);
  return avd_task_eval_math2in(inputs, n, ctx.GetOutputBuffer()[0], MATH2IN_AG);
}

double cTaskLib::Task_Math2in_AH(cTaskContext& ctx) const //(X^2+Y^2)
{
  int inputs[3]; int n = extract_inputs(ctx.GetInputBuffer(), inputs, 3);
  return avd_task_eval_math2in(inputs, n, ctx.GetOutputBuffer()[0], MATH2IN_AH);
}

double cTaskLib::Task_Math2in_AI(cTaskContext& ctx) const //(X^2+Y^3)
{
  int inputs[3]; int n = extract_inputs(ctx.GetInputBuffer(), inputs, 3);
  return avd_task_eval_math2in(inputs, n, ctx.GetOutputBuffer()[0], MATH2IN_AI);
}

double cTaskLib::Task_Math2in_AJ(cTaskContext& ctx) const //((sqrt(X)+Y)/(X-7))
{
  int inputs[3]; int n = extract_inputs(ctx.GetInputBuffer(), inputs, 3);
  return avd_task_eval_math2in(inputs, n, ctx.GetOutputBuffer()[0], MATH2IN_AJ);
}

double cTaskLib::Task_Math2in_AK(cTaskContext& ctx) const //(log(|X/Y|))
{
  int inputs[3]; int n = extract_inputs(ctx.GetInputBuffer(), inputs, 3);
  return avd_task_eval_math2in(inputs, n, ctx.GetOutputBuffer()[0], MATH2IN_AK);
}

double cTaskLib::Task_Math2in_AL(cTaskContext& ctx) const //(log(|X|)/Y)
{
  int inputs[3]; int n = extract_inputs(ctx.GetInputBuffer(), inputs, 3);
  return avd_task_eval_math2in(inputs, n, ctx.GetOutputBuffer()[0], MATH2IN_AL);
}

double cTaskLib::Task_Math2in_AM(cTaskContext& ctx) const //(X/log(|Y|))
{
  int inputs[3]; int n = extract_inputs(ctx.GetInputBuffer(), inputs, 3);
  return avd_task_eval_math2in(inputs, n, ctx.GetOutputBuffer()[0], MATH2IN_AM);
}

double cTaskLib::Task_Math2in_AN(cTaskContext& ctx) const //(X+Y)
{
  int inputs[3]; int n = extract_inputs(ctx.GetInputBuffer(), inputs, 3);
  return avd_task_eval_math2in(inputs, n, ctx.GetOutputBuffer()[0], MATH2IN_AN);
}

double cTaskLib::Task_Math2in_AO(cTaskContext& ctx) const //(X-Y)
{
  int inputs[3]; int n = extract_inputs(ctx.GetInputBuffer(), inputs, 3);
  return avd_task_eval_math2in(inputs, n, ctx.GetOutputBuffer()[0], MATH2IN_AO);
}

double cTaskLib::Task_Math2in_AP(cTaskContext& ctx) const //(X/Y)
{
  int inputs[3]; int n = extract_inputs(ctx.GetInputBuffer(), inputs, 3);
  return avd_task_eval_math2in(inputs, n, ctx.GetOutputBuffer()[0], MATH2IN_AP);
}

double cTaskLib::Task_Math2in_AQ(cTaskContext& ctx) const //(XY)
{
  int inputs[3]; int n = extract_inputs(ctx.GetInputBuffer(), inputs, 3);
  return avd_task_eval_math2in(inputs, n, ctx.GetOutputBuffer()[0], MATH2IN_AQ);
}

double cTaskLib::Task_Math2in_AR(cTaskContext& ctx) const //(sqrt(X)+sqrt(Y))
{
  int inputs[3]; int n = extract_inputs(ctx.GetInputBuffer(), inputs, 3);
  return avd_task_eval_math2in(inputs, n, ctx.GetOutputBuffer()[0], MATH2IN_AR);
}

double cTaskLib::Task_Math2in_AS(cTaskContext& ctx) const //(X+2Y)
{
  int inputs[3]; int n = extract_inputs(ctx.GetInputBuffer(), inputs, 3);
  return avd_task_eval_math2in(inputs, n, ctx.GetOutputBuffer()[0], MATH2IN_AS);
}

double cTaskLib::Task_Math2in_AT(cTaskContext& ctx) const //(X+3Y)
{
  int inputs[3]; int n = extract_inputs(ctx.GetInputBuffer(), inputs, 3);
  return avd_task_eval_math2in(inputs, n, ctx.GetOutputBuffer()[0], MATH2IN_AT);
}

double cTaskLib::Task_Math2in_AU(cTaskContext& ctx) const //(2X+3Y)
{
  int inputs[3]; int n = extract_inputs(ctx.GetInputBuffer(), inputs, 3);
  return avd_task_eval_math2in(inputs, n, ctx.GetOutputBuffer()[0], MATH2IN_AU);
}

double cTaskLib::Task_Math2in_AV(cTaskContext& ctx) const //(XY^2)
{
  int inputs[3]; int n = extract_inputs(ctx.GetInputBuffer(), inputs, 3);
  return avd_task_eval_math2in(inputs, n, ctx.GetOutputBuffer()[0], MATH2IN_AV);
}

double cTaskLib::Task_Math2in_AX(cTaskContext& ctx) const //(X+3Y)
{
  int inputs[3]; int n = extract_inputs(ctx.GetInputBuffer(), inputs, 3);
  return avd_task_eval_math2in(inputs, n, ctx.GetOutputBuffer()[0], MATH2IN_AX);
}

double cTaskLib::Task_Math2in_AY(cTaskContext& ctx) const //(2A+B)
{
  int inputs[3]; int n = extract_inputs(ctx.GetInputBuffer(), inputs, 3);
  return avd_task_eval_math2in(inputs, n, ctx.GetOutputBuffer()[0], MATH2IN_AY);
}

double cTaskLib::Task_Math2in_AZ(cTaskContext& ctx) const //(4A+6B)
{
  int inputs[3]; int n = extract_inputs(ctx.GetInputBuffer(), inputs, 3);
  return avd_task_eval_math2in(inputs, n, ctx.GetOutputBuffer()[0], MATH2IN_AZ);
}

double cTaskLib::Task_Math2in_AAA(cTaskContext& ctx) const //(3A-2B)
{
  int inputs[3]; int n = extract_inputs(ctx.GetInputBuffer(), inputs, 3);
  return avd_task_eval_math2in(inputs, n, ctx.GetOutputBuffer()[0], MATH2IN_AAA);
}

double cTaskLib::Task_Math3in_AA(cTaskContext& ctx) const //(X^2+Y^2+Z^2)
{
  int inputs[3]; int n = extract_inputs(ctx.GetInputBuffer(), inputs, 3);
  return avd_task_eval_math3in(inputs, n, ctx.GetOutputBuffer()[0], MATH3IN_AA);
}

double cTaskLib::Task_Math3in_AB(cTaskContext& ctx) const //(sqrt(X)+sqrt(Y)+sqrt(Z))
{
  int inputs[3]; int n = extract_inputs(ctx.GetInputBuffer(), inputs, 3);
  return avd_task_eval_math3in(inputs, n, ctx.GetOutputBuffer()[0], MATH3IN_AB);
}

double cTaskLib::Task_Math3in_AC(cTaskContext& ctx) const //(X+2Y+3Z)
{
  int inputs[3]; int n = extract_inputs(ctx.GetInputBuffer(), inputs, 3);
  return avd_task_eval_math3in(inputs, n, ctx.GetOutputBuffer()[0], MATH3IN_AC);
}

double cTaskLib::Task_Math3in_AD(cTaskContext& ctx) const //(XY^2+Z^3)
{
  int inputs[3]; int n = extract_inputs(ctx.GetInputBuffer(), inputs, 3);
  return avd_task_eval_math3in(inputs, n, ctx.GetOutputBuffer()[0], MATH3IN_AD);
}

double cTaskLib::Task_Math3in_AE(cTaskContext& ctx) const //((X%Y)*Z)
{
  int inputs[3]; int n = extract_inputs(ctx.GetInputBuffer(), inputs, 3);
  return avd_task_eval_math3in(inputs, n, ctx.GetOutputBuffer()[0], MATH3IN_AE);
}

double cTaskLib::Task_Math3in_AF(cTaskContext& ctx) const //((X+Y)^2+sqrt(Y+Z))
{
  int inputs[3]; int n = extract_inputs(ctx.GetInputBuffer(), inputs, 3);
  return avd_task_eval_math3in(inputs, n, ctx.GetOutputBuffer()[0], MATH3IN_AF);
}

double cTaskLib::Task_Math3in_AG(cTaskContext& ctx) const //((XY)%(YZ))
{
  int inputs[3]; int n = extract_inputs(ctx.GetInputBuffer(), inputs, 3);
  return avd_task_eval_math3in(inputs, n, ctx.GetOutputBuffer()[0], MATH3IN_AG);
}

double cTaskLib::Task_Math3in_AH(cTaskContext& ctx) const //(X+Y+Z)
{
  int inputs[3]; int n = extract_inputs(ctx.GetInputBuffer(), inputs, 3);
  return avd_task_eval_math3in(inputs, n, ctx.GetOutputBuffer()[0], MATH3IN_AH);
}

double cTaskLib::Task_Math3in_AI(cTaskContext& ctx) const //(-X-Y-Z)
{
  int inputs[3]; int n = extract_inputs(ctx.GetInputBuffer(), inputs, 3);
  return avd_task_eval_math3in(inputs, n, ctx.GetOutputBuffer()[0], MATH3IN_AI);
}

double cTaskLib::Task_Math3in_AJ(cTaskContext& ctx) const //((X-Y)^2+(Y-Z)^2+(Z-X)^2)
{
  int inputs[3]; int n = extract_inputs(ctx.GetInputBuffer(), inputs, 3);
  return avd_task_eval_math3in(inputs, n, ctx.GetOutputBuffer()[0], MATH3IN_AJ);
}

double cTaskLib::Task_Math3in_AK(cTaskContext& ctx) const //((X+Y)^2+(Y+Z)^2+(Z+X)^2)
{
  int inputs[3]; int n = extract_inputs(ctx.GetInputBuffer(), inputs, 3);
  return avd_task_eval_math3in(inputs, n, ctx.GetOutputBuffer()[0], MATH3IN_AK);
}

double cTaskLib::Task_Math3in_AL(cTaskContext& ctx) const //((X-Y)^2+(X-Z)^2)
{
  int inputs[3]; int n = extract_inputs(ctx.GetInputBuffer(), inputs, 3);
  return avd_task_eval_math3in(inputs, n, ctx.GetOutputBuffer()[0], MATH3IN_AL);
}

double cTaskLib::Task_Math3in_AM(cTaskContext& ctx) const //((X+Y)^2+(Y+Z)^2)
{
  int inputs[3]; int n = extract_inputs(ctx.GetInputBuffer(), inputs, 3);
  return avd_task_eval_math3in(inputs, n, ctx.GetOutputBuffer()[0], MATH3IN_AM);
}

double cTaskLib::Task_Fib1(cTaskContext& ctx) const
{
  return avd_tasklib_fib_check(ctx.GetOutputBuffer()[0], 1);
}

double cTaskLib::Task_Fib2(cTaskContext& ctx) const
{
  return avd_tasklib_fib_check(ctx.GetOutputBuffer()[0], 2);
}

double cTaskLib::Task_Fib4(cTaskContext& ctx) const
{
  return avd_tasklib_fib_check(ctx.GetOutputBuffer()[0], 4);
}

double cTaskLib::Task_Fib5(cTaskContext& ctx) const
{
  return avd_tasklib_fib_check(ctx.GetOutputBuffer()[0], 5);
}

double cTaskLib::Task_Fib6(cTaskContext& ctx) const
{
  return avd_tasklib_fib_check(ctx.GetOutputBuffer()[0], 6);
}

double cTaskLib::Task_Fib7(cTaskContext& ctx) const
{
  return avd_tasklib_fib_check(ctx.GetOutputBuffer()[0], 7);
}

double cTaskLib::Task_Fib8(cTaskContext& ctx) const
{
  return avd_tasklib_fib_check(ctx.GetOutputBuffer()[0], 8);
}

double cTaskLib::Task_Fib9(cTaskContext& ctx) const
{
  return avd_tasklib_fib_check(ctx.GetOutputBuffer()[0], 9);
}

double cTaskLib::Task_Fib10(cTaskContext& ctx) const
{
  return avd_tasklib_fib_check(ctx.GetOutputBuffer()[0], 10);
}


void cTaskLib::Load_MatchStr(const cString& name, const cString& argstr, cEnvReqs& envreqs, Feedback& feedback)
{
  cArgSchema schema;
  schema.AddEntry("string", 0, cArgSchema::SCHEMA_STRING);
  schema.AddEntry("partial",0, 0);
  schema.AddEntry("binary",1,1);
  schema.AddEntry("pow",0,2.0);
  cArgContainer* args = cArgContainer::Load(argstr, schema, feedback);
  envreqs.SetMinOutputs(args->GetString(0).GetSize());
  if (args) NewTask(name, "MatchStr", &cTaskLib::Task_MatchStr, 0, args);
}

double cTaskLib::Task_MatchStr(cTaskContext& ctx) const
{
  tBuffer<int> temp_buf(ctx.GetOutputBuffer());
  const cString& string_to_match = ctx.GetTaskEntry()->GetArguments().GetString(0);
  int partial = ctx.GetTaskEntry()->GetArguments().GetInt(0);
  int binary = ctx.GetTaskEntry()->GetArguments().GetInt(1);

  // Flatten output buffer for Rust FFI
  int out_len = temp_buf.GetNumStored();
  std::vector<int> out_flat(out_len);
  for (int i = 0; i < out_len; i++) out_flat[i] = temp_buf[i];

  // Flatten received messages buffer if present
  std::vector<int> recv_flat;
  int recv_len = 0;
  if (ctx.GetReceivedMessages()) {
    tBuffer<int> received(*(ctx.GetReceivedMessages()));
    recv_len = received.GetNumStored();
    recv_flat.resize(recv_len);
    for (int i = 0; i < recv_len; i++) recv_flat[i] = received[i];
  }

  return avd_task_eval_match_str(
    out_flat.data(), out_len,
    recv_flat.empty() ? nullptr : recv_flat.data(), recv_len,
    (const unsigned char*)(const char*)string_to_match, string_to_match.GetSize(),
    partial, binary);
}

vector<cString> cTaskLib::GetMatchStrings()
{
  return m_strings;
}

cString cTaskLib::GetMatchString(int x)
{ 
  cString s; 
  if (x >= 0 && x < (int)m_strings.size()){
    s = m_strings[x]; 
  } else { 
    s = cString("");
  }
  
  return s;
}


void cTaskLib::Load_MatchProdStr(const cString& name, const cString& argstr, cEnvReqs& envreqs, Feedback& feedback)
{
  cArgSchema schema;
  schema.AddEntry("string", 0, cArgSchema::SCHEMA_STRING);		
  schema.AddEntry("partial",0, 0);
  schema.AddEntry("binary",1,1);
  schema.AddEntry("pow",0,2.0);
	schema.AddEntry("tag",2,-1);
  cArgContainer* args = cArgContainer::Load(argstr, schema, feedback);	
  envreqs.SetMinOutputs(args->GetString(0).GetSize());
	m_strings.push_back(args->GetString(0));
  if (args) NewTask(name, "MatchProdStr", &cTaskLib::Task_MatchStr, 0, args);
}


double cTaskLib::Task_MatchProdStr(cTaskContext& ctx) const
{
  // These even out the stats tracking.
  m_world->GetStats().AddTag(ctx.GetTaskEntry()->GetArguments().GetInt(2), 0);
  m_world->GetStats().AddTag(-1, 0);

  tBuffer<int> temp_buf(ctx.GetOutputBuffer());

  const cString& string_to_match = ctx.GetTaskEntry()->GetArguments().GetString(0);
  int partial = ctx.GetTaskEntry()->GetArguments().GetInt(0);
  int binary = ctx.GetTaskEntry()->GetArguments().GetInt(1);
  double mypow = ctx.GetTaskEntry()->GetArguments().GetDouble(0);

  // Extract output buffer to a contiguous array for Rust FFI
  const int buf_len = binary ? string_to_match.GetSize() : (temp_buf.GetNumStored() > 0 ? 1 : 0);
  std::vector<int> out_vec(buf_len);
  for (int i = 0; i < buf_len; i++) out_vec[i] = temp_buf[i];

  // Delegate core string matching to Rust
  int64_t packed = avd_task_match_prod_str_core(
    (const unsigned char*)(const char*)string_to_match,
    string_to_match.GetSize(),
    out_vec.data(), buf_len, binary);
  int max_num_matched = static_cast<int>(packed / 1000);
  int num_real = static_cast<int>(packed % 1000);

  // Check if the organism already produced this string.
  // If so, it receives a perfect score for this task.
  int tag = ctx.GetTaskEntry()->GetArguments().GetInt(2);

  if (m_world->GetConfig().MATCH_ALREADY_PRODUCED.Get()) {
    int prod = ctx.GetOrganism()->GetNumberStringsProduced(tag);
    if (prod) max_num_matched = string_to_match.GetSize();
  }

  // Update the organism's tag.
  ctx.GetOrganism()->UpdateTag(tag, max_num_matched);
  if (ctx.GetOrganism()->GetTagLabel() == tag) {
    ctx.GetOrganism()->SetLineageLabel(ctx.GetTaskEntry()->GetArguments().GetInt(2));
  }

  // Update stats
  cString name;
  name = "[produced";
  name += string_to_match;
  name += "]";
  m_world->GetStats().AddStringBitsMatchedValue(name, max_num_matched);

  // if the organism hasn't donated, then zero out its reputation.
  if ((ctx.GetOrganism()->GetReputation() > 0) &&
      (ctx.GetOrganism()->GetNumberOfDonations() == 0)) {
    ctx.GetOrganism()->SetReputation(0);
  }

  // Delegate bonus computation to Rust
  return avd_task_match_prod_str_bonus(
    max_num_matched, string_to_match.GetSize(), num_real,
    partial, mypow);
}


void cTaskLib::Load_MatchNumber(const cString& name, const cString& argstr, cEnvReqs&, Feedback& feedback)
{
  cArgSchema schema;
  
  // Integer Arguments
  schema.AddEntry("target", 0, cArgSchema::SCHEMA_INT);
  schema.AddEntry("threshold", 1, -1);
  // Double Arguments
  schema.AddEntry("halflife", 0, cArgSchema::SCHEMA_DOUBLE);
  
  cArgContainer* args = cArgContainer::Load(argstr, schema, feedback);
  if (args) NewTask(name, "Match Number", &cTaskLib::Task_MatchNumber, 0, args);
}

double cTaskLib::Task_MatchNumber(cTaskContext& ctx) const
{
  const cArgContainer& args = ctx.GetTaskEntry()->GetArguments();
  
  long long diff = ::llabs((long long)args.GetInt(0) - ctx.GetOutputBuffer()[0]);
  return avd_tasklib_threshold_halflife_quality(diff, args.GetInt(1), args.GetDouble(0));
}


void cTaskLib::Load_SortInputs(const cString& name, const cString& argstr, cEnvReqs& envreqs, Feedback& feedback)
{
  cArgSchema schema;
  
  // Integer Arguments
  schema.AddEntry("size", 0, cArgSchema::SCHEMA_INT); // Number of items to sort
  schema.AddEntry("direction", 1, 0); // < 0 = Descending, Otherwise = Ascending
  schema.AddEntry("contiguous", 2, 1); // 0 = No, Otherwise = Yes
  // Double Arguments
  schema.AddEntry("halflife", 0, cArgSchema::SCHEMA_DOUBLE);
  
  cArgContainer* args = cArgContainer::Load(argstr, schema, feedback);
  if (args) {
    envreqs.SetMinInputs(args->GetInt(0));
    envreqs.SetMinOutputs(args->GetInt(0) * 2);
    envreqs.SetTrueRandInputs();
    NewTask(name, "Sort Inputs", &cTaskLib::Task_SortInputs, 0, args);
  }
}

double cTaskLib::Task_SortInputs(cTaskContext& ctx) const
{
  const cArgContainer& args = ctx.GetTaskEntry()->GetArguments();
  const tBuffer<int>& output = ctx.GetOutputBuffer();
  const int size = args.GetInt(0);
  const int stored = output.GetNumStored();

  // if less than half, can't possibly reach threshold
  if (stored <= (size / 2)) return 0.0;

  // Flatten output buffer for Rust FFI
  std::vector<int> out_flat(stored);
  for (int i = 0; i < stored; i++) out_flat[i] = output[i];

  // Collect organism inputs for Rust FFI
  std::vector<int> in_flat(size);
  for (int i = 0; i < size; i++) in_flat[i] = ctx.GetOrganism()->GetInputAt(i);

  return avd_task_eval_sort_inputs(
    out_flat.data(), stored,
    in_flat.data(), size,
    size, args.GetInt(1), args.GetInt(2), args.GetDouble(0));
}




class cFibSeqState : public cTaskState {
public:
  int seq[2];
  int count;
  
  cFibSeqState() : count(0) { seq[0] = 1; seq[1] = 0; }
};


void cTaskLib::Load_FibonacciSequence(const cString& name, const cString& argstr, cEnvReqs&, Feedback& feedback)
{
  cArgSchema schema;
  
  // Integer Arguments
  schema.AddEntry("target", 0, cArgSchema::SCHEMA_INT);
  // Double Arguments
  schema.AddEntry("penalty", 0, 0.0);
  
  cArgContainer* args = cArgContainer::Load(argstr, schema, feedback);
  
  if (args) NewTask(name, "Fibonacci Sequence", &cTaskLib::Task_FibonacciSequence, 0, args);
}


double cTaskLib::Task_FibonacciSequence(cTaskContext& ctx) const
{
  const cArgContainer& args = ctx.GetTaskEntry()->GetArguments();
  cFibSeqState* state = static_cast<cFibSeqState*>(ctx.GetTaskState());
  if (state == NULL) {
    state = new cFibSeqState();
    ctx.AddTaskState(state);
  }
  return avd_task_eval_fibonacci_seq(
    state->seq, &state->count, ctx.GetOutputBuffer()[0],
    args.GetInt(0), args.GetDouble(0));
}


void cTaskLib::Load_Optimize(const cString& name, const cString& argstr, cEnvReqs& envreqs, Feedback& feedback)
{
  cArgSchema schema;
  
  // Integer Arguments
  schema.AddEntry("function", 0, cArgSchema::SCHEMA_INT);
  schema.AddEntry("binary", 1, 0);
  schema.AddEntry("varlength", 2, 8);
  schema.AddEntry("numvars", 3, 2);
  // Double Arguments
  schema.AddEntry("basepow", 0, 2.0);
  schema.AddEntry("maxFx", 1, 1.0);
  schema.AddEntry("minFx", 2, 0.0);
  schema.AddEntry("thresh", 3, -1.0);
  schema.AddEntry("threshMax", 4, -1.0);
  
  cArgContainer* args = cArgContainer::Load(argstr, schema, feedback);
  if (args) {
    if (args->GetInt(1)) {
      envreqs.SetMinOutputs(args->GetInt(2)*args->GetInt(3));
    }
    else {
      // once have ability to change args should in each of these cases change the max/min
      // to the appropriate defaults for this function
      switch (args->GetInt(0)) {
      case 1:
	envreqs.SetMinOutputs(1);
	break;
      case 2:
	envreqs.SetMinOutputs(2);
	break;
      case 3:
	envreqs.SetMinOutputs(2);
	break;
      default:
	envreqs.SetMinOutputs(2);
      };
    }
    
    NewTask(name, "Optimize", &cTaskLib::Task_Optimize, 0, args);
  }
}

double cTaskLib::Task_Optimize(cTaskContext& ctx) const
{
  // if the org hasn't output yet enough numbers, just return without completing any tasks
  if (ctx.GetOutputBuffer().GetNumStored() < ctx.GetOutputBuffer().GetCapacity()) return 0;

  const cArgContainer& args = ctx.GetTaskEntry()->GetArguments();
  const int function = args.GetInt(0);

  // Flatten output buffer for Rust FFI
  tBuffer<int> buf(ctx.GetOutputBuffer());
  int out_len = buf.GetNumStored();
  std::vector<int> out_flat(out_len);
  for (int i = 0; i < out_len; i++) out_flat[i] = buf[i];

  // Get string argument for function 20
  const unsigned char* str_ptr = nullptr;
  int str_len = 0;
  if (function == 20) {
    const cString& s = args.GetString(0);
    str_ptr = (const unsigned char*)(const char*)s;
    str_len = s.GetSize();
  }

  OptimizeResult result = avd_task_eval_optimize(
    out_flat.data(), out_len, ctx.GetOutputBuffer().GetCapacity(),
    function, args.GetInt(1), args.GetInt(2), args.GetInt(3),
    args.GetDouble(0), args.GetDouble(1), args.GetDouble(2),
    args.GetDouble(3), args.GetDouble(4),
    str_ptr, str_len);

  ctx.SetTaskValue(result.fx);

  if (result.quality > 1) {
    cout << "\n\nquality > 1!  quality= " << result.quality << "  Fx= " << result.fx << endl;
  }

  return result.quality;
}


void cTaskLib::Load_Mult(const cString& name, const cString& argstr, cEnvReqs&, Feedback& feedback)
{
  cArgSchema schema;
  
  // Integer Arguments
  schema.AddEntry("threshold", 0, -1);
  // Double Arguments
  schema.AddEntry("halflife", 0, cArgSchema::SCHEMA_DOUBLE);
  
  cArgContainer* args = cArgContainer::Load(argstr, schema, feedback);
  if (args) NewTask(name, "Multiplication", &cTaskLib::Task_Mult, 0, args);
}


double cTaskLib::Task_Mult(cTaskContext& ctx) const
{
  const cArgContainer& args = ctx.GetTaskEntry()->GetArguments();
  
  const tBuffer<int>& input_buffer = ctx.GetInputBuffer();
  const long long test_output = ctx.GetOutputBuffer()[0];
  const int input_size = input_buffer.GetNumStored();
  
  long long diff = avd_tasklib_diff_scan_init();
  
  for (int i = 0; i < input_size; i ++) {
    for (int j = 0; j < input_size; j ++) {
      if (i == j) continue;
      long long cur_diff = avd_tasklib_binary_pair_input_diff(
        input_buffer[i],
        input_buffer[j],
        test_output,
        AVD_TASKLIB_BINARY_OP_MULT
      );
      diff = avd_tasklib_diff_scan_update(diff, cur_diff);
    }
  }
  
  return avd_tasklib_threshold_halflife_quality(diff, args.GetInt(0), args.GetDouble(0));
}


void cTaskLib::Load_Div(const cString& name, const cString& argstr, cEnvReqs&, Feedback& feedback)
{
  cArgSchema schema;
  
  // Integer Arguments
  schema.AddEntry("threshold", 0, -1);
  // Double Arguments
  schema.AddEntry("halflife", 0, cArgSchema::SCHEMA_DOUBLE);
  
  cArgContainer* args = cArgContainer::Load(argstr, schema, feedback);
  if (args) NewTask(name, "Division", &cTaskLib::Task_Div, 0, args);
}


double cTaskLib::Task_Div(cTaskContext& ctx) const
{
  const cArgContainer& args = ctx.GetTaskEntry()->GetArguments();
  
  const tBuffer<int>& input_buffer = ctx.GetInputBuffer();
  const long long test_output = ctx.GetOutputBuffer()[0];
  const int input_size = input_buffer.GetNumStored();
  
  long long diff = avd_tasklib_diff_scan_init();
  
  for (int i = 0; i < input_size; i ++) {
    for (int j = 0; j < input_size; j ++) {
      if (i == j || input_buffer[j] == 0) continue;
      long long cur_diff = avd_tasklib_binary_pair_input_diff(
        input_buffer[i],
        input_buffer[j],
        test_output,
        AVD_TASKLIB_BINARY_OP_DIV
      );
      diff = avd_tasklib_diff_scan_update(diff, cur_diff);
    }
  }
  
  return avd_tasklib_threshold_halflife_quality(diff, args.GetInt(0), args.GetDouble(0));
}


void cTaskLib::Load_Log(const cString& name, const cString& argstr, cEnvReqs&, Feedback& feedback)
{
  cArgSchema schema;
  
  // Integer Arguments
  schema.AddEntry("threshold", 0, -1);
  // Double Arguments
  schema.AddEntry("halflife", 0, cArgSchema::SCHEMA_DOUBLE);
  
  cArgContainer* args = cArgContainer::Load(argstr, schema, feedback);
  if (args) NewTask(name, "Logarithm (natural)", &cTaskLib::Task_Log, 0, args);
}


double cTaskLib::Task_Log(cTaskContext& ctx) const
{
  const cArgContainer& args = ctx.GetTaskEntry()->GetArguments();
  
  const tBuffer<int>& input_buffer = ctx.GetInputBuffer();
  const long long test_output = ctx.GetOutputBuffer()[0];
  const int input_size = input_buffer.GetNumStored();
  
  long long diff = avd_tasklib_diff_scan_init();
  
  for (int i = 0; i < input_size; i ++) {
    long long cur_diff = avd_tasklib_unary_math_input_diff(
      input_buffer[i],
      test_output,
      AVD_TASKLIB_UNARY_OP_LOG,
      dCastPrecision
    );
    diff = avd_tasklib_diff_scan_update(diff, cur_diff);
  }
  
  return avd_tasklib_threshold_halflife_quality(diff, args.GetInt(0), args.GetDouble(0));
}


void cTaskLib::Load_Log2(const cString& name, const cString& argstr, cEnvReqs&, Feedback& feedback)
{
  cArgSchema schema;
  
  // Integer Arguments
  schema.AddEntry("threshold", 0, -1);
  // Double Arguments
  schema.AddEntry("halflife", 0, cArgSchema::SCHEMA_DOUBLE);
  
  cArgContainer* args = cArgContainer::Load(argstr, schema, feedback);
  if (args) NewTask(name, "Logarithm (base-2)", &cTaskLib::Task_Log2, 0, args);
}

double cTaskLib::Task_Log2(cTaskContext& ctx) const
{
  const cArgContainer& args = ctx.GetTaskEntry()->GetArguments();
  
  const tBuffer<int>& input_buffer = ctx.GetInputBuffer();
  const long long test_output = ctx.GetOutputBuffer()[0];
  const int input_size = input_buffer.GetNumStored();
  
  long long diff = avd_tasklib_diff_scan_init();
  
  for (int i = 0; i < input_size; i ++) {
    long long cur_diff = avd_tasklib_unary_math_input_diff(
      input_buffer[i],
      test_output,
      AVD_TASKLIB_UNARY_OP_LOG2,
      dCastPrecision
    );
    diff = avd_tasklib_diff_scan_update(diff, cur_diff);
  }
  
  return avd_tasklib_threshold_halflife_quality(diff, args.GetInt(0), args.GetDouble(0));
}


void cTaskLib::Load_Log10(const cString& name, const cString& argstr, cEnvReqs&, Feedback& feedback)
{
  cArgSchema schema;
  
  // Integer Arguments
  schema.AddEntry("threshold", 0, -1);
  // Double Arguments
  schema.AddEntry("halflife", 0, cArgSchema::SCHEMA_DOUBLE);
  
  cArgContainer* args = cArgContainer::Load(argstr, schema, feedback);
  if (args) NewTask(name, "Logarithm (base-10)", &cTaskLib::Task_Log10, 0, args);
}


double cTaskLib::Task_Log10(cTaskContext& ctx) const
{
  const cArgContainer& args = ctx.GetTaskEntry()->GetArguments();
  
  const tBuffer<int>& input_buffer = ctx.GetInputBuffer();
  const long long test_output = ctx.GetOutputBuffer()[0];
  const int input_size = input_buffer.GetNumStored();
  
  long long diff = avd_tasklib_diff_scan_init();
  
  for (int i = 0; i < input_size; i ++) {
    long long cur_diff = avd_tasklib_unary_math_input_diff(
      input_buffer[i],
      test_output,
      AVD_TASKLIB_UNARY_OP_LOG10,
      dCastPrecision
    );
    diff = avd_tasklib_diff_scan_update(diff, cur_diff);
  }
  
  return avd_tasklib_threshold_halflife_quality(diff, args.GetInt(0), args.GetDouble(0));
}


void cTaskLib::Load_Sqrt(const cString& name, const cString& argstr, cEnvReqs&, Feedback& feedback)
{
  cArgSchema schema;
  
  // Integer Arguments
  schema.AddEntry("threshold", 0, -1);
  // Double Arguments
  schema.AddEntry("halflife", 0, cArgSchema::SCHEMA_DOUBLE);
  
  cArgContainer* args = cArgContainer::Load(argstr, schema, feedback);
  if (args) NewTask(name, "Square Root", &cTaskLib::Task_Sqrt, 0, args);
}


double cTaskLib::Task_Sqrt(cTaskContext& ctx) const
{
  const cArgContainer& args = ctx.GetTaskEntry()->GetArguments();
  
  const tBuffer<int>& input_buffer = ctx.GetInputBuffer();
  const long long test_output = ctx.GetOutputBuffer()[0];
  const int input_size = input_buffer.GetNumStored();
  
  long long diff = avd_tasklib_diff_scan_init();
  
  for (int i = 0; i < input_size; i ++) {
    long long cur_diff = avd_tasklib_unary_math_input_diff(
      input_buffer[i],
      test_output,
      AVD_TASKLIB_UNARY_OP_SQRT,
      dCastPrecision
    );
    diff = avd_tasklib_diff_scan_update(diff, cur_diff);
  }
  
  return avd_tasklib_threshold_halflife_quality(diff, args.GetInt(0), args.GetDouble(0));
}


void cTaskLib::Load_Sine(const cString& name, const cString& argstr, cEnvReqs&, Feedback& feedback)
{
  cArgSchema schema;
  
  // Integer Arguments
  schema.AddEntry("threshold", 0, -1);
  // Double Arguments
  schema.AddEntry("halflife", 0, cArgSchema::SCHEMA_DOUBLE);
  
  cArgContainer* args = cArgContainer::Load(argstr, schema, feedback);
  if (args) NewTask(name, "Sine", &cTaskLib::Task_Sine, 0, args);
}


double cTaskLib::Task_Sine(cTaskContext& ctx) const
{
  const cArgContainer& args = ctx.GetTaskEntry()->GetArguments();
  
  const tBuffer<int>& input_buffer = ctx.GetInputBuffer();
  const long long test_output = ctx.GetOutputBuffer()[0];
  const int input_size = input_buffer.GetNumStored();
  
  long long diff = avd_tasklib_diff_scan_init();
  
  for (int i = 0; i < input_size; i ++) {
    long long cur_diff = avd_tasklib_unary_math_input_diff(
      input_buffer[i],
      test_output,
      AVD_TASKLIB_UNARY_OP_SINE,
      dCastPrecision
    );
    diff = avd_tasklib_diff_scan_update(diff, cur_diff);
  }
  
  return avd_tasklib_threshold_halflife_quality(diff, args.GetInt(0), args.GetDouble(0));
}


void cTaskLib::Load_Cosine(const cString& name, const cString& argstr, cEnvReqs&, Feedback& feedback)
{
  cArgSchema schema;
  
  // Integer Arguments
  schema.AddEntry("threshold", 0, -1);
  // Double Arguments
  schema.AddEntry("halflife", 0, cArgSchema::SCHEMA_DOUBLE);
  
  cArgContainer* args = cArgContainer::Load(argstr, schema, feedback);
  if (args) NewTask(name, "Cosine", &cTaskLib::Task_Cosine, 0, args);
}


double cTaskLib::Task_Cosine(cTaskContext& ctx) const
{
  const cArgContainer& args = ctx.GetTaskEntry()->GetArguments();
  
  const tBuffer<int>& input_buffer = ctx.GetInputBuffer();
  const long long test_output = ctx.GetOutputBuffer()[0];
  const int input_size = input_buffer.GetNumStored();
  
  long long diff = avd_tasklib_diff_scan_init();
  
  for (int i = 0; i < input_size; i ++) {
    long long cur_diff = avd_tasklib_unary_math_input_diff(
      input_buffer[i],
      test_output,
      AVD_TASKLIB_UNARY_OP_COSINE,
      dCastPrecision
    );
    diff = avd_tasklib_diff_scan_update(diff, cur_diff);
  }
  
  return avd_tasklib_threshold_halflife_quality(diff, args.GetInt(0), args.GetDouble(0));
}


double cTaskLib::Task_CommEcho(cTaskContext& ctx) const
{
  const int test_output = ctx.GetOutputBuffer()[0];
  // Flatten neighbor input buffers for Rust FFI
  std::vector<int> flat;
  tConstListIterator<tBuffer<int> > buff_it(ctx.GetNeighborhoodInputBuffers());
  while (buff_it.Next() != NULL) {
    const tBuffer<int>& cur_buff = *(buff_it.Get());
    for (int i = 0; i < cur_buff.GetNumStored(); i++) flat.push_back(cur_buff[i]);
  }
  return avd_task_eval_comm_echo(test_output, flat.data(), static_cast<int>(flat.size()));
}


double cTaskLib::Task_CommNot(cTaskContext& ctx) const
{
  const int test_output = ctx.GetOutputBuffer()[0];
  // Flatten neighbor input buffers for Rust FFI
  std::vector<int> flat;
  tConstListIterator<tBuffer<int> > buff_it(ctx.GetNeighborhoodInputBuffers());
  while (buff_it.Next() != NULL) {
    const tBuffer<int>& cur_buff = *(buff_it.Get());
    for (int i = 0; i < cur_buff.GetNumStored(); i++) flat.push_back(cur_buff[i]);
  }
  return avd_task_eval_comm_not(test_output, flat.data(), static_cast<int>(flat.size()));
}


//TODO: add movement tasks here

double cTaskLib::Task_MoveUpGradient(cTaskContext& ctx) const
{
  ctx.EnsureSnapshot();
  return avd_task_ctx_move_up_gradient(&ctx.GetSnapshot());
}


double cTaskLib::Task_MoveNeutralGradient(cTaskContext& ctx) const
{
  ctx.EnsureSnapshot();
  return avd_task_ctx_move_neutral_gradient(&ctx.GetSnapshot());
}


double cTaskLib::Task_MoveDownGradient(cTaskContext& ctx) const
{
  ctx.EnsureSnapshot();
  return avd_task_ctx_move_down_gradient(&ctx.GetSnapshot());
}


double cTaskLib::Task_MoveNotUpGradient(cTaskContext& ctx) const
{
  ctx.EnsureSnapshot();
  return avd_task_ctx_move_not_up_gradient(&ctx.GetSnapshot());
}


double cTaskLib::Task_MoveToRightSide(cTaskContext& ctx) const
{
  cDeme* deme = ctx.GetOrganism()->GetDeme();
  int pos_x = deme->GetCellPosition(ctx.GetOrganism()->GetCellID()).first;
  return avd_task_eval_move_to_side(pos_x, m_world->GetConfig().WORLD_X.Get() - 1);
}


double cTaskLib::Task_MoveToLeftSide(cTaskContext& ctx) const
{
  cDeme* deme = ctx.GetOrganism()->GetDeme();
  int pos_x = deme->GetCellPosition(ctx.GetOrganism()->GetCellID()).first;
  return avd_task_eval_move_to_side(pos_x, 0);
}


double cTaskLib::Task_Move(cTaskContext& ctx) const
{
  int cell_id = ctx.GetOrganism()->GetCellID();
  if (m_world->GetConfig().USE_AVATARS.Get()) cell_id = ctx.GetOrganism()->GetAVCellID();
  int prev = ctx.GetOrganism()->GetPrevSeenCellID();
  double reward = avd_task_eval_move(cell_id, prev);
  if (reward > 0.0) ctx.GetOrganism()->SetPrevSeenCellID(cell_id);
  return reward;
}


double cTaskLib::Task_MoveToTarget(cTaskContext& ctx) const
{
  cOrganism* org = ctx.GetOrganism();
  if (org->GetCellID() == -1) return 0.0;
  cDeme* deme = org->GetDeme();
  assert(deme);
  int cell_data = org->GetCellData();
  int current_cell = deme->GetRelativeCellID(org->GetCellID());
  int prev_target = deme->GetRelativeCellID(org->GetPrevTaskCellID());
  double reward = avd_task_eval_move_to_target(cell_data, current_cell, prev_target);
  if (reward > 0.0) {
    org->AddReachedTaskCell();
    org->SetPrevTaskCellID(current_cell);
  }
  return reward;
}


double cTaskLib::Task_MoveToMovementEvent(cTaskContext& ctx) const
{
  cOrganism* org = ctx.GetOrganism();
  
  if (org->GetCellID() == -1) return 0.0;		
	
  cDeme* deme = org->GetDeme();
  assert(deme);
  
  int cell_data = org->GetCellData();
  if (cell_data <= 0) return 0.0;
    
  for (int i = 0; i < deme->GetNumMovementPredicates(); i++) {
    if (deme->GetMovPredicate(i)->GetEvent(0)->GetEventID() == cell_data) {
      org->AddReachedTaskCell();
      org->SetPrevTaskCellID(cell_data);
      return 1.0;
    }
  }
  return 0.0;
}


double cTaskLib::Task_MoveBetweenMovementEvent(cTaskContext& ctx) const
{	
  cOrganism* org = ctx.GetOrganism();

  if (org->GetCellID() == -1) return 0.0;
	
  cDeme* deme = org->GetDeme();
  assert(deme);

  int cell_data = org->GetCellData();
  
  int prev_target = deme->GetRelativeCellID(org->GetPrevTaskCellID());

  // NOTE: as of now, orgs aren't rewarded if they touch a target more than
  //   once in a row.  Could be useful in the future to have fractional reward
  //   or something.
  if ( (cell_data <= 0) || (cell_data == prev_target) ) return 0.0;
    
  for (int i = 0; i < deme->GetNumMovementPredicates(); i++) {
    // NOTE: having problems with calling the GetNumEvents function for some reason.  FIXME
    //int num_events = deme.GetMovPredicate(i)->GetNumEvents;
    int num_events = 2;

    if (num_events == 1) {
      if ( (deme->GetMovPredicate(i)->GetEvent(0)->IsActive()) &&
          (deme->GetMovPredicate(i)->GetEvent(0)->GetEventID() == cell_data) ) {
        org->AddReachedTaskCell();
        org->SetPrevTaskCellID(cell_data);
        return 1.0;
      }
    } else {
      for (int j = 0; j < num_events; j++) {
        cDemeCellEvent* event = deme->GetMovPredicate(i)->GetEvent(j);
        if ( (event != NULL) && (event->IsActive()) && (event->GetEventID() == cell_data) ) {
          org->AddReachedTaskCell();
          org->SetPrevTaskCellID(cell_data);
          return 1.0;
        }
      }
    }
  }
  return 0.0;
}


double cTaskLib::Task_MoveToEvent(cTaskContext& ctx) const
{
  cOrganism* org = ctx.GetOrganism();
  if (org->GetCellID() == -1) return 0.0;
  cDeme* deme = org->GetDeme();
  assert(deme);
  int cell_data = org->GetCellData();
  int num_events = deme->GetNumEvents();
  // Extract event IDs into flat array for Rust FFI
  std::vector<int> event_ids(num_events);
  for (int i = 0; i < num_events; i++) {
    event_ids[i] = deme->GetCellEvent(i)->GetEventID();
  }
  return avd_task_eval_move_to_event(event_ids.data(), num_events, cell_data);
}


double cTaskLib::Task_EventKilled(cTaskContext& ctx) const
{
  ctx.EnsureSnapshot();
  return avd_task_ctx_event_killed(&ctx.GetSnapshot());
}



void cTaskLib::Load_SGPathTraversal(const cString& name, const cString& argstr, cEnvReqs&, Feedback& feedback)
{
  cArgSchema schema;

  // Integer Arguments
  schema.AddEntry("pathlen", 0, cArgSchema::SCHEMA_INT);
  
  // String Arguments
  schema.AddEntry("sgname", 0, cArgSchema::SCHEMA_STRING);
  schema.AddEntry("poison", 1, cArgSchema::SCHEMA_STRING);
  
  // Double Arguments
//  schema.AddEntry("halflife", 0, cArgSchema::SCHEMA_DOUBLE);
//  schema.AddEntry("base", 1, 2.0);
  
  cArgContainer* args = cArgContainer::Load(argstr, schema, feedback);
  if (args) NewTask(name, "State Grid Path Traversal", &cTaskLib::Task_SGPathTraversal, 0, args);
}


double cTaskLib::Task_SGPathTraversal(cTaskContext& ctx) const
{
  const cArgContainer& args = ctx.GetTaskEntry()->GetArguments();
  const cStateGrid& sg = ctx.GetOrganism()->GetStateGrid();

  if (sg.GetName() != args.GetString(0)) return 0.0;

  int state = sg.GetStateID(args.GetString(1));
  if (state < 0) return 0.0;

  const AvidaArray<int>& ext_mem = ctx.GetExtendedMemory();

  // Extract history from extended memory (cells visited after header)
  const int history_offset = 3 + sg.GetNumStates();
  const int history_len = ext_mem.GetSize() - history_offset;

  // Extract state grid data for Rust
  const int grid_size = sg.GetWidth() * sg.GetHeight();
  std::vector<int> grid_data(grid_size);
  for (int i = 0; i < grid_size; i++) grid_data[i] = sg.GetStateAt(i);

  // Build history array for Rust
  std::vector<int> history(history_len);
  for (int i = 0; i < history_len; i++) history[i] = ext_mem[i + history_offset];

  // Delegate traversal quality computation to Rust
  return avd_task_sg_path_traversal_quality(
    history.data(), history_len,
    grid_data.data(), grid_size,
    state,
    ext_mem[3 + state],
    args.GetInt(0));
}  


/* This task provides major points for perfect strings and some points for just
   storing stuff. */
double cTaskLib::Task_CreatePerfectStrings(cTaskContext& ctx) const
{
  double bonus = 0.0;
  int min = -1;
  int temp = 0;
  for (unsigned int i = 0; i<m_strings.size(); i++) {
    temp = ctx.GetOrganism()->GetNumberStringsOnHand(i); 
    
    // Figure out what the minimum amount of a string is.
    if ((min == -1) || (temp < min)){
      min = temp;
    }
  }
  
  // Bonus for creating perfect strings!
  bonus = min; 
	
  // Add in some value for just creating stuff
  for (unsigned int i = 0; i<m_strings.size(); i++) {
    temp = ctx.GetOrganism()->GetNumberStringsOnHand(i); 
    
    if (temp > min) { 
      bonus += (temp - min); 
    }
  } 
  
  // Update stats
  m_world->GetStats().IncPerfectMatch(min);
  if (min > 0) m_world->GetStats().IncPerfectMatchOrg();
  
  return bonus; 
}


void cTaskLib::Load_FormSpatialGroup(const cString& name, const cString& argstr, cEnvReqs&, Feedback& feedback)
{
  cArgSchema schema;
  
  // Integer Arguments
  schema.AddEntry("group_size", 0, 1);
  
  cArgContainer* args = cArgContainer::Load(argstr, schema, feedback);
  if (args) NewTask(name, "FormSpatialGroups", &cTaskLib::Task_FormSpatialGroup, 0, args);
}


double cTaskLib::Task_FormSpatialGroup(cTaskContext& ctx) const
{
  int ideal_size = ctx.GetTaskEntry()->GetArguments().GetInt(0);
  int group_id = 0;
  if (ctx.GetOrganism()->HasOpinion()) {
    group_id = ctx.GetOrganism()->GetOpinion().first;
  }
  int group_size = m_world->GetPopulation().NumberOfOrganismsInGroup(group_id);
  return avd_task_eval_form_spatial_group(ideal_size, group_size);
}


/* Reward organisms for having a given group-id, provided the group is under the 
   max number of members. */

void cTaskLib::Load_FormSpatialGroupWithID(const cString& name, const cString& argstr, cEnvReqs&, Feedback& feedback)
{
  cArgSchema schema;
  
  // Integer Arguments
  schema.AddEntry("group_size", 0, 1);
  schema.AddEntry("group_id", 1, 1);
    
  cArgContainer* args = cArgContainer::Load(argstr, schema, feedback);
  if (args) NewTask(name, "FormSpatialGroupWithID", &cTaskLib::Task_FormSpatialGroupWithID, 0, args);
  
  // Add this group id to the list in the instructions file. 
  m_world->GetEnvironment().AddGroupID(args->GetInt(1));
}


double cTaskLib::Task_FormSpatialGroupWithID(cTaskContext& ctx) const
{
  int ideal_size = ctx.GetTaskEntry()->GetArguments().GetInt(0);
  int desired_group_id = ctx.GetTaskEntry()->GetArguments().GetInt(1);
  int group_id = -1;
  if (ctx.GetOrganism()->HasOpinion()) {
    group_id = ctx.GetOrganism()->GetOpinion().first;
  }
  int group_size = m_world->GetPopulation().NumberOfOrganismsInGroup(group_id);
  return avd_task_eval_form_spatial_group_with_id(ideal_size, group_id, desired_group_id, group_size);
}

/* Reward organisms for having a given group-id.*/
void cTaskLib::Load_LiveOnPatchRes(const cString& name, const cString& argstr, cEnvReqs&, Feedback& feedback)
{
  cArgSchema schema;
  
  schema.AddEntry("patch_id", 0, 1);
  
  cArgContainer* args = cArgContainer::Load(argstr, schema, feedback);
  if (args) NewTask(name, "LiveOnPatchRes", &cTaskLib::Task_LiveOnPatchRes, 0, args);
	
  // Add this patch id to the list in the instructions file. 
  m_world->GetEnvironment().AddGroupID(args->GetInt(0));
}

double cTaskLib::Task_LiveOnPatchRes(cTaskContext& ctx) const
{
  // Identical to OpinionIs: reward if opinion matches desired patch ID.
  ctx.EnsureSnapshot();
  ctx.GetSnapshotMut().task_arg_int[0] = ctx.GetTaskEntry()->GetArguments().GetInt(0);
  return avd_task_ctx_opinion_is(&ctx.GetSnapshot());
}

void cTaskLib::Load_CollectOddCell(const cString& name, const cString& argstr, cEnvReqs& envreqs, Feedback& feedback)
{
  cArgSchema schema;
  
  schema.AddEntry("even_or_odd", 0, 0);
  
  cArgContainer* args = cArgContainer::Load(argstr, schema, feedback);
  if (args) NewTask(name, "collect-odd-cell", &cTaskLib::Task_CollectOddCell, 0, args);
}

double cTaskLib::Task_CollectOddCell(cTaskContext& ctx) const
{
  ctx.EnsureSnapshot();
  ctx.GetSnapshotMut().task_arg_int[0] = ctx.GetTaskEntry()->GetArguments().GetInt(0);
  return avd_task_ctx_collect_odd_cell(&ctx.GetSnapshot());
}

/* Reward organisms for having found a targeted resource*/
void cTaskLib::Load_ConsumeTarget(const cString& name, const cString& argstr, cEnvReqs&, Feedback& feedback)
{
  cArgSchema schema;
  
  schema.AddEntry("target_id", 0, 1);
  cArgContainer* args = cArgContainer::Load(argstr, schema, feedback);
  if (args) NewTask(name, "ConsumeTarget", &cTaskLib::Task_ConsumeTarget, 0, args);

  // Add this target id to the list in the instructions file. 
  m_world->GetEnvironment().AddTargetID(args->GetInt(0));
}

/* Reward organisms for having found a targeted resource + performing logic*/
void cTaskLib::Load_ConsumeTargetEcho(const cString& name, const cString& argstr, cEnvReqs&, Feedback& feedback)
{
  cArgSchema schema;
  
  schema.AddEntry("target_id", 0, 1);
  cArgContainer* args = cArgContainer::Load(argstr, schema, feedback);
  if (args) NewTask(name, "ConsumeTargetEcho", &cTaskLib::Task_ConsumeTargetEcho, 0, args);

  // Add this target id to the list in the instructions file. 
  m_world->GetEnvironment().AddTargetID(args->GetInt(0));
}

/* Reward organisms for having found a targeted resource + performing logic*/
void cTaskLib::Load_ConsumeTargetNand(const cString& name, const cString& argstr, cEnvReqs&, Feedback& feedback)
{
  cArgSchema schema;
  
  schema.AddEntry("target_id", 0, 1);
  cArgContainer* args = cArgContainer::Load(argstr, schema, feedback);
  if (args) NewTask(name, "ConsumeTargetNand", &cTaskLib::Task_ConsumeTargetNand, 0, args);

  // Add this target id to the list in the instructions file. 
  m_world->GetEnvironment().AddTargetID(args->GetInt(0));
}

/* Reward organisms for having found a targeted resource + performing logic*/
void cTaskLib::Load_ConsumeTargetAnd(const cString& name, const cString& argstr, cEnvReqs&, Feedback& feedback)
{
  cArgSchema schema;
  
  schema.AddEntry("target_id", 0, 1);
  cArgContainer* args = cArgContainer::Load(argstr, schema, feedback);
  if (args) NewTask(name, "ConsumeTargetAnd", &cTaskLib::Task_ConsumeTargetAnd, 0, args);

  // Add this target id to the list in the instructions file. 
  m_world->GetEnvironment().AddTargetID(args->GetInt(0));
}

/* Reward organisms for having found a targeted resource + performing logic*/
void cTaskLib::Load_ConsumeTargetOrn(const cString& name, const cString& argstr, cEnvReqs&, Feedback& feedback)
{
  cArgSchema schema;
  
  schema.AddEntry("target_id", 0, 1);
  cArgContainer* args = cArgContainer::Load(argstr, schema, feedback);
  if (args) NewTask(name, "ConsumeTargetOrn", &cTaskLib::Task_ConsumeTargetOrn, 0, args);

  // Add this target id to the list in the instructions file. 
  m_world->GetEnvironment().AddTargetID(args->GetInt(0));
}

/* Reward organisms for having found a targeted resource + performing logic*/
void cTaskLib::Load_ConsumeTargetOr(const cString& name, const cString& argstr, cEnvReqs&, Feedback& feedback)
{
  cArgSchema schema;
  
  schema.AddEntry("target_id", 0, 1);
  cArgContainer* args = cArgContainer::Load(argstr, schema, feedback);
  if (args) NewTask(name, "ConsumeTargetOr", &cTaskLib::Task_ConsumeTargetOr, 0, args);

  // Add this target id to the list in the instructions file. 
  m_world->GetEnvironment().AddTargetID(args->GetInt(0));
}

/* Reward organisms for having found a targeted resource + performing logic*/
void cTaskLib::Load_ConsumeTargetAndn(const cString& name, const cString& argstr, cEnvReqs&, Feedback& feedback)
{
  cArgSchema schema;
  
  schema.AddEntry("target_id", 0, 1);
  cArgContainer* args = cArgContainer::Load(argstr, schema, feedback);
  if (args) NewTask(name, "ConsumeTargetAndn", &cTaskLib::Task_ConsumeTargetAndn, 0, args);

  // Add this target id to the list in the instructions file. 
  m_world->GetEnvironment().AddTargetID(args->GetInt(0));
}

/* Reward organisms for having found a targeted resource + performing logic*/
void cTaskLib::Load_ConsumeTargetNor(const cString& name, const cString& argstr, cEnvReqs&, Feedback& feedback)
{
  cArgSchema schema;
  
  schema.AddEntry("target_id", 0, 1);
  cArgContainer* args = cArgContainer::Load(argstr, schema, feedback);
  if (args) NewTask(name, "ConsumeTargetNor", &cTaskLib::Task_ConsumeTargetNor, 0, args);

  // Add this target id to the list in the instructions file. 
  m_world->GetEnvironment().AddTargetID(args->GetInt(0));
}

/* Reward organisms for having found a targeted resource + performing logic*/
void cTaskLib::Load_ConsumeTargetXor(const cString& name, const cString& argstr, cEnvReqs&, Feedback& feedback)
{
  cArgSchema schema;
  
  schema.AddEntry("target_id", 0, 1);
  cArgContainer* args = cArgContainer::Load(argstr, schema, feedback);
  if (args) NewTask(name, "ConsumeTargetXor", &cTaskLib::Task_ConsumeTargetXor, 0, args);

  // Add this target id to the list in the instructions file. 
  m_world->GetEnvironment().AddTargetID(args->GetInt(0));
}

/* Reward organisms for having found a targeted resource + performing logic*/
void cTaskLib::Load_ConsumeTargetEqu(const cString& name, const cString& argstr, cEnvReqs&, Feedback& feedback)
{
  cArgSchema schema;
  
  schema.AddEntry("target_id", 0, 1);
  cArgContainer* args = cArgContainer::Load(argstr, schema, feedback);
  if (args) NewTask(name, "ConsumeTargetEqu", &cTaskLib::Task_ConsumeTargetEqu, 0, args);

  // Add this target id to the list in the instructions file. 
  m_world->GetEnvironment().AddTargetID(args->GetInt(0));
}

void cTaskLib::Load_MoveFT(const cString& name, const cString& argstr, cEnvReqs&, Feedback& feedback)
{
  cArgSchema schema;
  
  schema.AddEntry("target_id", 0, 1);
  cArgContainer* args = cArgContainer::Load(argstr, schema, feedback);
  if (args) NewTask(name, "MoveFT", &cTaskLib::Task_MoveFT, 0, args);

  // Add this target id to the list in the instructions file. 
  m_world->GetEnvironment().AddTargetID(args->GetInt(0));
}

double cTaskLib::Task_ConsumeTarget(cTaskContext& ctx) const
{
  ctx.EnsureSnapshot();
  ctx.GetSnapshotMut().task_arg_int[0] = ctx.GetTaskEntry()->GetArguments().GetInt(0);
  return avd_task_ctx_consume_target(&ctx.GetSnapshot());
}

double cTaskLib::Task_ConsumeTargetEcho(cTaskContext& ctx) const
{
  ctx.EnsureSnapshot();
  ctx.GetSnapshotMut().task_arg_int[0] = ctx.GetTaskEntry()->GetArguments().GetInt(0);
  return avd_task_ctx_consume_target_echo(&ctx.GetSnapshot());
}

double cTaskLib::Task_ConsumeTargetNand(cTaskContext& ctx) const
{
  ctx.EnsureSnapshot();
  ctx.GetSnapshotMut().task_arg_int[0] = ctx.GetTaskEntry()->GetArguments().GetInt(0);
  return avd_task_ctx_consume_target_logic(&ctx.GetSnapshot(), AVD_TASK_NAND);
}

double cTaskLib::Task_ConsumeTargetAnd(cTaskContext& ctx) const
{
  ctx.EnsureSnapshot();
  ctx.GetSnapshotMut().task_arg_int[0] = ctx.GetTaskEntry()->GetArguments().GetInt(0);
  return avd_task_ctx_consume_target_logic(&ctx.GetSnapshot(), AVD_TASK_AND);
}

double cTaskLib::Task_ConsumeTargetOrn(cTaskContext& ctx) const
{
  ctx.EnsureSnapshot();
  ctx.GetSnapshotMut().task_arg_int[0] = ctx.GetTaskEntry()->GetArguments().GetInt(0);
  return avd_task_ctx_consume_target_logic(&ctx.GetSnapshot(), AVD_TASK_ORNOT);
}

double cTaskLib::Task_ConsumeTargetOr(cTaskContext& ctx) const
{
  ctx.EnsureSnapshot();
  ctx.GetSnapshotMut().task_arg_int[0] = ctx.GetTaskEntry()->GetArguments().GetInt(0);
  return avd_task_ctx_consume_target_logic(&ctx.GetSnapshot(), AVD_TASK_OR);
}

double cTaskLib::Task_ConsumeTargetAndn(cTaskContext& ctx) const
{
  ctx.EnsureSnapshot();
  ctx.GetSnapshotMut().task_arg_int[0] = ctx.GetTaskEntry()->GetArguments().GetInt(0);
  return avd_task_ctx_consume_target_logic(&ctx.GetSnapshot(), AVD_TASK_ANDNOT);
}

double cTaskLib::Task_ConsumeTargetNor(cTaskContext& ctx) const
{
  ctx.EnsureSnapshot();
  ctx.GetSnapshotMut().task_arg_int[0] = ctx.GetTaskEntry()->GetArguments().GetInt(0);
  return avd_task_ctx_consume_target_logic(&ctx.GetSnapshot(), AVD_TASK_NOR);
}

double cTaskLib::Task_ConsumeTargetXor(cTaskContext& ctx) const
{
  ctx.EnsureSnapshot();
  ctx.GetSnapshotMut().task_arg_int[0] = ctx.GetTaskEntry()->GetArguments().GetInt(0);
  return avd_task_ctx_consume_target_logic(&ctx.GetSnapshot(), AVD_TASK_XOR);
}

double cTaskLib::Task_ConsumeTargetEqu(cTaskContext& ctx) const
{
  ctx.EnsureSnapshot();
  ctx.GetSnapshotMut().task_arg_int[0] = ctx.GetTaskEntry()->GetArguments().GetInt(0);
  return avd_task_ctx_consume_target_logic(&ctx.GetSnapshot(), AVD_TASK_EQU);
}

double cTaskLib::Task_MoveFT(cTaskContext& ctx) const
{
  int cell_id = ctx.GetOrganism()->GetCellID();
  if (m_world->GetConfig().USE_AVATARS.Get()) cell_id = ctx.GetOrganism()->GetAVCellID();
  int prev_seen = ctx.GetOrganism()->GetPrevSeenCellID();
  int forage_target = ctx.GetOrganism()->GetForageTarget();
  int desired_target = ctx.GetTaskEntry()->GetArguments().GetInt(0);
  double reward = avd_task_eval_move_ft(cell_id, prev_seen, forage_target, desired_target);
  if (reward > 0.0) {
    ctx.GetOrganism()->SetPrevSeenCellID(cell_id);
  }
  return reward;
}

/* Reward organisms for executing the explode command*/

double cTaskLib::Task_Exploded(cTaskContext& ctx) const
{
  ctx.EnsureSnapshot();
  return avd_task_ctx_exploded(&ctx.GetSnapshot());
}

/* Reward organisms for executing the explode command. Second reaction included in order to make two differently valued explode reactions*/

double cTaskLib::Task_Exploded2(cTaskContext& ctx) const
{
  ctx.EnsureSnapshot();
  return avd_task_ctx_exploded2(&ctx.GetSnapshot());
}

/*Charges organism for setting the autoinducer flag.*/
double cTaskLib::Task_AIDisplayCost(cTaskContext& ctx) const
{
  return avd_task_eval_ai_display_cost(avd_org_get_lyse_display(ctx.GetOrganism()));
}

/*Charges organism for producing public good.*/
double cTaskLib::Task_ProducePublicGood(cTaskContext& ctx) const
{
  ctx.EnsureSnapshot();
  return avd_task_ctx_produce_public_good(&ctx.GetSnapshot());
}

/*Reward organisms for having neighbors around them that are producing a public good. Currently assumes toroidal world.*/
double cTaskLib::Task_ConsumePublicGood(cTaskContext& ctx) const
{
  int good_counter = 0;
  
  int cellID = ctx.GetOrganism()->GetCellID();
  
  int radius = 1;
  
  int world_x = m_world->GetConfig().WORLD_X.Get();
  int world_y = m_world->GetConfig().WORLD_Y.Get();
  int cell_x = cellID % world_x;
  int cell_y = (cellID - cell_x)/world_x;

  std::set<cOrganism*> prod_set;
  for (int i = cell_x - radius; i <= cell_x + radius; i++) {
    for (int j = cell_y - radius; j <= cell_y + radius; j++) {
      int x;
      int y;
      //if (i==cell_x && j ==cell_y) continue;
      //TODO: make it modulus instead of subtract so that the radius can be bigger than the size of the world
      if (i<0) x = world_x + i;
      else if (i>= world_x) x = i-world_x;
      else x = i;
      
      if (j<0) y = world_y + j;
      else if (j >= world_y) y = j-world_y;
      else y = j;
      
      cPopulationCell& neighbor_cell = m_world->GetPopulation().GetCell(y*world_x + x);

      
      //do we actually have someone in neighborhood?
      if (neighbor_cell.IsOccupied() == false) continue;
      
      cOrganism* org_temp = neighbor_cell.GetOrganism();
      
      if (org_temp != NULL) {
        if (org_temp->GetPhenotype().GetKaboomExecuted()){
          good_counter ++;
          prod_set.insert(org_temp);
        }
      }
  
    }
  }
  if (good_counter >= 3){
    //Select a random organism from the producing set to actually get resource from
    double r = std::rand() % prod_set.size();
    set<cOrganism*>::const_iterator it(prod_set.begin());
    std::advance(it, r);
    cOrganism* org_choice = *it;
    org_choice->GetPhenotype().ClearKaboomExecuted();
    return true;
  }
  else return false;

}

double cTaskLib::Task_XorMax(cTaskContext& ctx) const
{
  AvidaArray<double> cell_res;
  if (!m_world->GetConfig().USE_AVATARS.Get()) cell_res = ctx.GetOrganism()->GetOrgInterface().GetResources(m_world->GetDefaultContext());
  else if (m_world->GetConfig().USE_AVATARS.Get()) cell_res = ctx.GetOrganism()->GetOrgInterface().GetAVResources(m_world->GetDefaultContext());
  
  double max_amount = 0.0;
  int max_res = 0;
  // if more than one resource is available, set the reaction to use the resource with the most available in this spot (note that, with global resources, the GLOBAL total will evaluated)
  for (int i = 0; i < cell_res.GetSize(); i++) {
    if (cell_res[i] > max_amount) {
      max_amount = cell_res[i];
      max_res = i;
    }
  }    
  cReaction* found_reaction = m_world->GetEnvironment().GetReactionLib().GetReaction(ctx.GetTaskEntry()->GetID());
  if (found_reaction == NULL) return false;
  m_world->GetEnvironment().ChangeResource(found_reaction, m_world->GetEnvironment().GetResourceLib().GetResource(max_res)->GetName());
  return Task_Xor(ctx);
}

void cTaskLib::Load_AllOnes(const cString& name, const cString& argstr, cEnvReqs& envreqs, Feedback& feedback)
{
  cArgSchema schema;
  schema.AddEntry("length", 0, 0);		
  cArgContainer* args = cArgContainer::Load(argstr, schema, feedback);	
  envreqs.SetMinOutputs(args->GetInt(0));
  if (args) NewTask(name, "all-ones", &cTaskLib::Task_AllOnes, 0, args);
}

double cTaskLib::Task_AllOnes(cTaskContext& ctx) const
{
  int length = ctx.GetTaskEntry()->GetArguments().GetInt(0);
  tBuffer<int> buf(ctx.GetOutputBuffer());
  std::vector<int> flat(length);
  for (int i = 0; i < length; i++) flat[i] = buf[i];
  return avd_task_eval_all_ones(flat.data(), length, length);
}


void cTaskLib::Load_RoyalRoad(const cString& name, const cString& argstr, cEnvReqs& envreqs, Feedback& feedback)
{
  cArgSchema schema;
  schema.AddEntry("length", 0, 0);
  schema.AddEntry("block_count", 1, 0);
  cArgContainer* args = cArgContainer::Load(argstr, schema, feedback);	
  envreqs.SetMinOutputs(args->GetInt(0));
  if (args) NewTask(name, "royal-road", &cTaskLib::Task_RoyalRoad, 0, args);
}

double cTaskLib::Task_RoyalRoad(cTaskContext& ctx) const
{
  int length = ctx.GetTaskEntry()->GetArguments().GetInt(0);
  int block_count = ctx.GetTaskEntry()->GetArguments().GetInt(1);
  tBuffer<int> buf(ctx.GetOutputBuffer());
  // Extract circular buffer to contiguous array for Rust FFI
  std::vector<int> flat(length);
  for (int i = 0; i < length; i++) flat[i] = buf[i];
  return avd_task_eval_royal_road(flat.data(), length, length, block_count);
}


void cTaskLib::Load_RoyalRoadWithDitches(const cString& name, const cString& argstr, cEnvReqs& envreqs, Feedback& feedback)
{
  cArgSchema schema;
  schema.AddEntry("length", 0, 0);
  schema.AddEntry("block_count", 1, 0);
  schema.AddEntry("width", 2, 0);
  schema.AddEntry("height", 3, 0);
  cArgContainer* args = cArgContainer::Load(argstr, schema, feedback);	
  envreqs.SetMinOutputs(args->GetInt(0));
  if (args) NewTask(name, "royal-road-wd", &cTaskLib::Task_RoyalRoadWithDitches, 0, args);
}


double cTaskLib::Task_RoyalRoadWithDitches(cTaskContext& ctx) const
{
  int length = ctx.GetTaskEntry()->GetArguments().GetInt(0);
  int block_count = ctx.GetTaskEntry()->GetArguments().GetInt(1);
  int width = ctx.GetTaskEntry()->GetArguments().GetInt(2);
  int height = ctx.GetTaskEntry()->GetArguments().GetInt(3);
  tBuffer<int> buf(ctx.GetOutputBuffer());
  // Extract circular buffer to contiguous array for Rust FFI
  std::vector<int> flat(length);
  for (int i = 0; i < length; i++) flat[i] = buf[i];
  return avd_task_eval_royal_road_wd(flat.data(), length, length, block_count, width, height);
}

	
	
//! Load the command line for checking an opinion.
void cTaskLib::Load_OpinionIs(const cString& name, const cString& argstr, cEnvReqs&, Feedback& feedback)
{
  cArgSchema schema;
  schema.AddEntry("opinion", 0, cArgSchema::SCHEMA_INT);
  cArgContainer* args = cArgContainer::Load(argstr, schema, feedback);
  if(args) {
    NewTask(name, "Whether organism's opinion is set to value.", &cTaskLib::Task_OpinionIs, 0, args);
  }
} 
	
//! This task is complete if this organism's current opinion is set to a configured value.
double cTaskLib::Task_OpinionIs(cTaskContext& ctx) const
{
  ctx.EnsureSnapshot();
  ctx.GetSnapshotMut().task_arg_int[0] = ctx.GetTaskEntry()->GetArguments().GetInt(0);
  return avd_task_ctx_opinion_is(&ctx.GetSnapshot());
}
