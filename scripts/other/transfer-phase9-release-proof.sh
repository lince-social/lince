#!/usr/bin/env bash
# Phase 9 Transfer release proof. This is the one entry point for the focused
# automated gates and the explicit two-Cell manual scenario ledger.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
MODE="${1:---all}"
# The Action enum now carries the complete Transfer workflow. Some older async
# integration tests retain it across awaits and exceed Rust's 2 MiB default
# test-thread stack in debug builds.
export RUST_MIN_STACK="${RUST_MIN_STACK:-16777216}"

phase() {
  printf '\n== %s ==\n' "$1"
}

run_automated() {
  cd "$ROOT"

  phase "Phase 0-1: authority, revision, draft and projection contracts"
  nix develop -c cargo check --workspace --all-targets
  nix develop -c cargo test -p engine --test auth_actions
  nix develop -c cargo test -p protein --test sources transfer_source_derives_the_occurrence_status_ladder

  phase "Phase 2-3: negotiation, messages and signed agreement foundations"
  nix develop -c cargo test -p engine --test record_edits record_threads_and_messages_are_records_plus_links
  nix develop -c cargo test -p protein --test features threads_include_returns_nested_record_messages
  nix develop -c cargo test -p engine --test transfer revising_a_signed_promise_invalidates_current_agreement
  nix develop -c cargo test -p engine --test transfer every_fact_is_signed_and_verifiable
  nix develop -c cargo test -p engine --test transfer_agreement -- --test-threads=1

  phase "Phase 4-5: occurrence, confirmation, settlement and correction foundations"
  nix develop -c cargo test -p engine --test confirmations
  nix develop -c cargo test -p engine --test expiry
  nix develop -c cargo test -p engine --test transfer a_sale_applies_only_reviewed_occurrences_owned_by_the_signer

  phase "Phase 6-7: hierarchy, bulk work and detailed inspection surfaces"
  nix develop -c cargo test -p protein --test features
  nix develop -c cargo test -p engine --test cycle_warning

  phase "Phase 8: signed two-Cell transport, retry and import boundaries"
  nix develop -c cargo test -p engine --test organ_sync
  nix develop -c cargo test -p engine --test sync

  phase "Phase 9: Transfer browser and explicit Record references"
  scripts/other/transfer-sand-selftest.sh
  guard_external_integrations

  printf '\nPASS: automated Transfer Phase 9 release proof\n'
}

guard_external_integrations() {
  local -a paths=(
    crates/engine/src/actions.rs
    crates/engine/src/transfer.rs
    crates/engine/src/transfer_delivery.rs
    crates/nucleus/src/transfer.rs
    crates/nucleus/src/transfer_delivery.rs
    crates/protein/src/lib.rs
    crates/store/src/transfers.rs
    crates/store/src/transfer_delivery.rs
    crates/web/src/presentation/http/transfer_delivery.rs
    crates/web/src/sand/transfer
  )
  local pattern='stripe|paypal|adyen|braintree|google[_ -]?calendar|microsoft[_ -]?graph|openstreetmap|mapbox|uber|lyft|fedex|dhl|ups[_ -]?api|twilio|docusign|hellosign|fiote[_ -]?transfer'
  if rg --line-number --ignore-case "$pattern" "${paths[@]}"; then
    printf 'FAIL: Transfer contains an external payment/carrier/calendar/routing/Fiote integration surface\n' >&2
    return 1
  fi
  printf 'PASS: no external payment, carrier, calendar-provider, routing, call, legal, or Fiote integration surface\n'
}

print_manual_matrix() {
  cat <<'EOF'
TWO-CELL MANUAL ACCEPTANCE MATRIX

Use two clean Cells with distinct Organ and Person signers. For every row,
repeat the final network submission and confirm the second submission produces
the same durable identity and no second quantity, agreement, claim, settlement,
message, receipt, or application effect.

[ ] donation
    One giver promise and one receiver path; both Cells converge on canonical
    quantity/unit/person/window/place, while only the receiving Cell sees its
    local Record and application formula.

[ ] sale
    Opposite resource and consideration promises remain separate canonical
    occurrences. No payment execution object or provider request is created.

[ ] assignment and group coordination
    A Transfer tree with at least three People shows per-Person agreement and
    claim blockers. Bulk work authors separate evidence only for its signer.

[ ] information and service
    Time-bounded service/information promises settle normally. A message can
    disclose an explicitly selected document/receipt Record, and an unrelated
    Record is absent from the recipient projection.

[ ] dependency
    A downstream promise stays blocked until its named upstream state is met;
    a cycle is rejected and does not partially revise either Transfer.

[ ] satiation
    First-completes records one winner and visible loser evidence. Retrying the
    winning settlement neither chooses another winner nor adds another effect.

[ ] correction
    A dispute preserves prior evidence; a reversing Transfer or compensation
    adds lineage without rewriting the original occurrence or Fact.

[ ] partial work
    A partial slice advances cumulative progress once and leaves a visible
    remainder. A remainder draft is local and unsigned until deliberately sent.

For every row also inspect: signed actor identity, authoritative revision,
outbox retry state, package receipt versus fulfillment claim, hosted/replicated
freshness, attachment disclosure, and absence of private formula/Record data.
EOF
}

validate_manual_results() {
  local results_file="$1"
  [ -f "$results_file" ] || {
    printf 'FAIL: manual result file does not exist: %s\n' "$results_file" >&2
    return 1
  }
  local -a scenarios=(
    donation
    sale
    assignment_group_coordination
    information_service
    dependency
    satiation
    correction
    partial_work
  )
  local scenario line status evidence
  for scenario in "${scenarios[@]}"; do
    line="$(awk -F '|' -v scenario="$scenario" '$1 == scenario { print; exit }' "$results_file")"
    IFS='|' read -r _ status evidence <<<"$line"
    if [ "$status" != "pass" ] || [ -z "${evidence//[[:space:]]/}" ]; then
      printf 'FAIL: manual scenario %s needs `%s|pass|<evidence note>`\n' "$scenario" "$scenario" >&2
      return 1
    fi
  done
  printf 'PASS: all two-Cell manual scenarios carry explicit evidence notes\n'
}

case "$MODE" in
  --all)
    run_automated
    if [ -z "${TRANSFER_PHASE9_MANUAL_RESULTS:-}" ]; then
      print_manual_matrix
      printf '\nFAIL: set TRANSFER_PHASE9_MANUAL_RESULTS to the completed result ledger\n' >&2
      exit 1
    fi
    validate_manual_results "$TRANSFER_PHASE9_MANUAL_RESULTS"
    ;;
  --automated)
    run_automated
    ;;
  --guard)
    cd "$ROOT"
    guard_external_integrations
    ;;
  --manual-matrix)
    print_manual_matrix
    ;;
  --verify-manual)
    [ "$#" -eq 2 ] || {
      printf 'usage: %s --verify-manual <results-file>\n' "$0" >&2
      exit 2
    }
    validate_manual_results "$2"
    ;;
  *)
    printf 'usage: %s [--all|--automated|--guard|--manual-matrix|--verify-manual <results-file>]\n' "$0" >&2
    exit 2
    ;;
esac
