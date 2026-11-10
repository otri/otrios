// SPDX-License-Identifier: LGPL-3.0-only
//! Bounded component ABI execution. Package trust and command authority are
//! checked by the caller; this boundary owns Wasm resource and byte validation.
//!
//! A `Guest` reserves one occurrence slot, including while it is staged. Each
//! call starts with a fresh Store, so linear memory and globals are scratch and
//! cannot become hidden resumable state. Immutable compiled code is shared.
use super::{Result, err};
use otrios_interfaces::oib::component_behavior::{
    ABI, DISPATCH_EXPORT, INPUT_BUFFER_EXPORT, Input, MAX_MESSAGE_BYTES, MEMORY_EXPORT, Output,
    VERSION_EXPORT, decode_output, encode_input,
};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicUsize, Ordering},
    },
};
use wasmi::{
    Config, Engine, ExternType, Linker, Memory, Module, Store, StoreLimits, StoreLimitsBuilder,
    TypedFunc,
};

pub const MAX_CODE_BYTES: usize = 65_536;
pub const MAX_INSTANCES: usize = 8;
pub const FUEL_PER_CALL: u64 = 10_000;
type CodeHash = [u8; 32];

#[derive(Default)]
struct Budget {
    live: AtomicUsize,
}
impl Budget {
    fn reserve(self: &Arc<Self>) -> Result<Lease> {
        self.live
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |live| {
                (live < MAX_INSTANCES).then_some(live + 1)
            })
            .map_err(|_| "Component behavior instance budget".to_string())?;
        Ok(Lease(self.clone()))
    }
}
struct Lease(Arc<Budget>);
impl Drop for Lease {
    fn drop(&mut self) {
        let previous = self.0.live.fetch_sub(1, Ordering::AcqRel);
        debug_assert!(previous > 0);
    }
}

/// One app's shared active/staged execution budget and immutable module cache.
#[derive(Default)]
pub struct Runtime {
    modules: Mutex<BTreeMap<CodeHash, Weak<Compiled>>>,
    budget: Arc<Budget>,
}
impl Runtime {
    pub fn live_instances(&self) -> usize {
        self.budget.live.load(Ordering::Acquire)
    }
    /// Call before staging a whole candidate. Individual allocations also check
    /// the shared limit, so a concurrent caller cannot exceed it.
    pub fn check_capacity(&self, additional: usize) -> Result<()> {
        if self
            .live_instances()
            .checked_add(additional)
            .is_none_or(|count| count > MAX_INSTANCES)
        {
            return Err("Component behavior instance budget".into());
        }
        Ok(())
    }
    /// Compile exact already-authenticated package bytes. A digest checks byte
    /// identity; it is not a signature, capability or publisher grant.
    pub fn compile(&self, bytes: &[u8], expected: CodeHash) -> Result<Arc<Compiled>> {
        let _profile = super::profiling::span("component.behavior.compile");
        if bytes.is_empty() || bytes.len() > MAX_CODE_BYTES {
            return Err("Component behavior code byte budget".into());
        }
        let actual: CodeHash = Sha256::digest(bytes).into();
        if actual != expected {
            return Err("Component behavior code digest mismatch".into());
        }
        let mut modules = self
            .modules
            .lock()
            .map_err(|_| "Component behavior module cache poisoned")?;
        modules.retain(|_, module| module.strong_count() != 0);
        if let Some(module) = modules.get(&actual).and_then(Weak::upgrade) {
            return Ok(module);
        }
        if modules.len() >= MAX_INSTANCES {
            return Err("Component behavior module cache budget".into());
        }
        // Wasmi's Engine owns its compiled-function arena. Give each cached
        // revision its own Engine so retiring its last Arc also frees code;
        // one long-lived Engine would accumulate retired module bodies.
        let mut config = Config::default();
        config
            .consume_fuel(true)
            .set_max_recursion_depth(32)
            .set_max_stack_height(1024);
        let engine = Engine::new(&config);
        let module = Module::new(&engine, bytes).map_err(err)?;
        if module.imports().next().is_some() {
            return Err("Component behavior imports are prohibited".into());
        }
        let Some(ExternType::Memory(memory)) = module.get_export(MEMORY_EXPORT) else {
            return Err("Component behavior requires exported memory".into());
        };
        if memory.is_64()
            || memory.minimum() != 1
            || memory.maximum() != Some(1)
            || module
                .exports()
                .filter(|export| matches!(export.ty(), ExternType::Memory(_)))
                .count()
                != 1
            || module
                .exports()
                .any(|export| matches!(export.ty(), ExternType::Table(_)))
        {
            return Err("Component behavior requires one fixed 64 KiB memory and no tables".into());
        }
        let compiled = Arc::new(Compiled {
            digest: actual,
            engine,
            module,
            budget: self.budget.clone(),
        });
        // Admission runs start/version/buffer exports under the same ceilings.
        // This transient Store counts against the app's active/staged budget.
        let _lease = self.budget.reserve()?;
        let _scratch = Scratch::load(&compiled)?;
        modules.insert(actual, Arc::downgrade(&compiled));
        Ok(compiled)
    }
    pub fn instantiate(&self, compiled: Arc<Compiled>) -> Result<Guest> {
        if !Arc::ptr_eq(&compiled.budget, &self.budget) {
            return Err("Component module belongs to another app runtime".into());
        }
        let lease = self.budget.reserve()?;
        let _scratch = Scratch::load(&compiled)?;
        Ok(Guest {
            compiled,
            _lease: lease,
        })
    }
}

/// Shared immutable Wasmi module. No instance memory or host state is shared.
pub struct Compiled {
    digest: CodeHash,
    engine: Engine,
    module: Module,
    budget: Arc<Budget>,
}
impl Compiled {
    pub fn code_hash(&self) -> CodeHash {
        self.digest
    }
}
pub struct Guest {
    compiled: Arc<Compiled>,
    _lease: Lease,
}
impl Guest {
    pub fn code_hash(&self) -> CodeHash {
        self.compiled.digest
    }
    /// Returns only a complete, canonical output. The caller must validate all
    /// declared properties/outlets and capture authority before applying it.
    pub fn call(&mut self, input: &Input) -> Result<Output> {
        let _profile = super::profiling::span("component.behavior.call");
        let bytes = encode_input(input)?;
        let mut scratch = Scratch::load(&self.compiled)?;
        scratch
            .memory
            .write(&mut scratch.store, scratch.input, &bytes)
            .map_err(err)?;
        let packed = {
            let _profile = super::profiling::span("component.behavior.dispatch");
            scratch
                .dispatch
                .call(
                    &mut scratch.store,
                    (scratch.input as i32, bytes.len() as i32),
                )
                .map_err(err)?
        } as u64;
        let offset = (packed & u64::from(u32::MAX)) as usize;
        let length = (packed >> 32) as usize;
        let range = memory_range(offset, length, scratch.memory.data_size(&scratch.store))?;
        decode_output(&scratch.memory.data(&scratch.store)[range])
    }
}
fn memory_range(offset: usize, length: usize, available: usize) -> Result<std::ops::Range<usize>> {
    if length > MAX_MESSAGE_BYTES {
        return Err("Component behavior message byte budget".into());
    }
    let end = offset
        .checked_add(length)
        .filter(|end| *end <= available)
        .ok_or("Component behavior memory range")?;
    Ok(offset..end)
}
struct Scratch {
    store: Store<StoreLimits>,
    memory: Memory,
    input: usize,
    dispatch: TypedFunc<(i32, i32), i64>,
}
impl Scratch {
    fn load(compiled: &Compiled) -> Result<Self> {
        let _profile = super::profiling::span("component.behavior.load");
        let limits = StoreLimitsBuilder::new()
            .memory_size(MAX_CODE_BYTES)
            .memories(1)
            .tables(0)
            .instances(1)
            .build();
        let mut store = Store::new(&compiled.engine, limits);
        store.limiter(|limits| limits);
        store.set_fuel(FUEL_PER_CALL).map_err(err)?;
        let instance = Linker::new(&compiled.engine)
            .instantiate_and_start(&mut store, &compiled.module)
            .map_err(err)?;
        let memory = instance
            .get_memory(&store, MEMORY_EXPORT)
            .ok_or("Missing component memory")?;
        if memory.data_size(&store) != MAX_CODE_BYTES {
            return Err("Component memory must be exactly 64 KiB".into());
        }
        let version = instance
            .get_typed_func::<(), i32>(&store, VERSION_EXPORT)
            .map_err(err)?;
        if version.call(&mut store, ()).map_err(err)? != i32::from(ABI) {
            return Err("Unsupported component behavior ABI".into());
        }
        let input_buffer = instance
            .get_typed_func::<(), i32>(&store, INPUT_BUFFER_EXPORT)
            .map_err(err)?;
        let input = usize::try_from(input_buffer.call(&mut store, ()).map_err(err)?)
            .map_err(|_| "Negative component input buffer")?;
        memory_range(input, MAX_MESSAGE_BYTES, memory.data_size(&store))?;
        let dispatch = instance
            .get_typed_func::<(i32, i32), i64>(&store, DISPATCH_EXPORT)
            .map_err(err)?;
        Ok(Self {
            store,
            memory,
            input,
            dispatch,
        })
    }
}

#[cfg(test)]
mod tests;
