use std::collections::HashMap;

#[derive(Debug)]
pub enum ShmError {
    InvalidArg,
    ShmExhausted,
    NotFound,
    InvalidRange,
}

pub type ShmResult<T> = Result<T, ShmError>;

#[derive(Debug, Clone, Copy)]
pub struct ShmConfig {
    pub hard_limit_bytes: u64,
    pub sweep_interval_ms: u64,
}

impl Default for ShmConfig {
    fn default() -> Self {
        Self {
            hard_limit_bytes: 512 * 1024 * 1024,
            sweep_interval_ms: 100,
        }
    }
}

pub struct ShmManager {
    config: ShmConfig,
    next_id: u64,
    used_bytes: u64,
    slots: HashMap<u64, Box<[u8]>>,
}

impl ShmManager {
    pub fn new(config: ShmConfig) -> Self {
        Self {
            config,
            next_id: 1,
            used_bytes: 0,
            slots: HashMap::new(),
        }
    }

    pub fn alloc(&mut self, size: u32) -> ShmResult<(u64, *mut u8)> {
        if size == 0 {
            return Err(ShmError::InvalidArg);
        }
        let size_u64 = size as u64;
        if self.used_bytes.saturating_add(size_u64) > self.config.hard_limit_bytes {
            return Err(ShmError::ShmExhausted);
        }

        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        if id == 0 {
            return Err(ShmError::InvalidArg);
        }

        let mut data = vec![0u8; size as usize].into_boxed_slice();
        let ptr = data.as_mut_ptr();
        self.slots.insert(id, data);
        self.used_bytes = self.used_bytes.saturating_add(size_u64);
        Ok((id, ptr))
    }

    pub fn release(&mut self, id: u64) -> ShmResult<()> {
        if id == 0 {
            return Err(ShmError::InvalidArg);
        }
        let Some(slot) = self.slots.remove(&id) else {
            return Err(ShmError::NotFound);
        };
        self.used_bytes = self.used_bytes.saturating_sub(slot.len() as u64);
        Ok(())
    }

    pub fn read_range(&self, id: u64, offset: u32, length: u32) -> ShmResult<Vec<u8>> {
        let Some(slot) = self.slots.get(&id) else {
            return Err(ShmError::NotFound);
        };
        let start = offset as usize;
        let len = length as usize;
        let end = start.saturating_add(len);
        if end > slot.len() {
            return Err(ShmError::InvalidRange);
        }
        Ok(slot[start..end].to_vec())
    }
}

impl Default for ShmManager {
    fn default() -> Self {
        Self::new(ShmConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alloc_and_release_updates_usage() {
        let mut m = ShmManager::new(ShmConfig {
            hard_limit_bytes: 16,
            sweep_interval_ms: 100,
        });
        let (id, _ptr) = m.alloc(8).expect("alloc should succeed");
        assert!(m.release(id).is_ok());
    }

    #[test]
    fn alloc_respects_hard_limit() {
        let mut m = ShmManager::new(ShmConfig {
            hard_limit_bytes: 4,
            sweep_interval_ms: 100,
        });
        assert!(matches!(m.alloc(8), Err(ShmError::ShmExhausted)));
    }
}
