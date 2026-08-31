set -eu

artifact_dir=target/interface-laboratory/accessibility
report="$artifact_dir/report.json"
tree="$artifact_dir/atspi-tree.txt"
runtime_log="$artifact_dir/runtime.log"
orca_log="$artifact_dir/orca.log"
mkdir -p "$artifact_dir"

launcher_pid=
runner_pid=
orca_pid=

cleanup() {
  if [ -n "$orca_pid" ]; then
    kill -TERM "$orca_pid" 2>/dev/null || true
  fi
  if [ -n "$runner_pid" ]; then
    kill -INT "$runner_pid" 2>/dev/null || true
  fi
  if [ -n "$launcher_pid" ]; then
    kill -TERM "$launcher_pid" 2>/dev/null || true
  fi
}

trap cleanup EXIT INT TERM

if ! busctl --user call org.a11y.Bus /org/a11y/bus org.a11y.Bus GetAddress >/dev/null 2>&1; then
  "$LINCE_AT_SPI_BUS_LAUNCHER" --launch-immediately --a11y=1 --screen-reader=1 &
  launcher_pid=$!
  for attempt in $(seq 1 100); do
    if busctl --user call org.a11y.Bus /org/a11y/bus org.a11y.Bus GetAddress >/dev/null 2>&1; then
      break
    fi
    sleep 0.05
  done
fi

address=$(busctl --user call org.a11y.Bus /org/a11y/bus org.a11y.Bus GetAddress | sed -n 's/^s "\(.*\)"$/\1/p')
test -n "$address"

cargo run --release --manifest-path crates/interface/Cargo.toml --features joined-runtime --bin lince-interface-cef-diagnostic -- --joined-report-and-exit --accessibility-probe --warmup-seconds 1 --sample-seconds 8 --report "$report" >"$runtime_log" 2>&1 &
runner_pid=$!

service=
for attempt in $(seq 1 2400); do
  for candidate in $(busctl --address="$address" list --no-legend | awk '$1 ~ /^:/ { print $1 }'); do
    name=$(busctl --address="$address" get-property "$candidate" /org/a11y/atspi/accessible/root org.a11y.atspi.Accessible Name 2>/dev/null || true)
    if printf '%s' "$name" | grep -q 'lince-interface-cef-diagnostic'; then
      service=$candidate
      break
    fi
  done
  if [ -n "$service" ]; then
    break
  fi
  sleep 0.05
done
test -n "$service"

root=/org/a11y/atspi/accessible/0/0
children=$(busctl --address="$address" call "$service" "$root" org.a11y.atspi.Accessible GetChildren)
button=
: >"$tree"
printf 'service %s\n' "$service" | tee -a "$tree"
busctl --address="$address" get-property "$service" "$root" org.a11y.atspi.Accessible Name | tee -a "$tree"
busctl --address="$address" call "$service" "$root" org.a11y.atspi.Accessible GetRole | tee -a "$tree"
printf '%s\n' "$children" | tee -a "$tree"
for path in $(printf '%s' "$children" | grep -o '"/org/a11y/atspi/accessible/[^\"]*"' | tr -d '"'); do
  name=$(busctl --address="$address" get-property "$service" "$path" org.a11y.atspi.Accessible Name 2>/dev/null || true)
  role=$(busctl --address="$address" call "$service" "$path" org.a11y.atspi.Accessible GetRole 2>/dev/null || true)
  printf '%s | %s | %s\n' "$path" "$role" "$name" | tee -a "$tree"
  if printf '%s' "$name" | grep -q 'Standalone Record button Sand'; then
    button=$path
  fi
done
test -n "$button"
busctl --address="$address" call "$service" "$button" org.a11y.atspi.Action GetActions | tee -a "$tree"
busctl --address="$address" call "$service" "$button" org.a11y.atspi.Action DoAction i 0 | tee -a "$tree"

orca --replace --debug-file "$orca_log" >"$artifact_dir/orca-runtime.log" 2>&1 &
orca_pid=$!
sleep 2
orca --list-apps | tee -a "$tree"

wait "$runner_pid"
runner_pid=
kill -TERM "$orca_pid" 2>/dev/null || true
orca_pid=

jq -e '.status == "passed" and .joined.accessibility_initial_tree_requests > 0 and .joined.accessibility_actions > 0' "$report" >/dev/null
printf 'Accessibility report: %s\n' "$report"
