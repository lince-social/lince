// non-lean-yet

use lince_resources::publication::Publication;

use crate::ergon::fixture::{Records, all_records};

type Program = (Option<&'static str>, Option<&'static str>);

pub(super) fn assert_relocated(original: &Records<'_>, reopened: &Publication) {
    for &(slug, programs) in PROGRAMS {
        let restored = all_records(reopened)
            .find(|record| record.slug.as_deref() == Some(slug))
            .unwrap_or_else(|| panic!("relocated Record {slug} is missing"));
        for &(filename, expected) in programs {
            let named_marker;
            let marker = match filename {
                Some(filename) => {
                    named_marker = format!("```python file={filename}\n");
                    &named_marker
                }
                None => "```python\n",
            };
            let body = original[slug]
                .body
                .split(marker)
                .nth(1)
                .unwrap_or_else(|| panic!("Record {slug} is missing {marker:?}"));
            let code = body.split_once("\n```").map_or(body, |(code, _)| code);
            if let Some(expected) = expected {
                assert!(
                    code.contains(expected),
                    "{slug}: {filename:?} lacks {expected:?}"
                );
            }
            assert!(restored.body.contains(code), "{slug}: {filename:?} changed");
        }
    }
}

const PROGRAMS: &[(&str, &[Program])] = &[
    (
        "computing-foundation-study",
        &[(Some("stock_trace.py"), Some("def trace_stock("))],
    ),
    (
        "computing-collection-study",
        &[
            (Some("catalog_lookup.py"), Some("def lookup_prepared(")),
            (
                Some("catalog_checks.py"),
                Some("import catalog_lookup as m"),
            ),
        ],
    ),
    (
        "inquiry-design-study",
        &[
            (Some("allocation_study.py"), Some("def summarize(")),
            (
                Some("allocation_checks.py"),
                Some("import allocation_study as m"),
            ),
        ],
    ),
    (
        "math-change-study",
        &[
            (Some("change_study.py"), Some("def partition_sums(")),
            (Some("change_checks.py"), Some("import change_study as m")),
        ],
    ),
    (
        "physics-thermal-study",
        &[
            (Some("thermal_study.py"), Some("def moments(")),
            (Some("thermal_checks.py"), Some("import thermal_study as m")),
        ],
    ),
    (
        "chemistry-dynamics-study",
        &[
            (Some("reaction_study.py"), Some("def state(")),
            (Some("reaction_checks.py"), Some("def decay_bounds(")),
        ],
    ),
    (
        "computing-machine-study",
        &[
            (Some("machine_study.py"), Some("def step(")),
            (Some("machine_checks.py"), Some("expected_pcs")),
        ],
    ),
    (
        "engineering-network-study",
        &[
            (Some("network_study.py"), Some("def steady(")),
            (Some("network_checks.py"), Some("def decay_bounds(")),
        ],
    ),
    (
        "inquiry-uncertainty-study",
        &[
            (Some("uncertainty_study.py"), Some("def conductance(")),
            (Some("uncertainty_checks.py"), Some("def pair_cov(")),
        ],
    ),
    (
        "math-periodic-study",
        &[
            (Some("periodic_study.py"), Some("def sine_cosine(")),
            (Some("periodic_checks.py"), Some("def reference(")),
        ],
    ),
    (
        "physics-wave-study",
        &[
            (Some("wave_study.py"), Some("def energy_change(")),
            (Some("wave_checks.py"), Some("integrated_right")),
        ],
    ),
    (
        "chemistry-solution-study",
        &[
            (Some("solution_study.py"), Some("def solve(")),
            (Some("solution_checks.py"), Some("def hydroxide_reference(")),
        ],
    ),
    (
        "computing-memory-study",
        &[
            (Some("space_study.py"), Some("def translate(")),
            (Some("space_checks.py"), Some("def reference(")),
        ],
    ),
    (
        "engineering-material-study",
        &[
            (Some("axial_study.py"), Some("def parallel(")),
            (Some("axial_checks.py"), Some("def linear_reference(")),
        ],
    ),
    (
        "inquiry-density-study",
        &[
            (Some("density_study.py"), Some("def mean_cdf(")),
            (Some("density_checks.py"), Some("def square_area_cdf(")),
        ],
    ),
    (
        "math-linear-system-study",
        &[
            (
                Some("linear_space_study.py"),
                Some("def orthogonal_columns("),
            ),
            (Some("linear_space_checks.py"), Some("def minor_rank(")),
        ],
    ),
    (
        "physics-electric-study",
        &[
            (Some("electric_study.py"), Some("def sharing(")),
            (Some("electric_checks.py"), Some("def midpoint_work(")),
        ],
    ),
    (
        "chemistry-electron-study",
        &[
            (
                Some("redox_study.py"),
                Some("from linear_space_study import eliminate"),
            ),
            (Some("redox_checks.py"), Some("def atom_charge_sum(")),
        ],
    ),
    (
        "computing-durable-study",
        &[
            (Some("durable_study.py"), Some("def explore(")),
            (Some("durable_checks.py"), Some("def subset_images(")),
        ],
    ),
    (
        "engineering-electric-study",
        &[
            (
                Some("realization_study.py"),
                Some("from linear_space_study import eliminate"),
            ),
            (Some("realization_checks.py"), Some("def reduced(")),
        ],
    ),
    (
        "inquiry-fitting-study",
        &[
            (
                Some("fitting_study.py"),
                Some("from linear_space_study import eliminate"),
            ),
            (Some("fitting_checks.py"), Some("def centered(")),
        ],
    ),
    (
        "math-multivariable-study",
        &[
            (
                Some("multivariable_study.py"),
                Some("from linear_space_study import eliminate"),
            ),
            (Some("multivariable_checks.py"), Some("def compose(")),
        ],
    ),
    (
        "physics-entropy-study",
        &[
            (
                Some("entropy_study.py"),
                Some("from reaction_study import decimal"),
            ),
            (Some("entropy_checks.py"), Some("def logarithm_bounds(")),
        ],
    ),
    (
        "chemistry-free-energy-study",
        &[
            (
                Some("free_energy_study.py"),
                Some("from reaction_study import decimal"),
            ),
            (
                Some("free_energy_checks.py"),
                Some("from entropy_checks import logarithm_bounds"),
            ),
        ],
    ),
    (
        "computing-concurrency-study",
        &[
            (Some("concurrency_study.py"), Some("def wait_successors(")),
            (
                Some("concurrency_checks.py"),
                Some("def serial_prefix_oracle("),
            ),
        ],
    ),
    (
        "engineering-feedback-study",
        &[
            (
                Some("feedback_study.py"),
                Some("from network_study import response, number"),
            ),
            (Some("feedback_checks.py"), Some("def root_oracle(")),
        ],
    ),
    (
        "inquiry-reporting-study",
        &[
            (
                Some("reporting_study.py"),
                Some("from fitting_study import fit, partition"),
            ),
            (Some("reporting_checks.py"), Some("def oracle(")),
        ],
    ),
    (
        "math-discrete-study",
        &[
            (Some("discrete_study.py"), Some("def quotient(")),
            (Some("discrete_checks.py"), Some("def closure_oracle(")),
        ],
    ),
    (
        "physics-rotation-study",
        &[
            (Some("rotation_study.py"), Some("def rolling(")),
            (Some("rotation_checks.py"), Some("expected = eliminate(")),
        ],
    ),
    (
        "chemistry-identity-study",
        &[
            (Some("identity_study.py"), Some("def tetrahedral_classes(")),
            (Some("identity_checks.py"), Some("def prufer_tree(")),
        ],
    ),
    (
        "computing-communication-study",
        &[
            (Some("communication_study.py"), Some("class Decoder:")),
            (Some("communication_checks.py"), Some("def abstract_graph(")),
        ],
    ),
    (
        "engineering-production-study",
        &[
            (Some("production_study.py"), Some("def window_area(")),
            (Some("production_checks.py"), Some("def assignments(")),
        ],
    ),
    (
        "math-series-study",
        &[
            (Some("series_study.py"), Some("def select_cutoff(")),
            (Some("series_checks.py"), Some("pairs=[(i,j,r**(i+j))")),
        ],
    ),
    (
        "physics-statistical-study",
        &[
            (
                Some("statistical_study.py"),
                Some("def time_mean_variance("),
            ),
            (
                Some("statistical_checks.py"),
                Some("for path in product((0, 1), repeat=length):"),
            ),
        ],
    ),
    (
        "chemistry-phase-study",
        &[
            (Some("phase_study.py"), Some("def certify_binodal(")),
            (Some("phase_checks.py"), Some("alternatives=[(i,total-i)")),
        ],
    ),
    (
        "mat-regular-example",
        &[(None, Some("def symmetric_regular_solution("))],
    ),
    (
        "computing-abstraction-study",
        &[
            (Some("abstraction_study.py"), Some("def reserve_many(")),
            (
                Some("abstraction_checks.py"),
                Some("for history in product(alphabet, repeat=length):"),
            ),
        ],
    ),
    (
        "engineering-service-study",
        &[
            (Some("service_study.py"), Some("def evaluate(")),
            (
                Some("service_checks.py"),
                Some("for history in product(events("),
            ),
        ],
    ),
    (
        "inquiry-causal-study",
        &[
            (Some("causal_study.py"), Some("def horvitz_thompson(")),
            (
                Some("causal_checks.py"),
                Some("observed(null_model) == observed(effect_model)"),
            ),
        ],
    ),
    (
        "math-operator-study",
        &[
            (Some("operator_study.py"), Some("def exponential_partial(")),
            (
                Some("operator_checks.py"),
                Some("distance+uncertainty <= tail"),
            ),
        ],
    ),
    (
        "physics-quantum-study",
        &[
            (Some("quantum_study.py"), Some("def dephase(")),
            (Some("quantum_checks.py"), Some("q.uncertainty([1, 0, 0]")),
        ],
    ),
    (
        "computing-trust-study",
        &[
            (
                Some("trust_study.py"),
                Some("from communication_study import natural, receive"),
            ),
            (Some("trust_checks.py"), Some("expected_histories")),
        ],
    ),
    (
        "engineering-operation-study",
        &[
            (
                Some("operation_study.py"),
                Some("from production_study import integer"),
            ),
            (Some("operation_checks.py"), Some("independent_minimal")),
        ],
    ),
    (
        "math-fields-study",
        &[
            (
                Some("field_study.py"),
                Some("from multivariable_study import segment_integral, value"),
            ),
            (
                Some("field_checks.py"),
                Some("result['boundary'], expected"),
            ),
        ],
    ),
    (
        "physics-magnetic-study",
        &[
            (
                Some("magnetic_study.py"),
                Some("from electric_study import exact, decay_bracket"),
            ),
            (Some("magnetic_checks.py"), Some("current_two_tau_bounds")),
        ],
    ),
    (
        "physics-spacetime-study",
        &[
            (
                Some("spacetime_study.py"),
                Some("from electric_study import exact, rational_root, potential_field"),
            ),
            (
                Some("spacetime_checks.py"),
                Some("reported_minus_prediction_over_reported_uncertainty"),
            ),
        ],
    ),
    (
        "physics-astronomy-study",
        &[
            (
                Some("astronomy_study.py"),
                Some("from spacetime_study import orbit"),
            ),
            (
                Some("astronomy_checks.py"),
                Some("historical_absolute_magnitude"),
            ),
        ],
    ),
    (
        "math-structures-study",
        &[
            (Some("structures_study.py"), None),
            (Some("structures_checks.py"), None),
        ],
    ),
];
