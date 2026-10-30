use anyhow::Result;
use ash::vk;

use crate::device::Gpu;

const MAX_SCOPES: u32 = 64;

struct SlotQueries {
    pool: vk::QueryPool,
    names: Vec<String>,
}

/// GPU time per named scope, measured with timestamp queries. Results arrive
/// a few frames late, once the slot that recorded them comes round again.
pub struct GpuTimer {
    slots: Vec<SlotQueries>,
    results: Vec<(String, f32)>,
    period_ns: f32,
}

impl GpuTimer {
    pub fn new(gpu: &Gpu, slots: usize) -> Result<Self> {
        let mut queries = Vec::with_capacity(slots);
        for _ in 0..slots {
            let info = vk::QueryPoolCreateInfo::default().query_type(vk::QueryType::TIMESTAMP).query_count(MAX_SCOPES * 2);
            let pool = unsafe { gpu.device.create_query_pool(&info, None)? };
            unsafe { gpu.device.reset_query_pool(pool, 0, MAX_SCOPES * 2) };
            queries.push(SlotQueries { pool, names: Vec::new() });
        }
        Ok(Self { slots: queries, results: Vec::new(), period_ns: gpu.info.timestamp_period })
    }

    /// Collects the slot's previous results and resets it. Call after the
    /// slot's fence has been waited on.
    pub fn begin_frame(&mut self, gpu: &Gpu, slot: usize) {
        let queries = &mut self.slots[slot];
        let count = queries.names.len() as u32;
        if count > 0 {
            let mut stamps = vec![0u64; count as usize * 2];
            let read = unsafe {
                gpu.device.get_query_pool_results(queries.pool, 0, &mut stamps, vk::QueryResultFlags::TYPE_64)
            };
            if read.is_ok() {
                self.results = queries
                    .names
                    .iter()
                    .enumerate()
                    .map(|(i, name)| {
                        let ticks = stamps[i * 2 + 1].saturating_sub(stamps[i * 2]);
                        (name.clone(), ticks as f32 * self.period_ns / 1_000_000.0)
                    })
                    .collect();
            }
            unsafe { gpu.device.reset_query_pool(queries.pool, 0, count * 2) };
        }
        queries.names.clear();
    }

    /// Starts a scope; returns its index for [`end`](Self::end), or `None`
    /// when the slot is out of queries.
    pub fn begin(&mut self, gpu: &Gpu, cmd: vk::CommandBuffer, slot: usize, name: &str) -> Option<u32> {
        let queries = &mut self.slots[slot];
        let index = queries.names.len() as u32;
        if index >= MAX_SCOPES {
            return None;
        }
        queries.names.push(name.to_string());
        unsafe { gpu.device.cmd_write_timestamp2(cmd, vk::PipelineStageFlags2::TOP_OF_PIPE, queries.pool, index * 2) };
        Some(index)
    }

    pub fn end(&mut self, gpu: &Gpu, cmd: vk::CommandBuffer, slot: usize, scope: Option<u32>) {
        let Some(index) = scope else { return };
        let pool = self.slots[slot].pool;
        unsafe { gpu.device.cmd_write_timestamp2(cmd, vk::PipelineStageFlags2::BOTTOM_OF_PIPE, pool, index * 2 + 1) };
    }

    /// Milliseconds per scope from the most recent completed frame.
    pub fn results(&self) -> &[(String, f32)] {
        &self.results
    }

    pub fn destroy(&mut self, gpu: &Gpu) {
        for slot in self.slots.drain(..) {
            unsafe { gpu.device.destroy_query_pool(slot.pool, None) };
        }
    }
}
