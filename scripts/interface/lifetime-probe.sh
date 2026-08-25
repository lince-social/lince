set -eu

artifact_dir=target/interface-laboratory/lifetime
binary_path=$(realpath target/release/lince-interface-cef-diagnostic 2>/dev/null || true)
runner_pid=

cleanup() {
  if [ -n "$runner_pid" ]; then
    kill -INT "$runner_pid" 2>/dev/null || true
  fi
}

trap cleanup EXIT INT TERM
mkdir -p "$artifact_dir/cycles"

matching_pids() {
  for process_dir in /proc/[0-9]*; do
    executable=$(readlink "$process_dir/exe" 2>/dev/null || true)
    if [ -n "$binary_path" ] && [ "$executable" = "$binary_path" ]; then
      basename "$process_dir"
    fi
  done
}

sample_resources() {
  total_rss=0
  total_gpu=0
  gpu_observed=0
  process_count=0
  for process_id in $(matching_pids); do
    rss=$(awk '/^VmRSS:/ { print $2 }' "/proc/$process_id/status" 2>/dev/null || true)
    total_rss=$((total_rss + ${rss:-0}))
    process_count=$((process_count + 1))
    for fdinfo in /proc/"$process_id"/fdinfo/*; do
      if [ -r "$fdinfo" ]; then
        value=$(awk '/^drm-memory-(local|shared|system|gtt):/ { sum += $2; found = 1 } END { if (found) print sum }' "$fdinfo" 2>/dev/null || true)
        if [ -n "$value" ]; then
          total_gpu=$((total_gpu + value))
          gpu_observed=1
        fi
      fi
    done
  done
  printf '%s %s %s %s\n' "$total_rss" "$total_gpu" "$gpu_observed" "$process_count"
}

rss_values=
gpu_values=
startup_values=
process_values=
all_cycles_passed=true
all_processes_closed=true

for cycle in $(seq 1 10); do
  cycle_report="$artifact_dir/cycles/$cycle.json"
  cycle_log="$artifact_dir/cycles/$cycle.log"
  cargo run --release --manifest-path crates/interface-prototype/Cargo.toml --features joined-runtime --bin lince-interface-cef-diagnostic -- --joined-report-and-exit --warmup-seconds 0 --sample-seconds 5 --cef-count 2 --report "$cycle_report" >"$cycle_log" 2>&1 &
  runner_pid=$!
  peak_rss=0
  peak_gpu=0
  saw_gpu=0
  peak_processes=0
  while kill -0 "$runner_pid" 2>/dev/null; do
    set -- $(sample_resources)
    if [ "$1" -gt "$peak_rss" ]; then peak_rss=$1; fi
    if [ "$2" -gt "$peak_gpu" ]; then peak_gpu=$2; fi
    if [ "$3" -gt 0 ]; then saw_gpu=1; fi
    if [ "$4" -gt "$peak_processes" ]; then peak_processes=$4; fi
    sleep 0.05
  done
  wait "$runner_pid"
  runner_pid=
  if ! jq -e '.status == "passed" and ([.surfaces[].closed] | all)' "$cycle_report" >/dev/null; then
    all_cycles_passed=false
  fi
  if [ -n "$(matching_pids)" ]; then
    all_processes_closed=false
  fi
  startup=$(jq -r '.host.process_to_interactive_window_millis' "$cycle_report")
  rss_values="${rss_values}${rss_values:+,}$peak_rss"
  if [ "$saw_gpu" -gt 0 ]; then
    gpu_values="${gpu_values}${gpu_values:+,}$peak_gpu"
  else
    gpu_values="${gpu_values}${gpu_values:+,}null"
  fi
  startup_values="${startup_values}${startup_values:+,}$startup"
  process_values="${process_values}${process_values:+,}$peak_processes"
  printf 'cycle %s: RSS %s KiB, GPU %s KiB, processes %s, startup %s ms\n' "$cycle" "$peak_rss" "$peak_gpu" "$peak_processes" "$startup"
done

jq -n \
  --argjson rss "[$rss_values]" \
  --argjson gpu "[$gpu_values]" \
  --argjson startup "[$startup_values]" \
  --argjson processes "[$process_values]" \
  --argjson cycles_passed "$all_cycles_passed" \
  --argjson processes_closed "$all_processes_closed" \
  '
    def monotonic: . as $values | all(range(1; length); $values[.] >= $values[. - 1]);
    def growth: if .[0] > 0 then (.[-1] - .[0]) / .[0] else 0 end;
    ($rss[1:] | (monotonic and growth > 0.05) | not) as $rss_passed |
    ($gpu | map(select(. != null))) as $known_gpu |
    ($known_gpu | if length >= 2 then (monotonic and ((.[-1] - .[0]) / (.[0] | if . > 0 then . else 1 end)) > 0.05) | not else true end) as $gpu_passed |
    ($startup[1:] | all(. <= 2000.0)) as $warm_startup_passed |
    ($startup[0] <= 4000.0) as $cold_startup_passed |
    {
      schema_version: 1,
      gate: "joined runtime lifetime",
      status: if $cycles_passed and $processes_closed and $rss_passed and $gpu_passed and $warm_startup_passed and $cold_startup_passed then "passed" else "failed" end,
      cycles: 10,
      peak_family_rss_kib: $rss,
      peak_family_drm_memory_kib: $gpu,
      peak_process_counts: $processes,
      process_to_interactive_window_millis: $startup,
      assertions: [
        {name: "all joined cycles passed", passed: $cycles_passed},
        {name: "all CEF process families closed after every cycle", passed: $processes_closed},
        {name: "RSS has no unexplained monotonic growth above five percent after warm cycle", passed: $rss_passed},
        {name: "available DRM memory counters have no monotonic growth above five percent", passed: $gpu_passed},
        {name: "warm cycles reach an interactive native window within two seconds", passed: $warm_startup_passed},
        {name: "cold cycle reaches an interactive native window within four seconds", passed: $cold_startup_passed}
      ],
      drm_memory_note: if ($gpu | all(. == null)) then "No per-process drm-memory counters were exposed by the Iris Xe driver through /proc fdinfo." else "Sum of drm-memory counters exposed through every matching process fdinfo." end
    }
  ' >"$artifact_dir/report.json"

jq -e '.status == "passed" and ([.assertions[].passed] | all)' "$artifact_dir/report.json" >/dev/null
printf 'Lifetime report: %s\n' "$artifact_dir/report.json"
