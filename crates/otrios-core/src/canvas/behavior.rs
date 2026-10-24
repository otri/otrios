// SPDX-License-Identifier: LGPL-3.0-only
use super::*;
use wasmi::{Config, Engine, Linker, Module, Store, StoreLimits, StoreLimitsBuilder, TypedFunc};
pub struct Behavior {
    digest: Vec<u8>,
    store: Store<StoreLimits>,
    dispatch: TypedFunc<(i32, i32, i32, i32), i64>,
    writes: Vec<u16>,
}
impl Behavior {
    pub fn load(bytes: &[u8], reference: &BehaviorRef) -> Result<Self> {
        if bytes.len() > 65536
            || bytes.is_empty()
            || reference.abi != 1
            || reference.sha256 != hash(bytes)
            || reference.writes.iter().any(|v| !(1..=5).contains(v))
        {
            return Err("Behavior identity/ABI/permissions invalid".into());
        }
        let mut config = Config::default();
        config
            .consume_fuel(true)
            .set_max_recursion_depth(32)
            .set_max_stack_height(1024);
        let engine = Engine::new(&config);
        let module = Module::new(&engine, bytes).map_err(err)?;
        if module.imports().next().is_some() {
            return Err("Behavior imports are prohibited".into());
        }
        let limits = StoreLimitsBuilder::new()
            .memory_size(65536)
            .memories(1)
            .tables(0)
            .instances(1)
            .build();
        let mut store = Store::new(&engine, limits);
        store.limiter(|s| s);
        store.set_fuel(10000).map_err(err)?;
        let instance = Linker::new(&engine)
            .instantiate_and_start(&mut store, &module)
            .map_err(err)?;
        let version = instance
            .get_typed_func::<(), i32>(&store, "abi_version")
            .map_err(err)?;
        if version.call(&mut store, ()).map_err(err)? != 1 {
            return Err("Wrong behavior ABI version".into());
        }
        let dispatch = instance.get_typed_func(&store, "dispatch").map_err(err)?;
        Ok(Self {
            digest: reference.sha256.clone(),
            store,
            dispatch,
            writes: reference.writes.clone(),
        })
    }
    pub fn code_hash(&self) -> &[u8] {
        &self.digest
    }
    pub fn call(&mut self, event: u16, a: u16, b: u16, current: u16) -> Result<Vec<(u16, u16)>> {
        if a > 1000 || b > 1000 || current > 1000 {
            return Err("Event value outside domain".into());
        }
        self.store.set_fuel(10000).map_err(err)?;
        let result = self
            .dispatch
            .call(
                &mut self.store,
                (event.into(), a.into(), b.into(), current.into()),
            )
            .map_err(err)? as u64;
        if result >> 55 != 0 {
            return Err("Invalid behavior command bits".into());
        }
        let mask = (result >> 50) & 31;
        let mut commands = Vec::new();
        for lane in 0..5 {
            let value = ((result >> (lane * 10)) & 1023) as u16;
            let id = lane as u16 + 1;
            if mask & (1 << lane) != 0 {
                if value > 1000 || !self.writes.contains(&id) {
                    return Err("Unauthorized behavior command".into());
                }
                commands.push((id, value));
            } else if value != 0 {
                return Err("Noncanonical unused command lane".into());
            }
        }
        Ok(commands)
    }
}
