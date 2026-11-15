# Linux CI: elapsed time and compute minutes

The owner currently prefers the shortest release wait. `build.yml` builds the
application and Park measurement probe concurrently on **separate runners** using
the `binary` matrix. Each runner has its own compatible artifact cache. The probe
first tries its own cache, then the old `linux-release` scope to avoid a cold start
while its cache warms up. Only the probe restores from the other scope; the two
jobs never compete to save the same cache key.

This choice spends additional compute to reduce elapsed time. It does not claim
to reduce billed runner minutes or cost. Two concurrent 50-minute jobs take about
50 minutes to finish but consume about 100 runner minutes, before setup and
packaging. Check actual GitHub plan/allowance/billing rather than assuming the
repository's public/private status or free-minute balance.

## Why the probe exists

`park-responsiveness` is an optimized executable from package `lince`, using the
same `instinct,park` features, default UI, target, compiler, flags and ThinLTO
configuration as the shipping application. It reuses
`crates/park/examples/responsiveness.rs`; it is not a second implementation of
Park. It supports isolated native responsiveness suites and prepared private
saved-workspace/Record-edit/projection measurements. Building the probe does not
run those measurements or establish physical desktop acceptance.

The compilation jobs upload their outputs independently. `park-measurement`
checks the application and probe revision/version/optimization/target identities
against the workflow's commit before producing the existing
`park-measurement-artifacts-x86_64-unknown-linux-gnu` archive, including both build
identities and checksums. All release publication jobs depend on that check and
the tooling checks. Shipping patterns still select only
`lince-binary-artifacts-*`. Intermediate probe and identity artifacts expire after
one day; final artifacts and separate timing reports last seven days.

## If asked to spend fewer GitHub minutes

**Prefer a shared build on one runner instead of adding still more parallel
jobs.** This is an intentional alternative when the free allowance or compute
budget matters more than the fastest delivery. Do not restore the old pair of
sequential Cargo commands unchanged: the measured run rebuilt large dependencies
for the example after building the application.

Replace the compilation matrix with one job and select both targets in a single
Cargo invocation, preserving the current release environment:

```sh
cargo build --locked --release --package lince --features instinct,park \
  --bin lince --example park-responsiveness --target "$BUILD_TARGET" --timings
"target/$BUILD_TARGET/release/lince" --build-info > target/release-build-info.json
```

Keep `LINCE_REVISION`, `RUST_MIN_STACK=33554432`, warnings as errors, the pinned
toolchain, current linker and `CARGO_PROFILE_RELEASE_LTO=thin`. Run the existing
application identity check and packaging, capture the probe's `--build-info`, and
use `institute/packaging/linux/package-park-probe.sh` to keep the identity and
checksum guarantees. The intermediate artifact transfers and separate packaging
runner can then be removed. Keep publication gated on successful packaging and
tooling. Use one cache scope for the combined build; allow compatible fallback
restores from today's scopes when migrating.

A single Cargo graph lets compatible dependencies be shared and schedules both
final executables as their prerequisites become available. It may avoid the
observed duplicate dependency work; measure that rather than promising it. The
two expensive final optimization/link invocations still exist and compete for
one runner's CPU and memory. Simply increasing `-j` on the same small runner is
not equivalent to adding compute and may worsen memory pressure. Running two
independent Cargo processes against one target directory also introduces lock
contention; separate directories avoid that but duplicate compiled inputs.

## Baseline and how to judge a change

The owner's screenshot corresponds to Actions run **43713878374**, revision
`389fcf9a835ee7eb512ec00d8eeb04bf140eebc7` (2026-11-15). Its serialized steps
were 51m36s for the application and 42m36s for the probe: **94m12s elapsed** for
these steps alone. The downloaded Cargo timing reports show four CPU/job slots.
For the probe, 43 dependency units took the first 15m51s of the critical path;
its final compiler invocation took 26m34s. This is not proof that all of that last
interval is linking. 1,881 units were already fresh, so this was not a fully cold
build.

After changing the workflow, compare both **elapsed time to verified artifacts**
and the **sum of runner/job minutes**, separating warm and cold cache runs.
Inspect the uploaded per-matrix Cargo timing reports for repeated dependencies,
idle CPU slots and final compiler costs. No speedup is measured merely by changing
the workflow. Runner concurrency limits, cache downloads, memory and setup costs
affect the result. Keep benchmark compilation separate from timed Park runs.
Do not lower optimization, remove the probe, skip validation, change the measured
feature graph, or publish mismatched artifacts just to improve CI timing.
