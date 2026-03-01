#[derive(Debug, Default, Clone, Copy)]
pub struct TopicStats {
    pub p50_latency_us: u64,
    pub p95_latency_us: u64,
    pub throughput_per_sec: u64,
    pub drops: u64,
    pub queue_depth: u64,
}
