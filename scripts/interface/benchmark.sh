set -eu

artifact_dir=target/interface-laboratory/benchmark
mkdir -p "$artifact_dir"

for repeat in 1 2 3; do
  cargo run --release --manifest-path crates/interface/Cargo.toml --features joined-runtime --bin lince-interface-cef-diagnostic -- --joined-report-and-exit --warmup-seconds 30 --sample-seconds 120 --cef-count 2 --report "$artifact_dir/joined-$repeat.json"
done

jq -s '
  {
    schema_version: 1,
    gate: "three-repeat joined native Interface benchmark",
    status: if all(.[];
      .status == "passed" and
      .host.window_system == "wayland" and
      .joined.frame_p95_millis <= 16.67 and
      .joined.frame_p99_millis <= 25.0 and
      .joined.frame_hitches_over_50_millis <= 1 and
      .joined.input_to_present_call_p95_millis <= 33.4 and
      .joined.input_to_present_call_p99_millis <= 50.0 and
      .joined.fixed_step_p95_millis <= 8.0 and
      .joined.fixed_step_backlog_steps == 0
    ) then "passed" else "failed" end,
    repeats: map({
      git_revision,
      git_dirty,
      source_fingerprint_sha256,
      profile,
      host,
      workload,
      frames_presented,
      frame_samples: .joined.frame_samples,
      frame_p50_millis: .joined.frame_p50_millis,
      frame_p95_millis: .joined.frame_p95_millis,
      frame_p99_millis: .joined.frame_p99_millis,
      hitches_over_50_millis: .joined.frame_hitches_over_50_millis,
      cpu_frame_p95_millis: .joined.cpu_frame_p95_millis,
      input_to_present_call_p95_millis: .joined.input_to_present_call_p95_millis,
      input_to_present_call_p99_millis: .joined.input_to_present_call_p99_millis,
      fixed_step_p95_millis: .joined.fixed_step_p95_millis,
      first_cef_frame_millis: .host.process_to_first_cef_frame_millis,
      off_camera_paint_suppression_effective: .joined.off_camera_paint_suppression_effective
    }),
    raw_reports: ["joined-1.json", "joined-2.json", "joined-3.json"],
    presentation_limit: "WGPU and the active Wayland compositor expose no trustworthy physical-display timestamp through this host path; input-to-present-call is a measured lower bound, not camera-measured visible latency."
  }
' "$artifact_dir"/joined-1.json "$artifact_dir"/joined-2.json "$artifact_dir"/joined-3.json >"$artifact_dir/summary.json"

jq -e '.status == "passed"' "$artifact_dir/summary.json" >/dev/null
printf 'Benchmark summary: %s\n' "$artifact_dir/summary.json"
