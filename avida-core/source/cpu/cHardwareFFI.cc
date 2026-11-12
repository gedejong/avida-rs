/*
 *  cHardwareFFI.cc — Thin FFI layer for CPU hardware state access from Rust.
 *
 *  Exposes cHardwareCPU internals (heads, stacks, memory, registers, labels)
 *  via opaque cHardwareBase* pointers.  C++ retains ownership; Rust reads and
 *  writes through these accessors.
 *
 *  The opaque pointer is cHardwareBase* (the base class), which is
 *  static_cast'd to cHardwareCPU* inside each function.  This is safe because
 *  these FFI functions are only called from instruction handlers that are
 *  already running inside a cHardwareCPU context.
 */

#include "cHardwareCPU.h"
#include "cAvidaContext.h"
#include "cOrganism.h"
#include "cPhenotype.h"
#include "nHardware.h"

// Helper: safe downcast.  Returns nullptr if hw is null.
static inline cHardwareCPU* as_cpu(cHardwareBase* hw) {
  return hw ? static_cast<cHardwareCPU*>(hw) : nullptr;
}

extern "C" {

// ---- Head accessors ----
// head_id: 0=IP, 1=READ, 2=WRITE, 3=FLOW (nHardware::tHeads)

int avd_hw_get_head_position(cHardwareBase* hw, int head_id) {
  auto* cpu = as_cpu(hw);
  if (!cpu || head_id < 0 || head_id >= nHardware::NUM_HEADS) return -1;
  return cpu->GetHead(head_id).GetPosition();
}

void avd_hw_set_head_position(cHardwareBase* hw, int head_id, int pos) {
  auto* cpu = as_cpu(hw);
  if (!cpu || head_id < 0 || head_id >= nHardware::NUM_HEADS) return;
  cpu->GetHead(head_id).Set(pos);
}

int avd_hw_get_ip_position(cHardwareBase* hw) {
  auto* cpu = as_cpu(hw);
  if (!cpu) return -1;
  return cpu->IP().GetPosition();
}

void avd_hw_set_ip_position(cHardwareBase* hw, int pos) {
  auto* cpu = as_cpu(hw);
  if (!cpu) return;
  cpu->IP().Set(pos);
}

void avd_hw_advance_ip(cHardwareBase* hw) {
  auto* cpu = as_cpu(hw);
  if (!cpu) return;
  cpu->IP().Advance();
}

// ---- Stack accessors ----
// Stack operations use the active stack (local or global) for the current thread.

void avd_hw_stack_push(cHardwareBase* hw, int value) {
  auto* cpu = as_cpu(hw);
  if (!cpu) return;
  cpu->FFI_StackPush(value);
}

int avd_hw_stack_pop(cHardwareBase* hw) {
  auto* cpu = as_cpu(hw);
  if (!cpu) return 0;
  return cpu->FFI_StackPop();
}

void avd_hw_switch_stack(cHardwareBase* hw) {
  auto* cpu = as_cpu(hw);
  if (!cpu) return;
  cpu->FFI_SwitchStack();
}

void avd_hw_stack_flip(cHardwareBase* hw) {
  auto* cpu = as_cpu(hw);
  if (!cpu) return;
  cpu->FFI_StackFlip();
}

void avd_hw_stack_clear(cHardwareBase* hw) {
  auto* cpu = as_cpu(hw);
  if (!cpu) return;
  cpu->FFI_StackClear();
}

int avd_hw_get_stack(cHardwareBase* hw, int depth, int stack_id) {
  auto* cpu = as_cpu(hw);
  if (!cpu) return 0;
  return cpu->GetStack(depth, stack_id, -1);
}

int avd_hw_get_cur_stack_id(cHardwareBase* hw) {
  auto* cpu = as_cpu(hw);
  if (!cpu) return 0;
  return cpu->GetCurStack(-1);
}

// ---- Register accessors ----

int avd_hw_get_register(cHardwareBase* hw, int reg_id) {
  auto* cpu = as_cpu(hw);
  if (!cpu || reg_id < 0 || reg_id >= cpu->GetNumRegisters()) return 0;
  return cpu->GetRegister(reg_id);
}

void avd_hw_set_register(cHardwareBase* hw, int reg_id, int value) {
  auto* cpu = as_cpu(hw);
  if (!cpu || reg_id < 0 || reg_id >= cpu->GetNumRegisters()) return;
  cpu->GetRegister(reg_id) = value;
}

// Get the CpuRegisters pointer for the current thread (for Rust register ops).
CpuRegisters* avd_hw_get_regs(cHardwareBase* hw) {
  auto* cpu = as_cpu(hw);
  if (!cpu) return nullptr;
  return cpu->FFI_GetRegs();
}

// ---- Register/nop helpers ----
// FindModifiedRegister has IP side effects (advances past nop, sets flags).
// Exposed as FFI so Rust instruction handlers can call it.

int avd_hw_find_modified_register(cHardwareBase* hw, int default_reg) {
  auto* cpu = as_cpu(hw);
  if (!cpu) return default_reg;
  return cpu->FFI_FindModifiedRegister(default_reg);
}

int avd_hw_find_modified_next_register(cHardwareBase* hw, int default_reg) {
  auto* cpu = as_cpu(hw);
  if (!cpu) return default_reg;
  return cpu->FFI_FindModifiedNextRegister(default_reg);
}

int avd_hw_find_next_register(int base_reg) {
  return (base_reg + 1) % 3;
}

int avd_hw_find_modified_head(cHardwareBase* hw, int default_head) {
  auto* cpu = as_cpu(hw);
  if (!cpu) return default_head;
  return cpu->FFI_FindModifiedHead(default_head);
}

// ---- Memory accessors ----

int avd_hw_get_memory_size(cHardwareBase* hw) {
  auto* cpu = as_cpu(hw);
  if (!cpu) return 0;
  return cpu->GetMemSize();
}

int avd_hw_get_memory_inst(cHardwareBase* hw, int idx) {
  auto* cpu = as_cpu(hw);
  if (!cpu) return -1;
  if (idx < 0 || idx >= cpu->GetMemSize()) return -1;
  return cpu->GetMemory()[idx].GetOp();
}

void avd_hw_set_memory_inst(cHardwareBase* hw, int idx, int inst_id) {
  auto* cpu = as_cpu(hw);
  if (!cpu) return;
  if (idx < 0 || idx >= cpu->GetMemSize()) return;
  cpu->GetMemory()[idx].SetOp(inst_id);
}

int avd_hw_get_memory_flags(cHardwareBase* hw, int idx) {
  auto* cpu = as_cpu(hw);
  if (!cpu) return 0;
  if (idx < 0 || idx >= cpu->GetMemSize()) return 0;
  const cCPUMemory& mem = cpu->GetMemory();
  int flags = 0;
  if (mem.FlagCopied(idx))   flags |= 0x01;
  if (mem.FlagMutated(idx))  flags |= 0x02;
  if (mem.FlagExecuted(idx)) flags |= 0x04;
  if (mem.FlagPointMut(idx)) flags |= 0x08;
  if (mem.FlagCopyMut(idx))  flags |= 0x10;
  if (mem.FlagInjected(idx)) flags |= 0x20;
  return flags;
}

cOrganism* avd_hw_get_organism(cHardwareBase* hw) {
  if (!hw) return nullptr;
  return hw->GetOrganism();
}

int avd_hw_get_inst_set_size(cHardwareBase* hw) {
  if (!hw) return 0;
  return hw->GetInstSet().GetSize();
}

// Read instruction opcode at a raw position (wraps to memory bounds via cHeadCPU).
int avd_hw_get_inst_at_raw_pos(cHardwareBase* hw, int raw_pos) {
  auto* cpu = as_cpu(hw);
  if (!cpu) return -1;
  cHeadCPU temp(cpu, raw_pos, 0);
  return temp.GetInst().GetOp();
}

// ---- Thread accessors ----

int avd_hw_get_thread_id(cHardwareBase* hw) {
  auto* cpu = as_cpu(hw);
  if (!cpu) return -1;
  return cpu->FFI_GetThreadID();
}

int avd_hw_get_cycle_counter(cHardwareBase* hw) {
  auto* cpu = as_cpu(hw);
  if (!cpu) return 0;
  return static_cast<int>(cpu->FFI_GetCycleCounter());
}

// ---- Head manipulation ----

void avd_hw_advance_head(cHardwareBase* hw, int head_id) {
  auto* cpu = as_cpu(hw);
  if (!cpu || head_id < 0 || head_id >= nHardware::NUM_HEADS) return;
  cpu->GetHead(head_id).Advance();
}

// ---- Label operations ----

void avd_hw_read_label(cHardwareBase* hw) {
  auto* cpu = as_cpu(hw);
  if (!cpu) return;
  cpu->FFI_ReadLabel();
}

int avd_hw_get_label_as_int(cHardwareBase* hw, int mode) {
  auto* cpu = as_cpu(hw);
  if (!cpu) return 0;
  // mode: 0=AsInt, 1=GreyCode, 2=Direct, 3=AdditivePolynomial, 4=Fib, 5=PolynomialCoefficient
  switch (mode) {
    case 0: return cpu->FFI_GetLabelAsInt(3);
    case 1: return cpu->FFI_GetLabelAsIntGreyCode(3);
    case 2: return cpu->FFI_GetLabelAsIntDirect(3);
    case 3: return cpu->FFI_GetLabelAsIntAdditivePolynomial(3);
    case 4: return cpu->FFI_GetLabelAsIntFib(3);
    case 5: return cpu->FFI_GetLabelAsIntPolynomialCoefficent(3);
    default: return 0;
  }
}

int avd_hw_if_label_match(cHardwareBase* hw) {
  auto* cpu = as_cpu(hw);
  if (!cpu) return 0;
  return cpu->FFI_IfLabelMatch();
}

int avd_hw_if_label_direct_match(cHardwareBase* hw) {
  auto* cpu = as_cpu(hw);
  if (!cpu) return 0;
  return cpu->FFI_IfLabelDirectMatch();
}

int avd_hw_search_label(cHardwareBase* hw, int direction) {
  auto* cpu = as_cpu(hw);
  if (!cpu) return -1;
  return cpu->FFI_SearchLabel(direction);
}

int avd_hw_get_label_size(cHardwareBase* hw) {
  auto* cpu = as_cpu(hw);
  if (!cpu) return 0;
  return cpu->FFI_GetLabelSize();
}

void avd_hw_set_cur_head(cHardwareBase* hw, int head_id) {
  auto* cpu = as_cpu(hw);
  if (!cpu || head_id < 0 || head_id >= nHardware::NUM_HEADS) return;
  cpu->FFI_SetCurHead(head_id);
}

// ---- cAvidaContext RNG accessors ----
// These take a cAvidaContext* (passed from the instruction dispatch).

int avd_ctx_random_p(cAvidaContext* ctx, double prob) {
  if (!ctx) return 0;
  return ctx->GetRandom().P(prob) ? 1 : 0;
}

int avd_ctx_random_get_uint(cAvidaContext* ctx, unsigned int max) {
  if (!ctx) return 0;
  return static_cast<int>(ctx->GetRandom().GetUInt(max));
}

double avd_ctx_random_get_double(cAvidaContext* ctx) {
  if (!ctx) return 0.0;
  return ctx->GetRandom().GetDouble();
}

int avd_ctx_random_get_int(cAvidaContext* ctx, int max) {
  if (!ctx) return 0;
  return ctx->GetRandom().GetInt(max);
}


// ---- Flash info accessors (m_flash_info on cHardwareCPU) ----

unsigned int avd_hw_get_flash_received(cHardwareBase* hw) {
  auto* cpu = as_cpu(hw);
  if (!cpu) return 0;
  return cpu->GetFlashReceived();
}

unsigned int avd_hw_get_flash_cycle(cHardwareBase* hw) {
  auto* cpu = as_cpu(hw);
  if (!cpu) return 0;
  return cpu->GetFlashCycle();
}

void avd_hw_reset_flash_info(cHardwareBase* hw) {
  auto* cpu = as_cpu(hw);
  if (!cpu) return;
  cpu->ResetFlashInfo();
}

// ---- Label search (direct, no complement rotation) ----

int avd_hw_search_label_direct(cHardwareBase* hw, int direction) {
  auto* cpu = as_cpu(hw);
  if (!cpu) return -1;
  return cpu->FFI_SearchLabelDirect(direction);
}

// ---- Nop detection ----

int avd_hw_is_next_inst_nop(cHardwareBase* hw) {
  auto* cpu = as_cpu(hw);
  if (!cpu) return 0;
  return cpu->FFI_IsNextInstNop();
}

// ---- Hardware state: m_last_cell_data ----

int avd_hw_get_last_cell_data_valid(cHardwareBase* hw) {
  auto* cpu = as_cpu(hw);
  if (!cpu) return 0;
  return cpu->FFI_GetLastCellDataValid();
}

int avd_hw_get_last_cell_data_value(cHardwareBase* hw) {
  auto* cpu = as_cpu(hw);
  if (!cpu) return 0;
  return cpu->FFI_GetLastCellDataValue();
}

// ---- Memory editing (flags, insert/remove) ----
// Flag masks match avd_hw_get_memory_flags: 0x01 copied, 0x02 mutated,
// 0x04 executed, 0x08 point-mut, 0x10 copy-mut, 0x20 injected.

void avd_hw_memory_set_flags(cHardwareBase* hw, int idx, int mask) {
  auto* cpu = as_cpu(hw);
  if (!cpu || idx < 0 || idx >= cpu->GetMemSize()) return;
  cpu->FFI_MemorySetFlags(idx, mask);
}

void avd_hw_memory_clear_flags(cHardwareBase* hw, int idx, int mask) {
  auto* cpu = as_cpu(hw);
  if (!cpu || idx < 0 || idx >= cpu->GetMemSize()) return;
  cpu->FFI_MemoryClearFlags(idx, mask);
}

void avd_hw_memory_insert(cHardwareBase* hw, int idx, int inst_id) {
  auto* cpu = as_cpu(hw);
  if (!cpu) return;
  cpu->FFI_MemoryInsert(idx, inst_id);
}

void avd_hw_memory_remove(cHardwareBase* hw, int idx) {
  auto* cpu = as_cpu(hw);
  if (!cpu) return;
  cpu->FFI_MemoryRemove(idx);
}

// ---- Instruction-set queries ----

int avd_hw_get_nop_mod(cHardwareBase* hw, int inst_id) {
  auto* cpu = as_cpu(hw);
  if (!cpu) return 0;
  return cpu->FFI_GetNopMod(inst_id);
}

int avd_hw_is_nop(cHardwareBase* hw, int inst_id) {
  auto* cpu = as_cpu(hw);
  if (!cpu) return 0;
  return cpu->FFI_IsNop(inst_id);
}

int avd_hw_get_random_inst(cHardwareBase* hw, cAvidaContext* ctx) {
  auto* cpu = as_cpu(hw);
  if (!cpu || !ctx) return 0;
  return cpu->FFI_GetRandomInst(*ctx);
}

int avd_hw_check_no_mut_list(cHardwareBase* hw, int idx) {
  auto* cpu = as_cpu(hw);
  if (!cpu) return 0;
  return cpu->FFI_CheckNoMutList(idx);
}

// ---- Label / head / thread state ----

int avd_hw_get_label_nop(cHardwareBase* hw, int idx) {
  auto* cpu = as_cpu(hw);
  if (!cpu || idx < 0 || idx >= cpu->FFI_GetLabelSize()) return -1;
  return cpu->FFI_GetLabelNop(idx);
}

int avd_hw_get_cur_head(cHardwareBase* hw) {
  auto* cpu = as_cpu(hw);
  if (!cpu) return 0;
  return cpu->FFI_GetCurHead();
}

void avd_hw_read_inst(cHardwareBase* hw, int inst_id) {
  auto* cpu = as_cpu(hw);
  if (!cpu) return;
  cpu->FFI_ReadInst(inst_id);
}

void avd_hw_set_advance_ip(cHardwareBase* hw, int advance) {
  auto* cpu = as_cpu(hw);
  if (!cpu) return;
  cpu->FFI_SetAdvanceIP(advance != 0);
}

int avd_hw_get_slip_read_head(cHardwareBase* hw) {
  auto* cpu = as_cpu(hw);
  if (!cpu) return 0;
  return cpu->FFI_GetSlipReadHead();
}

int avd_hw_fork_thread(cHardwareBase* hw) {
  auto* cpu = as_cpu(hw);
  if (!cpu) return 0;
  return cpu->FFI_ForkThread() ? 1 : 0;
}

int avd_hw_kill_thread(cHardwareBase* hw) {
  auto* cpu = as_cpu(hw);
  if (!cpu) return 0;
  return cpu->FFI_KillThread() ? 1 : 0;
}

void avd_hw_fault(cHardwareBase* hw, int fault_loc, int fault_type, const char* desc) {
  if (!hw || !hw->GetOrganism()) return;
  hw->GetOrganism()->Fault(fault_loc, fault_type, desc ? desc : "");
}

// ---- Copy mutations ----
// kind: 0 = point (mut), 1 = insertion, 2 = deletion, 3 = uniform, 4 = slip.

int avd_org_test_copy(cOrganism* org, cAvidaContext* ctx, int kind) {
  if (!org || !ctx) return 0;
  switch (kind) {
    case 0: return org->TestCopyMut(*ctx) ? 1 : 0;
    case 1: return org->TestCopyIns(*ctx) ? 1 : 0;
    case 2: return org->TestCopyDel(*ctx) ? 1 : 0;
    case 3: return org->TestCopyUniform(*ctx) ? 1 : 0;
    case 4: return org->TestCopySlip(*ctx) ? 1 : 0;
    default: return 0;
  }
}

double avd_org_get_copy_prob(cOrganism* org, int kind) {
  if (!org) return 0.0;
  switch (kind) {
    case 0: return org->GetCopyMutProb();
    case 1: return org->GetCopyInsProb();
    case 2: return org->GetCopyDelProb();
    case 3: return org->GetCopyUniformProb();
    case 4: return org->GetCopySlipProb();
    default: return 0.0;
  }
}

void avd_hw_do_uniform_copy_mutation(cHardwareBase* hw, cAvidaContext* ctx, int head_id) {
  auto* cpu = as_cpu(hw);
  if (!cpu || !ctx || head_id < 0 || head_id >= nHardware::NUM_HEADS) return;
  cpu->FFI_DoUniformCopyMutation(*ctx, head_id);
}

void avd_hw_do_slip_mutation(cHardwareBase* hw, cAvidaContext* ctx, int pos) {
  auto* cpu = as_cpu(hw);
  if (!cpu || !ctx) return;
  cpu->FFI_DoSlipMutation(*ctx, pos);
}

// ---- Allocation / division (genome ownership stays in C++) ----

int avd_hw_allocate_main(cHardwareBase* hw, cAvidaContext* ctx, int size) {
  auto* cpu = as_cpu(hw);
  if (!cpu || !ctx) return 0;
  return cpu->FFI_AllocateMain(*ctx, size) ? 1 : 0;
}

// kind: 0 = Divide_Main, 1 = Divide_MainRS, 2 = Divide_Main1RS, 3 = Divide_Main2RS.
int avd_hw_divide_main(cHardwareBase* hw, cAvidaContext* ctx, int div_point, int extra_lines,
                       double mut_multiplier, int kind) {
  auto* cpu = as_cpu(hw);
  if (!cpu || !ctx) return 0;
  return cpu->FFI_DivideMain(*ctx, div_point, extra_lines, mut_multiplier, kind) ? 1 : 0;
}

// cHeadCPU::Adjust on one head of the current thread; returns the adjusted position.
int avd_hw_adjust_head(cHardwareBase* hw, int head_id) {
  auto* cpu = as_cpu(hw);
  if (!cpu || head_id < 0 || head_id >= nHardware::NUM_HEADS) return -1;
  cHeadCPU& head = cpu->GetHead(head_id);
  head.Adjust();
  return head.GetPosition();
}

void avd_hw_adjust_heads(cHardwareBase* hw) {
  auto* cpu = as_cpu(hw);
  if (!cpu) return;
  cpu->FFI_AdjustHeads();
}

// Opcode of the named instruction in this hardware's instruction set
// (the error opcode, 255, when the set does not contain it).
int avd_hw_find_inst_op(cHardwareBase* hw, const char* name) {
  auto* cpu = as_cpu(hw);
  if (!cpu || !name) return 255;
  return cpu->FFI_FindInstOp(name);
}

// ---- Phenotype reproduction flags ----

void avd_org_set_divide_sex(cOrganism* org, int divide_sex) {
  if (!org) return;
  org->GetPhenotype().SetDivideSex(divide_sex != 0);
}

void avd_org_set_cross_num(cOrganism* org, int cross_num) {
  if (!org) return;
  org->GetPhenotype().SetCrossNum(cross_num);
}

void avd_org_set_mate_select_id(cOrganism* org, int select_id) {
  if (!org) return;
  org->GetPhenotype().SetMateSelectID(select_id);
}

int avd_org_born_parent_group(cOrganism* org) {
  if (!org) return 0;
  return org->GetPhenotype().BornParentGroup() ? 1 : 0;
}

} // extern "C"
