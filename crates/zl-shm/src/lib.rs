#[derive(Debug)]
pub enum ShmError {
    NotImplemented,
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
