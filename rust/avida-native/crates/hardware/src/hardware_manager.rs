//! Port of `cHardwareManager` (avida-core/source/cpu/cHardwareManager.{h,cc}
//! at 47f13dadb): instruction-set loading (through the genome crate's
//! [`InstSetRegistry`]), the hardware-type -> instruction library map, and
//! `Create` (hardware construction for an organism).
//!
//! Loud start-up failures (see the crate documentation):
//!
//! * a hardware type the C++ has no hardware for;
//! * an instruction whose handler is not ported (a structural check: every
//!   handler table is complete).

use crate::cpu_inst_lib::CPU_INST_DEFS;
use crate::hardware::Hardware;
use crate::hardware_cpu::HardwareCpu;
use crate::hardware_cpu_insts::handler_ported;
use crate::nhardware::*;
use crate::organism::OrgState;
use avida_config::config::Config;
use avida_genome::inst_lib::{BehavClass, InstLib, InstLibEntry};
use avida_genome::inst_set::InstCodeConfig;
use avida_genome::{Feedback, Genome, InstSet, InstSetRegistry};
use std::sync::{Arc, OnceLock};

/// `cHardwareCPU::GetInstLib()`: built once from [`CPU_INST_DEFS`].
pub fn cpu_inst_lib() -> Arc<InstLib> {
    static LIB: OnceLock<Arc<InstLib>> = OnceLock::new();
    LIB.get_or_init(|| {
        let entries = CPU_INST_DEFS
            .iter()
            .map(|d| InstLibEntry::new(d.name, d.class, d.flags, d.desc, BehavClass::None))
            .collect();
        // `s_n_array`: nop-A/B/C modify AX/BX/CX.
        let nops = vec![
            ("nop-A".to_string(), 0),
            ("nop-B".to_string(), 1),
            ("nop-C".to_string(), 2),
        ];
        let null_inst = CPU_INST_DEFS.len() as i32 - 1;
        Arc::new(InstLib::new(entries, nops, 0, null_inst))
    })
    .clone()
}

/// `cHardwareManager`.
#[derive(Debug, Default)]
pub struct HardwareManager {
    registry: InstSetRegistry,
}

impl HardwareManager {
    /// `cHardwareManager(world)`.
    pub fn new() -> Self {
        HardwareManager {
            registry: InstSetRegistry::new(),
        }
    }

    /// The instruction-set registry (for genome loading and the
    /// `GetInstSet` family).
    pub fn registry(&self) -> &InstSetRegistry {
        &self.registry
    }

    /// The registry, mutable (`RegisterInstSet`, `SetProbFail`, ...).
    pub fn registry_mut(&mut self) -> &mut InstSetRegistry {
        &mut self.registry
    }

    /// `LoadInstSets(feedback)` over `cfg.INSTSETS`.
    ///
    /// `Ok(false)` reports load errors through `feedback` as the C++ does;
    /// `Err` is a loud "unsupported" failure (a hardware type or an
    /// instruction handler that is not ported).
    pub fn load_inst_sets(
        &mut self,
        cfg_list: &[String],
        code_cfg: InstCodeConfig,
        feedback: &mut Feedback,
    ) -> Result<bool, String> {
        let mut provider = |hw_type: i32| -> Option<Arc<InstLib>> {
            match hw_type {
                HARDWARE_TYPE_CPU_ORIGINAL => Some(cpu_inst_lib()),
                HARDWARE_TYPE_CPU_TRANSSMT => {
                    Some(crate::hardware_transsmt_insts::transsmt_inst_lib())
                }
                HARDWARE_TYPE_CPU_EXPERIMENTAL => {
                    Some(crate::hardware_experimental_insts::exp_inst_lib())
                }
                HARDWARE_TYPE_CPU_GP8 => Some(crate::hardware_gp8_insts::gp8_inst_lib()),
                HARDWARE_TYPE_CPU_BCR => Some(crate::hardware_bcr_insts::bcr_inst_lib()),
                _ => None,
            }
        };
        let ok = self
            .registry
            .load_inst_sets(cfg_list, code_cfg, &mut provider, feedback);
        self.check_ported()?;
        Ok(ok)
    }

    /// Fail loudly if a loaded set uses an instruction whose handler is not
    /// ported.
    pub fn check_ported(&self) -> Result<(), String> {
        for is in self.registry.inst_sets() {
            check_inst_set_ported(is)?;
        }
        Ok(())
    }

    /// `m_is_name_map` lookup plus the hardware-type check of `Create`.
    pub fn inst_set_for_genome(&self, genome: &Genome) -> Result<Arc<InstSet>, String> {
        let name = genome.inst_set();
        let id = self.registry.inst_set_id(name);
        if id < 0 {
            return Err(format!(
                "no instruction set named '{}' (C++: cHardwareManager::Create returns NULL)",
                name
            ));
        }
        let is = self
            .registry
            .shared_inst_set(id)
            .ok_or_else(|| format!("instruction set '{}' not loaded", name))?;
        if is.hardware_type() != genome.hardware_type() {
            return Err(format!(
                "instruction set '{}' has hw_type {} but the genome has hw_type {} (C++: cHardwareManager::Create returns NULL)",
                name,
                is.hardware_type(),
                genome.hardware_type()
            ));
        }
        Ok(is)
    }

    /// `Create(ctx, org, genome)` with the instruction set already looked up
    /// by [`HardwareManager::inst_set_for_genome`]: build the hardware and
    /// run its constructor-time `Reset` (no interface is attached yet, so
    /// `cOrganism::HardwareReset` only clears the opinion and display).
    pub fn create(
        &self,
        cfg: &Config,
        org: &mut OrgState,
        inst_set: Arc<InstSet>,
    ) -> Result<Box<dyn Hardware>, String> {
        match inst_set.hardware_type() {
            HARDWARE_TYPE_CPU_ORIGINAL => {
                check_inst_set_ported(&inst_set)?;
                crate::hardware_cpu::check_inst_set(cfg, &inst_set)?;
                Ok(Box::new(HardwareCpu::new(cfg, org, inst_set)))
            }
            HARDWARE_TYPE_CPU_TRANSSMT => Ok(Box::new(
                crate::hardware_transsmt::HardwareTransSmt::new(cfg, org, inst_set),
            )),
            HARDWARE_TYPE_CPU_EXPERIMENTAL => {
                crate::hardware_experimental_insts::check_exp_inst_set_ported(&inst_set)?;
                Ok(Box::new(
                    crate::hardware_experimental::HardwareExperimental::new(cfg, org, inst_set),
                ))
            }
            HARDWARE_TYPE_CPU_GP8 => {
                crate::hardware_gp8_insts::check_gp8_inst_set_ported(&inst_set)?;
                Ok(Box::new(crate::hardware_gp8::HardwareGp8::new(
                    cfg, org, inst_set,
                )))
            }
            HARDWARE_TYPE_CPU_BCR => {
                crate::hardware_bcr_insts::check_bcr_inst_set_ported(&inst_set)?;
                Ok(Box::new(crate::hardware_bcr::HardwareBcr::new(
                    cfg, org, inst_set,
                )))
            }
            hw => Err(format!(
                "unsupported: hw_type={} (C++: cHardwareManager.cc:cHardwareManager::Create)",
                hw
            )),
        }
    }
}

/// Error if `is` (a heads-CPU set) uses an unported instruction.
pub fn check_inst_set_ported(is: &InstSet) -> Result<(), String> {
    if is.hardware_type() == HARDWARE_TYPE_CPU_EXPERIMENTAL {
        return crate::hardware_experimental_insts::check_exp_inst_set_ported(is);
    }
    if is.hardware_type() == HARDWARE_TYPE_CPU_GP8 {
        return crate::hardware_gp8_insts::check_gp8_inst_set_ported(is);
    }
    if is.hardware_type() == HARDWARE_TYPE_CPU_BCR {
        return crate::hardware_bcr_insts::check_bcr_inst_set_ported(is);
    }
    if is.hardware_type() != HARDWARE_TYPE_CPU_ORIGINAL {
        return Ok(());
    }
    for op in 0..is.size() {
        let idx = is.lib_function_index(avida_genome::Instruction::new(op));
        let Some(def) = CPU_INST_DEFS.get(idx.max(0) as usize) else {
            continue;
        };
        if !handler_ported(def.method) {
            return Err(format!(
                "unsupported: instruction '{}' in instruction set '{}' (C++: cHardwareCPU.cc:cHardwareCPU::{})",
                def.name,
                is.inst_set_name(),
                def.method
            ));
        }
    }
    Ok(())
}
