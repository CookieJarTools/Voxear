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
///
/// A scope runs from when the GPU starts its commands to when they finish.
/// The GPU can start a scope's commands while the last's still run, and a
/// scope after a barrier counts the wait for the work before it, so a light
/// pass after a heavy one can read as heavy, and the scopes added up are not
/// the frame's time. That is from the first scope's start to the last's end.
pub struct GpuTimer {
    slots: Vec<SlotQueries>,
    results: Vec<(String, f32)>,
    frame_ms: f32,
    period_ns: f32,
}

impl GpuTimer {
    pub fn new(gpu: &Gpu, slots: usize) -> Result<Self> {
        let mut queries = Vec::with_capacity(slots);
        for _ in 0..slots {
            let info =
                vk::QueryPoolCreateInfo::default().query_type(vk::QueryType::TIMESTAMP).query_count(MAX_SCOPES * 2);
            let pool = unsafe { gpu.device.create_query_pool(&info, None)? };
            unsafe { gpu.device.reset_query_pool(pool, 0, MAX_SCOPES * 2) };
            queries.push(SlotQueries { pool, names: Vec::new() });
        }
        Ok(Self { slots: queries, results: Vec::new(), frame_ms: 0.0, period_ns: gpu.info.timestamp_period })
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
                let first = stamps.iter().step_by(2).copied().min().unwrap_or(0);
                let last = stamps.iter().skip(1).step_by(2).copied().max().unwrap_or(0);
                self.frame_ms = last.saturating_sub(first) as f32 * self.period_ns / 1_000_000.0;
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

    /// The most recent completed frame's GPU time, in milliseconds: from its
    /// first scope's start to its last's end.
    pub fn frame_ms(&self) -> f32 {
        self.frame_ms
    }

    pub fn destroy(&mut self, gpu: &Gpu) {
        for slot in self.slots.drain(..) {
            unsafe { gpu.device.destroy_query_pool(slot.pool, None) };
        }
    }
}
