use crate::presentation::frame_trace;
use serde_json::json;
use smithay::backend::renderer::gles::{GlesRenderer, ffi};
use std::collections::VecDeque;

const MAX_PENDING: usize = 8;

#[derive(Default)]
pub(super) struct Timing {
    supported: Option<bool>,
    waiting: VecDeque<Sample>,
}

pub(super) struct Sample {
    queries: [u32; 2],
    render: u64,
    output: u64,
    started: u64,
    submitted: u64,
}

impl Timing {
    pub(super) fn begin(
        &mut self,
        renderer: &mut GlesRenderer,
        render: u64,
        output: u64,
        started: u64,
    ) -> Option<Sample> {
        if self.supported.is_none() {
            let supported = renderer.supports_disjoint_timer_query()
                && renderer
                    .with_context(|gl| {
                        let mut bits = 0;
                        // SAFETY: This renderer has a current context and all timer entry points were checked.
                        unsafe {
                            gl.GetQueryivEXT(
                                ffi::TIMESTAMP_EXT,
                                ffi::QUERY_COUNTER_BITS_EXT,
                                &mut bits,
                            )
                        };
                        bits > 0
                    })
                    .unwrap_or(false);
            self.supported = Some(supported);
            frame_trace::record(
                json!({"event":"gles-timestamp-capability", "supported":supported}),
            );
        }
        if self.supported != Some(true) {
            return None;
        }
        self.collect(renderer);
        if self.supported != Some(true) {
            return None;
        }
        if self.waiting.len() >= MAX_PENDING {
            frame_trace::record(json!({"event":"gles-timestamp-skipped", "render":render,
                "output":output, "composition_started_us":started, "reason":"pending query limit"}));
            return None;
        }
        let queries = renderer
            .with_context(|gl| {
                let mut queries = [0; 2];
                // SAFETY: The current renderer owns the query names and the output array has two entries.
                unsafe {
                    gl.GenQueriesEXT(2, queries.as_mut_ptr());
                    if !queries.contains(&0) {
                        gl.QueryCounterEXT(queries[0], ffi::TIMESTAMP_EXT);
                    } else {
                        gl.DeleteQueriesEXT(2, queries.as_ptr());
                    }
                }
                (!queries.contains(&0)).then_some(queries)
            })
            .ok()
            .flatten();
        let Some(queries) = queries else {
            frame_trace::record(
                json!({"event":"gles-timestamp-unavailable", "render":render,
                "output":output, "composition_started_us":started, "reason":"query allocation or context access failed"}),
            );
            return None;
        };
        Some(Sample {
            queries,
            render,
            output,
            started,
            submitted: 0,
        })
    }

    pub(super) fn end(&mut self, renderer: &mut GlesRenderer, mut sample: Sample) -> bool {
        if renderer
            .with_context(|gl| {
                // SAFETY: The query names belong to this current context. Flush submits without waiting.
                unsafe {
                    gl.QueryCounterEXT(sample.queries[1], ffi::TIMESTAMP_EXT);
                    gl.Flush();
                }
            })
            .is_ok()
        {
            sample.submitted = frame_trace::timestamp();
            self.waiting.push_back(sample);
            true
        } else {
            self.supported = Some(false);
            frame_trace::record(
                json!({"event":"gles-timestamp-unavailable", "render":sample.render,
                "output":sample.output, "composition_started_us":sample.started,
                "reason":"could not restore renderer context"}),
            );
            false
        }
    }

    pub(super) fn cancel(&mut self, renderer: &mut GlesRenderer, sample: Sample) {
        let _ = renderer.with_context(|gl| {
            // SAFETY: The query names came from this renderer and will never be read again.
            unsafe { gl.DeleteQueriesEXT(2, sample.queries.as_ptr()) };
        });
        frame_trace::record(
            json!({"event":"gles-timestamp-unavailable", "render":sample.render,
            "output":sample.output, "composition_started_us":sample.started, "reason":"composition failed"}),
        );
    }

    fn collect(&mut self, renderer: &mut GlesRenderer) {
        let result = renderer.with_context(|gl| {
            let mut disjoint = 0;
            // SAFETY: This runs on the owning context and writes one integer to the supplied pointer.
            unsafe { gl.GetIntegerv(ffi::GPU_DISJOINT_EXT, &mut disjoint) };
            let mut ready = Vec::new();
            while let Some(sample) = self.waiting.front() {
                let duration = if disjoint != 0 {
                    None
                } else {
                    let mut available = [0; 2];
                    // SAFETY: Both query names are live in this context and the pointers refer to single entries.
                    unsafe {
                        for (query, available) in sample.queries.iter().zip(&mut available) {
                            gl.GetQueryObjectuivEXT(
                                *query,
                                ffi::QUERY_RESULT_AVAILABLE_EXT,
                                available,
                            );
                        }
                    }
                    if available.contains(&0) {
                        break;
                    }
                    let mut times = [0; 2];
                    // SAFETY: Availability was checked for both queries, so these reads do not wait.
                    unsafe {
                        for (query, time) in sample.queries.iter().zip(&mut times) {
                            gl.GetQueryObjectui64vEXT(*query, ffi::QUERY_RESULT_EXT, time);
                        }
                    }
                    times[1].checked_sub(times[0])
                };
                let sample = self.waiting.pop_front().expect("queued timer sample");
                // SAFETY: This is the final access to both names from their owning context.
                unsafe { gl.DeleteQueriesEXT(2, sample.queries.as_ptr()) };
                ready.push((sample, duration));
            }
            if disjoint == 0 {
                // SAFETY: Recheck the context clock after reading the available timestamps.
                unsafe { gl.GetIntegerv(ffi::GPU_DISJOINT_EXT, &mut disjoint) };
            }
            if disjoint != 0 {
                for (_, duration) in &mut ready {
                    *duration = None;
                }
                for sample in self.waiting.drain(..) {
                    // SAFETY: These outstanding query names will not be used again after the reset.
                    unsafe { gl.DeleteQueriesEXT(2, sample.queries.as_ptr()) };
                    ready.push((sample, None));
                }
            }
            ready
        });
        if let Ok(ready) = result {
            let observed = frame_trace::timestamp();
            for (sample, gpu_ns) in ready {
                frame_trace::record(
                    json!({"event":"gles-timestamp-readback", "render":sample.render,
                    "output":sample.output, "composition_started_us":sample.started,
                    "monotonic_us":observed, "readback_age_us":observed.saturating_sub(sample.submitted),
                    "command_interval_ns":gpu_ns}),
                );
            }
        } else {
            self.supported = Some(false);
            for sample in self.waiting.drain(..) {
                frame_trace::record(
                    json!({"event":"gles-timestamp-unavailable", "render":sample.render,
                    "output":sample.output, "composition_started_us":sample.started, "reason":"query context unavailable during readback"}),
                );
            }
        }
    }
}
