#![cfg(feature = "publication")]

use lince_resources::{Library, publication::Publication};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::PathBuf,
};

#[test]
fn hephaestus_discovers_six_areas_and_relocates_the_shared_core_without_specializations() {
    let directory =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../institute/trails/ergon/hephaestus");
    let temporary = tempfile::tempdir().unwrap();
    let library = Library::new(temporary.path().join("library"));
    let publication = Publication::load(
        &library,
        directory.join("Hephaestus.lingua").to_str().unwrap(),
        &[],
        false,
    )
    .unwrap();
    let records: BTreeMap<_, _> = publication
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .map(|record| (record.slug.as_deref().unwrap(), record))
        .collect();
    let area_slugs = [
        "mathematics-map",
        "physics-map",
        "chemistry-map",
        "computing-map",
        "engineering-map",
        "inquiry-map",
    ];
    let core_slugs = [
        "math-core",
        "physics-core",
        "chemistry-core",
        "computing-core",
        "engineering-core",
        "inquiry-core",
    ];
    let root = records["hephaestus"].uid.clone();
    let expected_map: BTreeSet<_> = area_slugs
        .iter()
        .map(|slug| (root.clone(), records[slug].uid.clone()))
        .collect();
    assert_eq!(learning_edges(&publication, "hephaestus"), expected_map);
    let mut expected_core = expected_map.clone();
    expected_core.extend(
        area_slugs
            .iter()
            .zip(core_slugs)
            .map(|(area, core)| (records[area].uid.clone(), records[core].uid.clone())),
    );
    let mathematics = [
        "math-core",
        "ql-quantity",
        "ql-arithmetic",
        "math-domains",
        "ql-fractions",
        "ql-ratios",
        "math-proportional-models",
        "ql-equations",
        "math-equivalent-equations",
        "ql-functions",
        "math-function-structure",
        "math-quantified-claims",
        "math-first-proofs",
        "math-foundation-study",
        "math-foundation-review",
    ];
    expected_core.extend(
        mathematics
            .windows(2)
            .map(|pair| (records[pair[0]].uid.clone(), records[pair[1]].uid.clone())),
    );
    let physics = [
        "physics-core",
        "physics-system-question",
        "ql-units",
        "physics-location-clock",
        "ql-graphs",
        "physics-average-local",
        "ql-geometry",
        "physics-constant-acceleration",
        "ql-trigonometry",
        "ql-vectors",
        "mech-force",
        "physics-momentum-balance",
        "physics-work-energy",
        "physics-model-evidence",
        "physics-foundation-study",
        "physics-foundation-review",
    ];
    expected_core.extend(
        physics
            .windows(2)
            .map(|pair| (records[pair[0]].uid.clone(), records[pair[1]].uid.clone())),
    );
    let chemistry = [
        "chemistry-core",
        "gc-question",
        "gc-matter",
        "gc-models",
        "gc-names",
        "gc-atomic-evidence",
        "gc-isotopes",
        "chemistry-quantity-bridge",
        "gc-amount",
        "gc-precision",
        "gc-equations",
        "ql-balances",
        "gc-extent",
        "gc-limiting",
        "chemistry-foundation-study",
        "chemistry-foundation-review",
    ];
    expected_core.extend(
        chemistry
            .windows(2)
            .map(|pair| (records[pair[0]].uid.clone(), records[pair[1]].uid.clone())),
    );
    let computing = [
        "computing-core",
        "computing-representation",
        "rc-files",
        "rc-values",
        "rc-functions",
        "rc-io",
        "rc-schema",
        "computing-contract",
        "computing-loop-argument",
        "computing-cost",
        "computing-execution",
        "computing-foundation-study",
        "computing-foundation-review",
    ];
    expected_core.extend(
        computing
            .windows(2)
            .map(|pair| (records[pair[0]].uid.clone(), records[pair[1]].uid.clone())),
    );
    let engineering_core = [
        "engineering-core",
        "mech-purpose",
        "engineering-requirements",
        "mech-parts",
        "engineering-interfaces",
        "engineering-model-choices",
        "mech-model",
        "engineering-alternatives",
        "engineering-evidence",
        "engineering-operation",
        "engineering-foundation-study",
        "engineering-foundation-review",
    ];
    expected_core.extend(
        engineering_core
            .windows(2)
            .map(|pair| (records[pair[0]].uid.clone(), records[pair[1]].uid.clone())),
    );
    let inquiry_core = [
        "inquiry-core",
        "inquiry-claims",
        "met-measurand",
        "met-principle",
        "inquiry-variation",
        "inquiry-comparisons",
        "met-sampling-target",
        "rc-claim",
        "inquiry-reproduction",
        "met-measurement-records",
        "inquiry-communication",
        "inquiry-foundation-study",
        "inquiry-foundation-review",
    ];
    expected_core.extend(
        inquiry_core
            .windows(2)
            .map(|pair| (records[pair[0]].uid.clone(), records[pair[1]].uid.clone())),
    );
    let math_space = [
        "math-foundation-review",
        "math-geometric-reasoning",
        "math-coordinate-frames",
        "math-linear-combinations",
        "math-projections",
        "math-linear-systems",
        "math-linear-maps",
        "math-linear-study",
        "math-linear-review",
    ];
    expected_core.extend(
        math_space
            .windows(2)
            .map(|pair| (records[pair[0]].uid.clone(), records[pair[1]].uid.clone())),
    );
    let physics_space = [
        "physics-foundation-review",
        "physics-planar-motion",
        "mech-free-body",
        "physics-planar-moment",
        "mech-moments",
        "physics-load-resultants",
        "mech-equilibrium",
        "mech-constraints",
        "physics-constraint-evidence",
        "physics-spatial-study",
        "physics-spatial-review",
    ];
    expected_core.extend(
        physics_space
            .windows(2)
            .map(|pair| (records[pair[0]].uid.clone(), records[pair[1]].uid.clone())),
    );
    let chemistry_structure = [
        "chemistry-foundation-review",
        "chemistry-light-bridge",
        "gc-light",
        "chemistry-state-models",
        "gc-quantum",
        "gc-periodicity",
        "gc-lewis",
        "gc-resonance",
        "chemistry-charge-models",
        "gc-shape",
        "gc-interactions",
        "chemistry-thermal-bridge",
        "gc-energy",
        "gc-calorimetry",
        "gc-phases",
        "chemistry-structure-study",
        "chemistry-structure-review",
    ];
    expected_core.extend(
        chemistry_structure
            .windows(2)
            .map(|pair| (records[pair[0]].uid.clone(), records[pair[1]].uid.clone())),
    );
    let computing_collections = [
        "computing-foundation-review",
        "computing-collections",
        "computing-aliasing",
        "computing-order-contract",
        "computing-linear-search",
        "computing-binary-search",
        "computing-index-cost",
        "computing-collection-study",
        "computing-collection-review",
    ];
    expected_core.extend(
        computing_collections
            .windows(2)
            .map(|pair| (records[pair[0]].uid.clone(), records[pair[1]].uid.clone())),
    );
    let engineering_geometry = [
        "engineering-foundation-review",
        "engineering-geometric-representations",
        "ql-drawings",
        "engineering-dimension-constraints",
        "ql-tolerances",
        "engineering-interval-models",
        "mech-tolerance-stacks",
        "engineering-coupled-variation",
        "mech-dimensional-entry",
        "engineering-decision-evidence",
        "engineering-geometric-study",
        "engineering-geometric-review",
    ];
    expected_core.extend(
        engineering_geometry
            .windows(2)
            .map(|pair| (records[pair[0]].uid.clone(), records[pair[1]].uid.clone())),
    );
    let inquiry_design = [
        "inquiry-foundation-review",
        "inquiry-design-units",
        "met-calibration-design",
        "inquiry-finite-probability",
        "inquiry-conditional-dependence",
        "inquiry-expectation-variation",
        "inquiry-randomized-comparisons",
        "inquiry-evidence-limits",
        "inquiry-design-study",
        "inquiry-design-review",
    ];
    expected_core.extend(
        inquiry_design
            .windows(2)
            .map(|pair| (records[pair[0]].uid.clone(), records[pair[1]].uid.clone())),
    );
    let math_change = [
        "math-linear-review",
        "math-limit-neighborhoods",
        "math-continuity-models",
        "math-derivative-definition",
        "ql-small-changes",
        "math-derivative-rules",
        "math-local-error",
        "math-partition-sums",
        "math-integral-definition",
        "math-fundamental-theorem",
        "math-change-study",
        "math-change-review",
    ];
    expected_core.extend(
        math_change
            .windows(2)
            .map(|pair| (records[pair[0]].uid.clone(), records[pair[1]].uid.clone())),
    );
    let physics_thermal = [
        "physics-spatial-review",
        "physics-matter-state",
        "th-scales",
        "physics-temperature-evidence",
        "th-state",
        "gc-gases",
        "physics-molecular-pressure",
        "th-eos",
        "physics-energy-paths",
        "th-closed",
        "physics-boundary-work",
        "th-boundary-work",
        "physics-thermal-study",
        "physics-thermal-review",
    ];
    expected_core.extend(
        physics_thermal
            .windows(2)
            .map(|pair| (records[pair[0]].uid.clone(), records[pair[1]].uid.clone())),
    );
    let chemistry_dynamics = [
        "chemistry-structure-review",
        "chemistry-rate-balance",
        "gc-rate-laws",
        "chemistry-logarithm-bridge",
        "chemistry-exponential-bridge",
        "chemistry-initial-value",
        "gc-integrated",
        "chemistry-reversible-model",
        "gc-qk",
        "gc-perturb-equilibrium",
        "gc-catalysis",
        "chemistry-kinetic-evidence",
        "chemistry-observation-model",
        "gc-spectra",
        "gc-method",
        "chemistry-dynamics-study",
        "chemistry-dynamics-review",
    ];
    expected_core.extend(
        chemistry_dynamics
            .windows(2)
            .map(|pair| (records[pair[0]].uid.clone(), records[pair[1]].uid.clone())),
    );
    let computing_machine = [
        "computing-collection-review",
        "computing-word-encoding",
        "computing-word-arithmetic",
        "computing-boolean-logic",
        "computing-state-storage",
        "computing-rounded-numbers",
        "rc-floating",
        "computing-instruction-machine",
        "computing-machine-invariant",
        "computing-machine-study",
        "computing-machine-review",
    ];
    expected_core.extend(
        computing_machine
            .windows(2)
            .map(|pair| (records[pair[0]].uid.clone(), records[pair[1]].uid.clone())),
    );
    let engineering_network = [
        "engineering-geometric-review",
        "engineering-port-balance",
        "fd-continuity",
        "engineering-component-laws",
        "th-fourier",
        "engineering-network-orientation",
        "engineering-anchored-network",
        "engineering-storage-response",
        "th-lumped",
        "engineering-design-evidence",
        "engineering-network-study",
        "engineering-network-review",
    ];
    expected_core.extend(
        engineering_network
            .windows(2)
            .map(|pair| (records[pair[0]].uid.clone(), records[pair[1]].uid.clone())),
    );
    let inquiry_uncertainty = [
        "inquiry-design-review",
        "inquiry-uncertainty-meaning",
        "met-budget",
        "inquiry-joint-covariance",
        "inquiry-affine-uncertainty",
        "inquiry-local-sensitivity",
        "met-sensitivity",
        "inquiry-nonlinear-propagation",
        "inquiry-model-checks",
        "met-model-discrepancy",
        "inquiry-uncertainty-reporting",
        "inquiry-uncertainty-study",
        "inquiry-uncertainty-review",
    ];
    expected_core.extend(
        inquiry_uncertainty
            .windows(2)
            .map(|pair| (records[pair[0]].uid.clone(), records[pair[1]].uid.clone())),
    );
    let mathematics_periodic = [
        "math-change-review",
        "math-circle-functions",
        "math-rotation-identities",
        "math-periodic-inverses",
        "math-trig-limits",
        "math-trig-calculus",
        "math-complex-arithmetic",
        "math-complex-rotations",
        "math-second-order",
        "math-trig-approximation",
        "math-periodic-study",
        "math-periodic-review",
    ];
    expected_core.extend(
        mathematics_periodic
            .windows(2)
            .map(|pair| (records[pair[0]].uid.clone(), records[pair[1]].uid.clone())),
    );
    let physics_waves = [
        "physics-thermal-review",
        "physics-restoring",
        "physics-oscillator-loss",
        "physics-wave-field",
        "physics-traveling-profile",
        "physics-string-balance",
        "physics-wave-boundaries",
        "physics-wave-energy",
        "physics-wave-study",
        "physics-wave-review",
    ];
    expected_core.extend(
        physics_waves
            .windows(2)
            .map(|pair| (records[pair[0]].uid.clone(), records[pair[1]].uid.clone())),
    );
    let chemistry_solutions = [
        "chemistry-dynamics-review",
        "chemistry-solution-meaning",
        "gc-solution-interactions",
        "chemistry-solution-amounts",
        "gc-preparation",
        "gc-ionic",
        "chemistry-solution-charge",
        "gc-acidbase",
        "chemistry-water-activities",
        "gc-ph",
        "chemistry-acid-balance",
        "gc-weak",
        "chemistry-solution-approximation",
        "chemistry-solution-study",
        "chemistry-solution-review",
    ];
    expected_core.extend(
        chemistry_solutions
            .windows(2)
            .map(|pair| (records[pair[0]].uid.clone(), records[pair[1]].uid.clone())),
    );
    let computing_memory = [
        "computing-machine-review",
        "computing-memory-objects",
        "computing-activation-state",
        "computing-process-state",
        "computing-page-translation",
        "computing-memory-ranges",
        "computing-controlled-service",
        "computing-scheduling-boundary",
        "computing-memory-cost",
        "computing-memory-study",
        "computing-memory-review",
    ];
    expected_core.extend(
        computing_memory
            .windows(2)
            .map(|pair| (records[pair[0]].uid.clone(), records[pair[1]].uid.clone())),
    );
    let engineering_material = [
        "engineering-network-review",
        "engineering-material-meaning",
        "mech-tensile-measures",
        "engineering-axial-law",
        "mech-load-response",
        "engineering-compatible-response",
        "mech-axial-members",
        "engineering-stored-work",
        "engineering-observation-compliance",
        "engineering-realization-evidence",
        "engineering-material-study",
        "engineering-material-review",
    ];
    expected_core.extend(
        engineering_material
            .windows(2)
            .map(|pair| (records[pair[0]].uid.clone(), records[pair[1]].uid.clone())),
    );
    let inquiry_density = [
        "inquiry-uncertainty-review",
        "inquiry-density-meaning",
        "inquiry-cumulative-quantiles",
        "inquiry-continuous-moments",
        "inquiry-joint-density",
        "inquiry-estimation-model",
        "inquiry-interval-coverage",
        "inquiry-support-and-rivals",
        "inquiry-density-study",
        "inquiry-density-review",
    ];
    expected_core.extend(
        inquiry_density
            .windows(2)
            .map(|pair| (records[pair[0]].uid.clone(), records[pair[1]].uid.clone())),
    );
    let math_finite = [
        "math-periodic-review",
        "math-finite-vectors",
        "math-linear-transpose",
        "math-row-equivalence",
        "math-nullity-basis",
        "math-gram-orthogonal",
        "math-least-squares",
        "math-rank-deficient-projection",
        "math-metric-conditioning",
        "math-linear-system-study",
        "math-linear-system-review",
    ];
    expected_core.extend(
        math_finite
            .windows(2)
            .map(|pair| (records[pair[0]].uid.clone(), records[pair[1]].uid.clone())),
    );
    let physics_electric = [
        "physics-wave-review",
        "physics-charge-account",
        "em-language",
        "physics-finite-field",
        "em-coulomb",
        "physics-potential-work",
        "em-potential",
        "physics-charge-flow",
        "em-continuity",
        "physics-capacitive-work",
        "physics-circuit-meaning",
        "em-dc-network",
        "physics-charge-sharing",
        "physics-electric-observation",
        "physics-electric-study",
        "physics-electric-review",
    ];
    expected_core.extend(
        physics_electric
            .windows(2)
            .map(|pair| (records[pair[0]].uid.clone(), records[pair[1]].uid.clone())),
    );
    let chemistry_electron = [
        "chemistry-solution-review",
        "chemistry-redox-language",
        "gc-redox-balance",
        "chemistry-half-reactions",
        "chemistry-electron-amount",
        "chemistry-cell-boundaries",
        "gc-cells",
        "chemistry-product-evidence",
        "gc-electrolysis",
        "chemistry-electrical-work",
        "chemistry-electron-study",
        "chemistry-electron-review",
    ];
    expected_core.extend(
        chemistry_electron
            .windows(2)
            .map(|pair| (records[pair[0]].uid.clone(), records[pair[1]].uid.clone())),
    );
    let computing_durable = [
        "computing-memory-review",
        "computing-durability",
        "computing-file-meaning",
        "computing-storage-contract",
        "computing-crash-states",
        "computing-copy-publish",
        "computing-recovery-retry",
        "rc-atomic",
        "computing-storage-cost",
        "computing-durable-study",
        "computing-durable-review",
    ];
    expected_core.extend(
        computing_durable
            .windows(2)
            .map(|pair| (records[pair[0]].uid.clone(), records[pair[1]].uid.clone())),
    );
    let engineering_electric = [
        "engineering-material-review",
        "engineering-electrical-function",
        "engineering-node-model",
        "pe-circuits",
        "engineering-port-equivalent",
        "engineering-electrical-limits",
        "pe-operating-state",
        "engineering-electrical-bounds",
        "engineering-electrical-observation",
        "engineering-electric-study",
        "engineering-electric-review",
    ];
    expected_core.extend(
        engineering_electric
            .windows(2)
            .map(|pair| (records[pair[0]].uid.clone(), records[pair[1]].uid.clone())),
    );
    let inquiry_fitting = [
        "inquiry-density-review",
        "inquiry-fit-model",
        "met-regression",
        "inquiry-linear-estimator",
        "inquiry-fit-residuals",
        "inquiry-fit-prediction",
        "met-regression-inverse",
        "inquiry-fit-rivals",
        "inquiry-fit-replication",
        "inquiry-fitting-study",
        "inquiry-fitting-review",
    ];
    expected_core.extend(
        inquiry_fitting
            .windows(2)
            .map(|pair| (records[pair[0]].uid.clone(), records[pair[1]].uid.clone())),
    );
    let math_multivariable = [
        "math-linear-system-review",
        "math-multivariable-domains",
        "math-multivariable-limits",
        "math-partial-derivatives",
        "math-total-differentiability",
        "math-jacobian-chain",
        "math-gradient-metric",
        "math-exact-differential",
        "math-constrained-extrema",
        "math-multivariable-study",
        "math-multivariable-review",
    ];
    expected_core.extend(
        math_multivariable
            .windows(2)
            .map(|pair| (records[pair[0]].uid.clone(), records[pair[1]].uid.clone())),
    );
    let physics_entropy = [
        "physics-electric-review",
        "physics-process-direction",
        "th-reversibility",
        "physics-entropy-state",
        "th-entropy-balance",
        "physics-reference-paths",
        "th-entropy-properties",
        "physics-cycle-bound",
        "th-cycles",
        "th-carnot",
        "physics-irreversible-paths",
        "physics-finite-bodies",
        "physics-entropy-evidence",
        "physics-entropy-study",
        "physics-entropy-review",
    ];
    expected_core.extend(
        physics_entropy
            .windows(2)
            .map(|pair| (records[pair[0]].uid.clone(), records[pair[1]].uid.clone())),
    );
    let chemistry_free_energy = [
        "chemistry-electron-review",
        "chemistry-thermo-constraints",
        "th-potentials",
        "chemistry-mixture-potential",
        "chemistry-ideal-mixture",
        "chemistry-reaction-derivative",
        "gc-gibbs",
        "chemistry-equilibrium-minimum",
        "gc-chemical-potential",
        "chemistry-reversible-cell",
        "chemistry-thermo-evidence",
        "chemistry-free-energy-study",
        "chemistry-free-energy-review",
    ];
    expected_core.extend(
        chemistry_free_energy
            .windows(2)
            .map(|pair| (records[pair[0]].uid.clone(), records[pair[1]].uid.clone())),
    );
    let computing_concurrency = [
        "computing-durable-review",
        "computing-concurrent-state",
        "computing-interleavings",
        "computing-lock-invariant",
        "computing-serial-histories",
        "computing-progress-assumptions",
        "computing-predicate-wait",
        "rc-scaling",
        "computing-concurrency-evidence",
        "computing-concurrency-study",
        "computing-concurrency-review",
    ];
    expected_core.extend(
        computing_concurrency
            .windows(2)
            .map(|pair| (records[pair[0]].uid.clone(), records[pair[1]].uid.clone())),
    );
    let engineering_feedback = [
        "engineering-electric-review",
        "engineering-feedback-boundary",
        "engineering-feedback-response",
        "engineering-feedback-observation",
        "engineering-feedback-limits",
        "pe-feedback-control",
        "engineering-feedback-sampling",
        "wu-water-control",
        "engineering-feedback-requirements",
        "engineering-feedback-evidence",
        "engineering-feedback-study",
        "engineering-feedback-review",
    ];
    expected_core.extend(
        engineering_feedback
            .windows(2)
            .map(|pair| (records[pair[0]].uid.clone(), records[pair[1]].uid.clone())),
    );
    let inquiry_reporting = [
        "inquiry-fitting-review",
        "inquiry-report-question",
        "inquiry-report-chain",
        "inquiry-report-comparison",
        "inquiry-report-presentation",
        "rc-reporting",
        "inquiry-report-criticism",
        "inquiry-report-revision",
        "rc-archive",
        "inquiry-reporting-study",
        "rc-review",
        "inquiry-reporting-review",
    ];
    expected_core.extend(
        inquiry_reporting
            .windows(2)
            .map(|pair| (records[pair[0]].uid.clone(), records[pair[1]].uid.clone())),
    );
    let math_discrete = [
        "math-multivariable-review",
        "math-discrete-objects",
        "math-discrete-fibers",
        "math-discrete-counting",
        "math-discrete-induction",
        "math-discrete-relations",
        "math-discrete-graphs",
        "math-discrete-reachability",
        "math-discrete-histories",
        "math-discrete-quotients",
        "math-discrete-study",
        "math-discrete-review",
    ];
    expected_core.extend(
        math_discrete
            .windows(2)
            .map(|pair| (records[pair[0]].uid.clone(), records[pair[1]].uid.clone())),
    );
    let physics_rotation = [
        "physics-entropy-review",
        "physics-rotation-motion",
        "physics-angular-balance",
        "physics-rotation-inertia",
        "mech-mass-inertia",
        "physics-rotation-work",
        "mech-angular-momentum",
        "physics-rotation-rolling",
        "physics-rotation-exchange",
        "physics-rotation-observation",
        "physics-rotation-study",
        "physics-rotation-review",
    ];
    expected_core.extend(
        physics_rotation
            .windows(2)
            .map(|pair| (records[pair[0]].uid.clone(), records[pair[1]].uid.clone())),
    );
    let chemistry_identity = [
        "chemistry-free-energy-review",
        "chemistry-identity-levels",
        "chemistry-connectivity",
        "chemistry-skeleton-counts",
        "chemistry-conformations",
        "chemistry-spatial-identity",
        "chemistry-configuration",
        "chemistry-structure-evidence",
        "chemistry-identity-study",
        "chemistry-identity-review",
    ];
    expected_core.extend(
        chemistry_identity
            .windows(2)
            .map(|pair| (records[pair[0]].uid.clone(), records[pair[1]].uid.clone())),
    );
    let computing_communication = [
        "computing-concurrency-review",
        "computing-channel-boundaries",
        "computing-message-framing",
        "computing-channel-failures",
        "computing-request-protocol",
        "computing-protocol-invariant",
        "computing-message-lifetimes",
        "computing-communication-progress",
        "computing-communication-study",
        "computing-communication-review",
    ];
    expected_core.extend(
        computing_communication
            .windows(2)
            .map(|pair| (records[pair[0]].uid.clone(), records[pair[1]].uid.clone())),
    );
    let engineering_production = [
        "engineering-feedback-review",
        "engineering-production-boundary",
        "engineering-production-visits",
        "engineering-production-timing",
        "engineering-production-scheduling",
        "engineering-production-rework",
        "engineering-production-inventory-time",
        "engineering-production-decision",
        "engineering-production-study",
        "engineering-production-review",
    ];
    expected_core.extend(
        engineering_production
            .windows(2)
            .map(|pair| (records[pair[0]].uid.clone(), records[pair[1]].uid.clone())),
    );
    let inquiry_learning = [
        "inquiry-reporting-review",
        "inquiry-learning-capability",
        "inquiry-learning-prerequisites",
        "inquiry-learning-practice",
        "inquiry-learning-assessment",
        "inquiry-learning-scoring",
        "inquiry-learning-comparison",
        "inquiry-learning-study",
        "inquiry-learning-handover",
        "inquiry-learning-review",
    ];
    expected_core.extend(
        inquiry_learning
            .windows(2)
            .map(|pair| (records[pair[0]].uid.clone(), records[pair[1]].uid.clone())),
    );
    let math_series = [
        "math-discrete-review",
        "math-sequence-limits",
        "math-sequence-completeness",
        "math-series-sums",
        "math-series-tails",
        "math-series-order",
        "math-series-uniform",
        "math-series-operations",
        "math-series-taylor",
        "math-series-weights",
        "math-series-truncation",
        "math-series-study",
        "math-series-review",
    ];
    expected_core.extend(
        math_series
            .windows(2)
            .map(|pair| (records[pair[0]].uid.clone(), records[pair[1]].uid.clone())),
    );
    let physics_statistical = [
        "physics-rotation-review",
        "physics-statistical-states",
        "physics-statistical-constraints",
        "physics-statistical-entropy",
        "physics-statistical-reservoir",
        "physics-statistical-canonical",
        "physics-statistical-fluctuations",
        "physics-statistical-countable",
        "physics-statistical-dynamics",
        "physics-statistical-study",
        "physics-statistical-review",
    ];
    expected_core.extend(
        physics_statistical
            .windows(2)
            .map(|pair| (records[pair[0]].uid.clone(), records[pair[1]].uid.clone())),
    );
    let chemistry_phase = [
        "chemistry-identity-review",
        "chemistry-phase-description",
        "chemistry-phase-coexistence",
        "chemistry-phase-correlation",
        "chemistry-phase-amounts",
        "mat-common-tangent",
        "chemistry-phase-tangent",
        "mat-regular-solution",
        "chemistry-phase-stability",
        "mat-regular-example",
        "chemistry-phase-certificates",
        "chemistry-phase-evidence",
        "chemistry-phase-study",
        "chemistry-phase-review",
    ];
    expected_core.extend(
        chemistry_phase
            .windows(2)
            .map(|pair| (records[pair[0]].uid.clone(), records[pair[1]].uid.clone())),
    );
    let computing_abstraction = [
        "computing-communication-review",
        "computing-abstraction-bindings",
        "computing-abstraction-contracts",
        "computing-abstraction-values",
        "computing-abstraction-representations",
        "computing-abstraction-refinement",
        "computing-abstraction-exposure",
        "rc-modules",
        "computing-abstraction-composition",
        "computing-abstraction-change",
        "computing-abstraction-study",
        "computing-abstraction-review",
    ];
    expected_core.extend(
        computing_abstraction
            .windows(2)
            .map(|pair| (records[pair[0]].uid.clone(), records[pair[1]].uid.clone())),
    );
    let engineering_service = [
        "engineering-production-review",
        "engineering-service-history",
        "engineering-service-mission",
        "engineering-service-exposure",
        "engineering-service-dependencies",
        "mech-maintenance",
        "engineering-service-cycles",
        "engineering-service-resources",
        "mech-condition-evidence",
        "mech-failure-investigation",
        "mech-restoration",
        "engineering-service-evidence",
        "engineering-service-study",
        "engineering-service-review",
    ];
    expected_core.extend(
        engineering_service
            .windows(2)
            .map(|pair| (records[pair[0]].uid.clone(), records[pair[1]].uid.clone())),
    );
    let inquiry_causal = [
        "inquiry-learning-review",
        "inquiry-causal-question",
        "inquiry-causal-identification",
        "inquiry-causal-standardization",
        "inquiry-causal-weighting",
        "inquiry-causal-ambiguity",
        "inquiry-causal-adjustment",
        "inquiry-causal-missing",
        "inquiry-causal-study",
        "inquiry-causal-review",
    ];
    expected_core.extend(
        inquiry_causal
            .windows(2)
            .map(|pair| (records[pair[0]].uid.clone(), records[pair[1]].uid.clone())),
    );
    let math_operators = [
        "math-series-review",
        "math-operator-vectors",
        "math-operator-projection",
        "math-operator-adjoint",
        "math-operator-eigen",
        "math-operator-basis",
        "math-operator-scalar-exponential",
        "math-operator-matrix-exponential",
        "math-operator-evolution",
        "math-operator-order",
        "math-operator-study",
        "math-operator-review",
    ];
    expected_core.extend(
        math_operators
            .windows(2)
            .map(|pair| (records[pair[0]].uid.clone(), records[pair[1]].uid.clone())),
    );
    let physics_quantum = [
        "physics-statistical-review",
        "physics-quantum-evidence",
        "physics-quantum-state",
        "physics-quantum-observable",
        "physics-quantum-measurement",
        "physics-quantum-mixtures",
        "physics-quantum-uncertainty",
        "physics-quantum-dynamics",
        "physics-quantum-wavefunction",
        "physics-quantum-composite",
        "physics-quantum-observations",
        "physics-quantum-study",
        "physics-quantum-review",
    ];
    expected_core.extend(
        physics_quantum
            .windows(2)
            .map(|pair| (records[pair[0]].uid.clone(), records[pair[1]].uid.clone())),
    );
    let computing_trust = [
        "computing-abstraction-review",
        "computing-trust-task",
        "computing-trust-state",
        "computing-trust-accessible",
        "computing-trust-authority",
        "computing-trust-provenance",
        "computing-trust-privacy",
        "computing-trust-evaluation",
        "computing-trust-study",
        "computing-trust-review",
    ];
    expected_core.extend(
        computing_trust
            .windows(2)
            .map(|pair| (records[pair[0]].uid.clone(), records[pair[1]].uid.clone())),
    );
    let engineering_operation = [
        "engineering-service-review",
        "engineering-operation-task",
        "engineering-operation-state",
        "engineering-operation-observation",
        "engineering-operation-error",
        "engineering-operation-recovery",
        "engineering-operation-handover",
        "engineering-operation-branches",
        "engineering-operation-study",
        "engineering-operation-review",
    ];
    expected_core.extend(
        engineering_operation
            .windows(2)
            .map(|pair| (records[pair[0]].uid.clone(), records[pair[1]].uid.clone())),
    );
    let math_fields = [
        "math-operator-review",
        "math-fields-regions",
        "math-fields-paths",
        "math-fields-orientation",
        "math-fields-coordinates",
        "math-fields-flux",
        "math-fields-local",
        "math-fields-green",
        "math-fields-gauss",
        "math-fields-stokes",
        "math-fields-balance",
        "math-fields-study",
        "math-fields-review",
    ];
    expected_core.extend(
        math_fields
            .windows(2)
            .map(|pair| (records[pair[0]].uid.clone(), records[pair[1]].uid.clone())),
    );
    let magnetic_fields = [
        "physics-quantum-review",
        "physics-magnetic-question",
        "em-magnetic-force",
        "physics-magnetic-orbit",
        "physics-magnetic-steady",
        "em-ampere",
        "physics-magnetic-flux",
        "em-induction",
        "physics-magnetic-moving",
        "physics-magnetic-storage",
        "physics-magnetic-maxwell",
        "physics-magnetic-energy",
        "physics-magnetic-waves",
        "physics-magnetic-study",
        "physics-magnetic-review",
    ];
    expected_core.extend(
        magnetic_fields
            .windows(2)
            .map(|pair| (records[pair[0]].uid.clone(), records[pair[1]].uid.clone())),
    );
    let spacetime_foundations = [
        "physics-magnetic-review",
        "physics-spacetime-events",
        "physics-spacetime-boost",
        "physics-spacetime-clocks",
        "physics-spacetime-causality",
        "physics-spacetime-energy",
        "em-relativity",
        "physics-spacetime-gravity",
        "physics-spacetime-orbits",
        "physics-spacetime-equivalence",
        "physics-spacetime-weakclocks",
        "physics-spacetime-evidence",
        "physics-spacetime-study",
        "physics-spacetime-review",
    ];
    expected_core.extend(
        spacetime_foundations
            .windows(2)
            .map(|pair| (records[pair[0]].uid.clone(), records[pair[1]].uid.clone())),
    );
    let astronomy_foundations = [
        "physics-spacetime-review",
        "physics-astronomy-directions",
        "physics-astronomy-parallax",
        "physics-astronomy-detectors",
        "physics-astronomy-brightness",
        "physics-astronomy-spectra",
        "physics-astronomy-planets",
        "physics-astronomy-stars",
        "physics-astronomy-populations",
        "physics-astronomy-galaxies",
        "physics-astronomy-cosmos",
        "physics-astronomy-evidence",
        "physics-astronomy-study",
        "physics-astronomy-review",
    ];
    expected_core.extend(
        astronomy_foundations
            .windows(2)
            .map(|pair| (records[pair[0]].uid.clone(), records[pair[1]].uid.clone())),
    );
    assert_eq!(
        learning_edges(&publication, "hephaestus-core"),
        expected_core
    );
    let selected: BTreeSet<_> = expected_core
        .iter()
        .flat_map(|(from, to)| [from.clone(), to.clone()])
        .collect();
    assert_eq!(selected.len(), 644);
    let mut reached = BTreeSet::new();
    let mut pending = vec![root];
    while let Some(uid) = pending.pop() {
        if reached.insert(uid.clone()) {
            pending.extend(
                expected_core
                    .iter()
                    .filter(|(from, _)| *from == uid)
                    .map(|(_, to)| to.clone()),
            );
        }
    }
    assert_eq!(reached, selected);
    for excluded in [
        "engineering-source-human-factors",

        "computing-source-interaction",
        "computing-source-accessibility",
        "computing-source-protection",
        "computing-source-privacy",

        "physics-source-quantum",
        "math-source-operators",
        "inquiry-source-causal",
        "computing-source-abstraction",
        "rc-rse",
        "mat-mit-fundamentals",
        "gc-webbook",
        "th-devoe",
        "mat-tong-statistical",
        "math-source-analysis",
        "inquiry-source-learning",
        "inquiry-source-assessment",
        "inquiry-source-retrieval",

        "engineering-source-production",
        "engineering-source-little",
        "computing-source-tcp",
        "computing-source-rpc",
        "chemistry-source-organic",
        "math-source-discrete",
        "engineering-source-process-control",
        "engineering-source-digital-control",
        "water-and-sanitation",
        "computing-source-structures",
        "gc-harvey",
        "em-openstax",
        "th-yan",
        "thermodynamics-and-heat-transfer",
        "electromagnetism",
        "ic-calculus",
        "industrial-chemistry",
        "inquiry-source-probability",
        "mech-npl-drawings",
        "ql-source-blueprint",
        "mech-desenho-pt",
        "computing-source-algorithms",
        "pe-linear-algebra",
        "electrical-power-engineering",
        "math-source-proof",
        "ql-source-algebra",
        "ql-source-prealgebra",
        "quantitative-literacy",
        "ql-sources",
        "fd-openstax",
        "fluid-dynamics",
        "rc-python",
        "rc-sundnes",
        "reproducible-computation",
        "gc-openstax",
        "general-chemistry",
        "met-bell",
        "met-handbook",
        "ql-source-vim",
        "rc-practices",
        "rc-peng",
        "metrology",
        "mech-nasa-systems",
        "mech-slocum",
        "mech-statics-book",
        "mechanical-engineering",
    ] {
        assert!(!selected.contains(&records[excluded].uid));
    }
    let engineering = learning_edges(&publication, "engineering");
    for nested in [
        "resources-materials-map",
        "utilities",
        "production-quality-map",
    ] {
        assert!(engineering.contains(&(
            records["engineering-map"].uid.clone(),
            records[nested].uid.clone(),
        )));
        assert!(!selected.contains(&records[nested].uid));
    }
    assert!(!selected.contains(&records["clean-manufacturing"].uid));
    assert_eq!(publication.documents.len(), 3752);
    let identities: BTreeSet<_> = records.values().map(|record| record.uid.clone()).collect();
    assert_eq!(identities.len(), 3752);
    for document in publication.documents.iter().filter(|document| {
        document.project.records.iter().any(|record| {
            record.slug.as_deref().is_some_and(|slug| {
                [
                    "math-source-proof",
                    "physics-spatial-review",
                    "math-structures-review",
                ]
                .contains(&slug)
            })
        })
    })
    {
        let from_entry = Publication::load(&library, &document.uri, &[], false).unwrap();
        let discovered: BTreeSet<_> = from_entry
            .documents
            .iter()
            .flat_map(|document| &document.project.records)
            .map(|record| record.uid.clone())
            .collect();
        assert_eq!(discovered, identities, "{}", document.uri);
    }
    let standalone = Publication::load(
        &library,
        directory
            .join("inquiry/quantitative-literacy/foundations/Quantity-and-reference.lingua")
            .to_str()
            .unwrap(),
        &[],
        false,
    )
    .unwrap();
    assert_eq!(standalone.documents.len(), 53);
    let original_quantitative = learning_edges(&standalone, "quantitative-literacy");
    assert_eq!(
        learning_edges(&publication, "quantitative-literacy"),
        original_quantitative
    );
    let mut shared_routes = BTreeMap::new();
    for (entry, predicate, count) in [
        (
            "chemistry/general-chemistry/sources/Analytical-Chemistry-Harvey.lingua",
            "general-chemistry",
            319,
        ),
        (
            "physics/electromagnetism/sources/University-Physics-Volume-2.lingua",
            "electromagnetism",
            346,
        ),
        (
            "physics/thermodynamics-and-heat-transfer/sources/Introduction-to-Engineering-Thermodynamics-Yan.lingua",
            "thermodynamics-and-heat-transfer",
            623,
        ),
        (
            "chemistry/industrial-chemistry/sources/Calculus-and-Differential-Equations-OpenStax.lingua",
            "industrial-chemistry",
            997,
        ),
        (
            "engineering/electrical-power-engineering/sources/Introduction-to-Applied-Linear-Algebra-Boyd.lingua",
            "electrical-power-engineering",
            525,
        ),
        (
            "inquiry/metrology/foundations/Question-and-measurand.lingua",
            "metrology",
            132,
        ),
        (
            "inquiry/reproducible-computation/foundations/Files-paths-and-processes.lingua",
            "reproducible-computation",
            212,
        ),
        (
            "chemistry/general-chemistry/foundations/A-chemical-question.lingua",
            "general-chemistry",
            319,
        ),
        (
            "engineering/mechanical-engineering-and-machine-design/statics/Forces-are-interactions-across-the-chosen-boundary.lingua",
            "mechanical-engineering",
            1018,
        ),
        (
            "physics/fluid-dynamics/sources/University-Physics-Volume-1.lingua",
            "fluid-dynamics",
            802,
        ),
        (
            "utilities/water-and-sanitation/operations/A-tank-balance-explains-control-offset-delay-and-saturation.lingua",
            "water-and-sanitation",
            1247,
        ),
    ] {
        let original = Publication::load(
            &library,
            directory.join(entry).to_str().unwrap(),
            &[],
            false,
        )
        .unwrap();
        assert_eq!(original.documents.len(), count);
        assert!(!original.documents.iter().any(|document| {
            document
                .project
                .records
                .iter()
                .any(|record| record.slug.as_deref() == Some("hephaestus"))
        }));
        let edges = learning_edges(&original, predicate);
        assert!(!edges.is_empty());
        assert_eq!(learning_edges(&publication, predicate), edges);
        shared_routes.insert(predicate, edges);
    }
    let stock_program = records["computing-foundation-study"]
        .body
        .split("```python file=stock_trace.py\n")
        .nth(1)
        .unwrap()
        .split("\n```")
        .next()
        .unwrap()
        .to_owned();
    assert!(stock_program.contains("def trace_stock("));
    let catalog_programs: Vec<_> = ["catalog_lookup.py", "catalog_checks.py"]
        .into_iter()
        .map(|filename| {
            let marker = format!("```python file={filename}\n");
            records["computing-collection-study"]
                .body
                .split(&marker)
                .nth(1)
                .unwrap()
                .split("\n```")
                .next()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert!(catalog_programs[0].contains("def lookup_prepared("));
    assert!(catalog_programs[1].contains("import catalog_lookup as m"));
    let allocation_programs: Vec<_> = ["allocation_study.py", "allocation_checks.py"]
        .into_iter()
        .map(|filename| {
            let marker = format!("```python file={filename}\n");
            records["inquiry-design-study"]
                .body
                .split(&marker)
                .nth(1)
                .unwrap()
                .split("\n```")
                .next()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert!(allocation_programs[0].contains("def summarize("));
    assert!(allocation_programs[1].contains("import allocation_study as m"));
    let change_programs: Vec<_> = ["change_study.py", "change_checks.py"]
        .into_iter()
        .map(|filename| {
            let marker = format!("```python file={filename}\n");
            records["math-change-study"]
                .body
                .split(&marker)
                .nth(1)
                .unwrap()
                .split("\n```")
                .next()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert!(change_programs[0].contains("def partition_sums("));
    assert!(change_programs[1].contains("import change_study as m"));
    let thermal_programs: Vec<_> = ["thermal_study.py", "thermal_checks.py"]
        .into_iter()
        .map(|filename| {
            let marker = format!("```python file={filename}\n");
            records["physics-thermal-study"]
                .body
                .split(&marker)
                .nth(1)
                .unwrap()
                .split("\n```")
                .next()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert!(thermal_programs[0].contains("def moments("));
    assert!(thermal_programs[1].contains("import thermal_study as m"));
    let reaction_programs: Vec<_> = ["reaction_study.py", "reaction_checks.py"]
        .into_iter()
        .map(|filename| {
            let marker = format!("```python file={filename}\n");
            records["chemistry-dynamics-study"]
                .body
                .split(&marker)
                .nth(1)
                .unwrap()
                .split("\n```")
                .next()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert!(reaction_programs[0].contains("def state("));
    assert!(reaction_programs[1].contains("def decay_bounds("));
    let machine_programs: Vec<_> = ["machine_study.py", "machine_checks.py"]
        .into_iter()
        .map(|filename| {
            let marker = format!("```python file={filename}\n");
            records["computing-machine-study"]
                .body
                .split(&marker)
                .nth(1)
                .unwrap()
                .split("\n```")
                .next()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert!(machine_programs[0].contains("def step("));
    assert!(machine_programs[1].contains("expected_pcs"));
    let floating_program = records["rc-floating"]
        .body
        .split("```python file=floating.py\n")
        .nth(1)
        .unwrap()
        .split("\n```")
        .next()
        .unwrap()
        .to_owned();
    let network_programs: Vec<_> = ["network_study.py", "network_checks.py"]
        .into_iter()
        .map(|filename| {
            let marker = format!("```python file={filename}\n");
            records["engineering-network-study"]
                .body
                .split(&marker)
                .nth(1)
                .unwrap()
                .split("\n```")
                .next()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert!(network_programs[0].contains("def steady("));
    assert!(network_programs[1].contains("def decay_bounds("));
    let uncertainty_programs: Vec<_> = ["uncertainty_study.py", "uncertainty_checks.py"]
        .into_iter()
        .map(|filename| {
            let marker = format!("```python file={filename}\n");
            records["inquiry-uncertainty-study"]
                .body
                .split(&marker)
                .nth(1)
                .unwrap()
                .split("\n```")
                .next()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert!(uncertainty_programs[0].contains("def conductance("));
    assert!(uncertainty_programs[1].contains("def pair_cov("));
    let periodic_programs: Vec<_> = ["periodic_study.py", "periodic_checks.py"]
        .into_iter()
        .map(|filename| {
            let marker = format!("```python file={filename}\n");
            records["math-periodic-study"]
                .body
                .split(&marker)
                .nth(1)
                .unwrap()
                .split("\n```")
                .next()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert!(periodic_programs[0].contains("def sine_cosine("));
    assert!(periodic_programs[1].contains("def reference("));
    let wave_programs: Vec<_> = ["wave_study.py", "wave_checks.py"]
        .into_iter()
        .map(|filename| {
            let marker = format!("```python file={filename}\n");
            records["physics-wave-study"]
                .body
                .split(&marker)
                .nth(1)
                .unwrap()
                .split("\n```")
                .next()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert!(wave_programs[0].contains("def energy_change("));
    assert!(wave_programs[1].contains("integrated_right"));
    let solution_programs: Vec<_> = ["solution_study.py", "solution_checks.py"]
        .into_iter()
        .map(|filename| {
            let marker = format!("```python file={filename}\n");
            records["chemistry-solution-study"]
                .body
                .split(&marker)
                .nth(1)
                .unwrap()
                .split("\n```")
                .next()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert!(solution_programs[0].contains("def solve("));
    assert!(solution_programs[1].contains("def hydroxide_reference("));
    let space_programs: Vec<_> = ["space_study.py", "space_checks.py"]
        .into_iter()
        .map(|filename| {
            let marker = format!("```python file={filename}\n");
            records["computing-memory-study"]
                .body
                .split(&marker)
                .nth(1)
                .unwrap()
                .split("\n```")
                .next()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert!(space_programs[0].contains("def translate("));
    assert!(space_programs[1].contains("def reference("));
    let axial_programs: Vec<_> = ["axial_study.py", "axial_checks.py"]
        .into_iter()
        .map(|filename| {
            let marker = format!("```python file={filename}\n");
            records["engineering-material-study"]
                .body
                .split(&marker)
                .nth(1)
                .unwrap()
                .split("\n```")
                .next()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert!(axial_programs[0].contains("def parallel("));
    assert!(axial_programs[1].contains("def linear_reference("));
    let density_programs: Vec<_> = ["density_study.py", "density_checks.py"]
        .into_iter()
        .map(|filename| {
            let marker = format!("```python file={filename}\n");
            records["inquiry-density-study"]
                .body
                .split(&marker)
                .nth(1)
                .unwrap()
                .split("\n```")
                .next()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert!(density_programs[0].contains("def mean_cdf("));
    assert!(density_programs[1].contains("def square_area_cdf("));
    let linear_space_programs: Vec<_> = ["linear_space_study.py", "linear_space_checks.py"]
        .into_iter()
        .map(|filename| {
            let marker = format!("```python file={filename}\n");
            records["math-linear-system-study"]
                .body
                .split(&marker)
                .nth(1)
                .unwrap()
                .split("\n```")
                .next()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert!(linear_space_programs[0].contains("def orthogonal_columns("));
    assert!(linear_space_programs[1].contains("def minor_rank("));
    let electric_programs: Vec<_> = ["electric_study.py", "electric_checks.py"]
        .into_iter()
        .map(|filename| {
            let marker = format!("```python file={filename}\n");
            records["physics-electric-study"]
                .body
                .split(&marker)
                .nth(1)
                .unwrap()
                .split("\n```")
                .next()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert!(electric_programs[0].contains("def sharing("));
    assert!(electric_programs[1].contains("def midpoint_work("));
    let redox_programs: Vec<_> = ["redox_study.py", "redox_checks.py"]
        .into_iter()
        .map(|filename| {
            let marker = format!("```python file={filename}\n");
            records["chemistry-electron-study"]
                .body
                .split(&marker)
                .nth(1)
                .unwrap()
                .split("\n```")
                .next()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert!(redox_programs[0].contains("from linear_space_study import eliminate"));
    assert!(redox_programs[1].contains("def atom_charge_sum("));
    let durable_programs: Vec<_> = ["durable_study.py", "durable_checks.py"]
        .into_iter()
        .map(|filename| {
            let marker = format!("```python file={filename}\n");
            records["computing-durable-study"]
                .body
                .split(&marker)
                .nth(1)
                .unwrap()
                .split("\n```")
                .next()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert!(durable_programs[0].contains("def explore("));
    assert!(durable_programs[1].contains("def subset_images("));
    let realization_programs: Vec<_> = ["realization_study.py", "realization_checks.py"]
        .into_iter()
        .map(|filename| {
            let marker = format!("```python file={filename}\n");
            records["engineering-electric-study"]
                .body
                .split(&marker)
                .nth(1)
                .unwrap()
                .split("\n```")
                .next()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert!(realization_programs[0].contains("from linear_space_study import eliminate"));
    assert!(realization_programs[1].contains("def reduced("));
    let fitting_programs: Vec<_> = ["fitting_study.py", "fitting_checks.py"]
        .into_iter()
        .map(|filename| {
            let marker = format!("```python file={filename}\n");
            records["inquiry-fitting-study"]
                .body
                .split(&marker)
                .nth(1)
                .unwrap()
                .split("\n```")
                .next()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert!(fitting_programs[0].contains("from linear_space_study import eliminate"));
    assert!(fitting_programs[1].contains("def centered("));
    let multivariable_programs: Vec<_> = ["multivariable_study.py", "multivariable_checks.py"]
        .into_iter()
        .map(|filename| {
            let marker = format!("```python file={filename}\n");
            records["math-multivariable-study"]
                .body
                .split(&marker)
                .nth(1)
                .unwrap()
                .split("\n```")
                .next()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert!(multivariable_programs[0].contains("from linear_space_study import eliminate"));
    assert!(multivariable_programs[1].contains("def compose("));
    let entropy_programs: Vec<_> = ["entropy_study.py", "entropy_checks.py"]
        .into_iter()
        .map(|filename| {
            let marker = format!("```python file={filename}\n");
            records["physics-entropy-study"]
                .body
                .split(&marker)
                .nth(1)
                .unwrap()
                .split("\n```")
                .next()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert!(entropy_programs[0].contains("from reaction_study import decimal"));
    assert!(entropy_programs[1].contains("def logarithm_bounds("));
    let free_energy_programs: Vec<_> = ["free_energy_study.py", "free_energy_checks.py"]
        .into_iter()
        .map(|filename| {
            let marker = format!("```python file={filename}\n");
            records["chemistry-free-energy-study"]
                .body
                .split(&marker)
                .nth(1)
                .unwrap()
                .split("\n```")
                .next()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert!(free_energy_programs[0].contains("from reaction_study import decimal"));
    assert!(free_energy_programs[1].contains("from entropy_checks import logarithm_bounds"));
    let concurrency_programs: Vec<_> = ["concurrency_study.py", "concurrency_checks.py"]
        .into_iter()
        .map(|filename| {
            let marker = format!("```python file={filename}\n");
            records["computing-concurrency-study"]
                .body
                .split(&marker)
                .nth(1)
                .unwrap()
                .split("\n```")
                .next()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert!(concurrency_programs[0].contains("def wait_successors("));
    assert!(concurrency_programs[1].contains("def serial_prefix_oracle("));
    let feedback_programs: Vec<_> = ["feedback_study.py", "feedback_checks.py"]
        .into_iter()
        .map(|filename| {
            let marker = format!("```python file={filename}\n");
            records["engineering-feedback-study"]
                .body
                .split(&marker)
                .nth(1)
                .unwrap()
                .split("\n```")
                .next()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert!(feedback_programs[0].contains("from network_study import response, number"));
    assert!(feedback_programs[1].contains("def root_oracle("));
    assert!(records["computing-map"].body.contains("**Developed sixth common foundation:** concurrent actors and atomic steps"));
    assert!(records["engineering-map"].body.contains("**Developed sixth common foundation:** feedback boundaries"));
    let reporting_programs: Vec<_> = ["reporting_study.py", "reporting_checks.py"]
        .into_iter()
        .map(|filename| {
            let marker = format!("```python file={filename}\n");
            records["inquiry-reporting-study"]
                .body
                .split(&marker)
                .nth(1)
                .unwrap()
                .split("\n```")
                .next()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert!(reporting_programs[0].contains("from fitting_study import fit, partition"));
    assert!(reporting_programs[1].contains("def oracle("));
    assert!(records["inquiry-map"].body.contains("**Developed sixth common foundation:** scientific questions and audiences"));
    let discrete_programs: Vec<_> = ["discrete_study.py", "discrete_checks.py"]
        .into_iter()
        .map(|filename| {
            let marker = format!("```python file={filename}\n");
            records["math-discrete-study"]
                .body
                .split(&marker)
                .nth(1)
                .unwrap()
                .split("\n```")
                .next()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert!(discrete_programs[0].contains("def quotient("));
    assert!(discrete_programs[1].contains("def closure_oracle("));
    assert!(records["mathematics-map"].body.contains("**Developed seventh common foundation:** finite objects and equality"));
    let rotation_programs: Vec<_> = ["rotation_study.py", "rotation_checks.py"]
        .into_iter()
        .map(|filename| {
            let marker = format!("```python file={filename}\n");
            records["physics-rotation-study"]
                .body
                .split(&marker)
                .nth(1)
                .unwrap()
                .split("\n```")
                .next()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert!(rotation_programs[0].contains("def rolling("));
    assert!(rotation_programs[1].contains("expected = eliminate("));
    assert!(records["physics-map"].body.contains("**Developed seventh common foundation:** orientation and angular motion"));
    let identity_programs: Vec<_> = ["identity_study.py", "identity_checks.py"]
        .into_iter()
        .map(|filename| {
            let marker = format!("```python file={filename}\n");
            records["chemistry-identity-study"]
                .body
                .split(&marker)
                .nth(1)
                .unwrap()
                .split("\n```")
                .next()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert!(identity_programs[0].contains("def tetrahedral_classes("));
    assert!(identity_programs[1].contains("def prufer_tree("));
    assert!(records["chemistry-map"].body.contains("**Developed seventh common foundation:** molecular identity across composition"));
    let communication_programs: Vec<_> = ["communication_study.py", "communication_checks.py"]
        .into_iter()
        .map(|filename| {
            let marker = format!("```python file={filename}\n");
            records["computing-communication-study"]
                .body
                .split(&marker)
                .nth(1)
                .unwrap()
                .split("\n```")
                .next()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert!(communication_programs[0].contains("class Decoder:"));
    assert!(communication_programs[1].contains("def abstract_graph("));
    assert!(records["computing-map"].body.contains("**Developed seventh common foundation:** communication boundaries"));
    let production_programs: Vec<_> = ["production_study.py", "production_checks.py"]
        .into_iter()
        .map(|filename| {
            let marker = format!("```python file={filename}\n");
            records["engineering-production-study"]
                .body
                .split(&marker)
                .nth(1)
                .unwrap()
                .split("\n```")
                .next()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert!(production_programs[0].contains("def window_area("));
    assert!(production_programs[1].contains("def assignments("));
    assert!(records["engineering-map"].body.contains("**Developed seventh common foundation:** useful production"));
    let learning_link = regex::Regex::new(r"(\[[^\]\n]*\]\()[^)\n]+#(r_[A-Z0-9]+)\)").unwrap();
    let learning_body = |body: &str| learning_link.replace_all(body, "$1#$2)").into_owned();
    let learning_material: Vec<_> = inquiry_learning[1..]
        .iter()
        .map(|slug| (*slug, learning_body(&records[slug].body)))
        .collect();
    assert!(records["inquiry-learning-assessment"].body.contains("| O | 1, 2 | 0, 1 | 2 | 0 |"));
    assert!(records["inquiry-learning-study"].body.contains("fork-and-join dependency"));
    assert!(records["inquiry-learning-scoring"].body.contains("(1,1,0)"));
    assert!(records["inquiry-map"].body.contains("**Developed seventh common foundation:** learning capabilities"));
    let series_programs: Vec<_> = ["series_study.py", "series_checks.py"]
        .into_iter()
        .map(|filename| {
            let marker = format!("```python file={filename}\n");
            records["math-series-study"]
                .body
                .split(&marker)
                .nth(1)
                .unwrap()
                .split("\n```")
                .next()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert!(series_programs[0].contains("def select_cutoff("));
    assert!(series_programs[1].contains("pairs=[(i,j,r**(i+j))"));
    assert!(records["mathematics-map"].body.contains("**Developed eighth common foundation:** sequences"));
    assert!(records["ic-calculus"].body.contains("**Eighth mathematics-core inspection"));
    let statistical_programs: Vec<_> = ["statistical_study.py", "statistical_checks.py"]
        .into_iter()
        .map(|filename| {
            let marker = format!("```python file={filename}\n");
            records["physics-statistical-study"]
                .body
                .split(&marker)
                .nth(1)
                .unwrap()
                .split("\n```")
                .next()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert!(statistical_programs[0].contains("def time_mean_variance("));
    assert!(statistical_programs[1].contains("for path in product((0, 1), repeat=length):"));
    assert!(records["physics-map"].body.contains("**Developed eighth common foundation:** microscopic states"));
    assert!(records["mat-tong-statistical"].body.contains("**Eighth physics-core inspection"));
    let phase_programs: Vec<_> = ["phase_study.py", "phase_checks.py"]
        .into_iter()
        .map(|filename| {
            let marker = format!("```python file={filename}\n");
            records["chemistry-phase-study"]
                .body
                .split(&marker)
                .nth(1)
                .unwrap()
                .split("\n```")
                .next()
                .unwrap()
                .to_owned()
        })
        .collect();
    let shared_phase_program = records["mat-regular-example"]
        .body
        .split("```python\n")
        .nth(1)
        .unwrap()
        .split("\n```")
        .next()
        .unwrap()
        .to_owned();
    assert!(phase_programs[0].contains("def certify_binodal("));
    assert!(phase_programs[1].contains("alternatives=[(i,total-i)"));
    assert!(shared_phase_program.contains("def symmetric_regular_solution("));
    assert!(records["chemistry-map"].body.contains("**Developed eighth common foundation:** phases and constraints"));
    for slug in ["th-devoe", "gc-webbook", "mat-mit-fundamentals"] {
        assert!(records[slug].body.contains("**Eighth chemistry-core inspection"));
    }
    let abstraction_programs: Vec<_> = ["abstraction_study.py", "abstraction_checks.py"]
        .into_iter()
        .map(|filename| {
            let marker = format!("```python file={filename}\n");
            records["computing-abstraction-study"]
                .body
                .split(&marker)
                .nth(1)
                .unwrap()
                .split("\n```")
                .next()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert!(abstraction_programs[0].contains("def reserve_many("));
    assert!(abstraction_programs[1].contains("for history in product(alphabet, repeat=length):"));
    assert!(records["computing-map"].body.contains("**Developed eighth common foundation:** names and captured environments"));
    assert!(records["rc-modules"].body.contains("**Eighth computing-block reading:"));
    for slug in ["rc-python", "rc-rse"] {
        assert!(records[slug].body.contains("**Eighth computing-core inspection"));
    }
    let service_programs: Vec<_> = ["service_study.py", "service_checks.py"]
        .into_iter()
        .map(|filename| {
            let marker = format!("```python file={filename}\n");
            records["engineering-service-study"]
                .body
                .split(&marker)
                .nth(1)
                .unwrap()
                .split("\n```")
                .next()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert!(service_programs[0].contains("def evaluate("));
    assert!(service_programs[1].contains("for history in product(events("));
    assert!(records["engineering-map"].body.contains("**Developed eighth common foundation:** operating histories"));
    for slug in ["mech-maintenance", "mech-condition-evidence", "mech-failure-investigation", "mech-restoration"] {
        assert!(records[slug].body.contains("**Eighth engineering-block reading:"));
    }
    for slug in ["met-handbook", "mech-nasa-systems"] {
        assert!(records[slug].body.contains("**Eighth engineering-core inspection"));
    }
    let causal_programs: Vec<_> = ["causal_study.py", "causal_checks.py"]
        .into_iter()
        .map(|filename| {
            let marker = format!("```python file={filename}\n");
            records["inquiry-causal-study"]
                .body
                .split(&marker)
                .nth(1)
                .unwrap()
                .split("\n```")
                .next()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert!(causal_programs[0].contains("def horvitz_thompson("));
    assert!(causal_programs[1].contains("observed(null_model) == observed(effect_model)"));
    assert!(records["inquiry-map"].body.contains("**Developed eighth common foundation:** causal questions"));
    assert!(records["inquiry-source-causal"].body.contains("19 August 2026"));
    let operator_programs: Vec<_> = ["operator_study.py", "operator_checks.py"]
        .into_iter()
        .map(|filename| {
            let marker = format!("```python file={filename}\n");
            records["math-operator-study"]
                .body
                .split(&marker)
                .nth(1)
                .unwrap()
                .split("\n```")
                .next()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert!(operator_programs[0].contains("def exponential_partial("));
    assert!(operator_programs[1].contains("distance+uncertainty <= tail"));
    assert!(records["mathematics-map"].body.contains("**Developed common operator bridge:**"));
    assert!(records["math-source-operators"].body.contains("30 October 2026"));
    let quantum_programs: Vec<_> = ["quantum_study.py", "quantum_checks.py"]
        .into_iter()
        .map(|filename| {
            let marker = format!("```python file={filename}\n");
            records["physics-quantum-study"]
                .body
                .split(&marker)
                .nth(1)
                .unwrap()
                .split("\n```")
                .next()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert!(quantum_programs[0].contains("def dephase("));
    assert!(quantum_programs[1].contains("q.uncertainty([1, 0, 0]"));
    assert!(records["physics-map"].body.contains("**Developed common quantum foundations:**"));
    assert!(records["physics-source-quantum"].body.contains("17 September 2013"));
    let trust_programs: Vec<_> = ["trust_study.py", "trust_checks.py"]
        .into_iter()
        .map(|filename| {
            let marker = format!("```python file={filename}\n");
            records["computing-trust-study"]
                .body
                .split(&marker)
                .nth(1)
                .unwrap()
                .split("\n```")
                .next()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert!(trust_programs[0].contains("from communication_study import natural, receive"));
    assert!(trust_programs[1].contains("expected_histories"));
    assert!(records["computing-map"].body.contains("**Developed common human-use and trust foundations:**"));
    assert!(records["computing-source-accessibility"].body.contains("12 December 2024"));
    let operation_programs: Vec<_> = ["operation_study.py", "operation_checks.py"]
        .into_iter()
        .map(|filename| {
            let marker = format!("```python file={filename}\n");
            records["engineering-operation-study"]
                .body
                .split(&marker)
                .nth(1)
                .unwrap()
                .split("\n```")
                .next()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert!(operation_programs[0].contains("from production_study import integer"));
    assert!(operation_programs[1].contains("independent_minimal"));
    assert!(records["engineering-map"].body.contains("**Developed common people and operation foundations:**"));
    assert!(records["engineering-source-human-factors"].body.contains("Open Government Licence"));
    let field_programs: Vec<_> = ["field_study.py", "field_checks.py"]
        .into_iter()
        .map(|filename| {
            let marker = format!("```python file={filename}\n");
            records["math-fields-study"]
                .body
                .split(&marker)
                .nth(1)
                .unwrap()
                .split("\n```")
                .next()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert!(field_programs[0].contains("from multivariable_study import segment_integral, value"));
    assert!(field_programs[1].contains("result['boundary'], expected"));
    assert!(records["mathematics-map"].body.contains("**Developed common oriented-field foundations:**"));
    assert!(!selected.contains(&records["em-vector-book"].uid));
    let magnetic_programs: Vec<_> = ["magnetic_study.py", "magnetic_checks.py"]
        .into_iter()
        .map(|filename| {
            let marker = format!("```python file={filename}\n");
            records["physics-magnetic-study"]
                .body
                .split(&marker)
                .nth(1)
                .unwrap()
                .split("\n```")
                .next()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert!(magnetic_programs[0].contains("from electric_study import exact, decay_bracket"));
    assert!(magnetic_programs[1].contains("current_two_tau_bounds"));
    assert!(records["physics-map"].body.contains("**Developed common magnetic and inductive foundations:**"));
    assert!(!selected.contains(&records["em-tong"].uid));
    let spacetime_programs: Vec<_> = ["spacetime_study.py", "spacetime_checks.py"]
        .into_iter()
        .map(|filename| {
            let marker = format!("```python file={filename}\n");
            records["physics-spacetime-study"]
                .body
                .split(&marker)
                .nth(1)
                .unwrap()
                .split("\n```")
                .next()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert!(spacetime_programs[0].contains("from electric_study import exact, rational_root, potential_field"));
    assert!(spacetime_programs[1].contains("reported_minus_prediction_over_reported_uncertainty"));
    assert!(records["physics-map"].body.contains("**Developed common spacetime and gravity foundations:**"));
    for slug in ["physics-source-dynrel", "physics-source-optical-clocks", "physics-source-equivalence"] {
        assert!(!selected.contains(&records[slug].uid));
    }
    let astronomy_programs: Vec<_> = ["astronomy_study.py", "astronomy_checks.py"]
        .into_iter()
        .map(|filename| {
            let marker = format!("```python file={filename}\n");
            records["physics-astronomy-study"]
                .body
                .split(&marker)
                .nth(1)
                .unwrap()
                .split("\n```")
                .next()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert!(astronomy_programs[0].contains("from spacetime_study import orbit"));
    assert!(astronomy_programs[1].contains("historical_absolute_magnitude"));
    assert!(records["physics-map"].body.contains("**Developed common astronomical foundations:**"));
    for slug in ["physics-source-astronomy", "physics-source-gaia-distances", "physics-source-hubble1929"] {
        assert!(!selected.contains(&records[slug].uid));
    }
    let structure_slugs = [
        "math-structures",
        "math-structures-language",
        "math-structures-models",
        "math-structures-consequence",
        "math-structures-proof",
        "math-structures-operations",
        "math-structures-maps",
        "math-structures-quotients",
        "math-structures-evidence",
        "math-structures-study",
        "math-structures-review",
    ];
    let structure_edges: BTreeSet<_> = structure_slugs.windows(2)
        .map(|pair| (records[pair[0]].uid.clone(), records[pair[1]].uid.clone()))
        .collect();
    assert_eq!(learning_edges(&publication, "math-structures"), structure_edges);
    let mathematics_branches = BTreeSet::from([(
        records["mathematics-map"].uid.clone(), records["math-structures"].uid.clone(),
    )]);
    assert_eq!(learning_edges(&publication, "mathematics-branches"), mathematics_branches);
    for slug in structure_slugs.into_iter().chain(["math-source-forallx", "math-source-judson"]) {
        assert!(!selected.contains(&records[slug].uid));
    }
    let structure_programs: Vec<_> = ["structures_study.py", "structures_checks.py"]
        .into_iter().map(|filename| {
            let marker = format!("```python file={filename}\n");
            records["math-structures-study"].body.split(&marker).nth(1).unwrap()
                .split("\n```").next().unwrap().to_owned()
        }).collect();
    let original_sources = external_references(&publication);
    let contextual_links = |publication: &Publication| {
        let included: BTreeSet<_> = publication
            .documents
            .iter()
            .map(|document| document.base.as_str())
            .collect();
        let mut links = BTreeSet::new();
        for document in &publication.documents {
            let base = lince_resources::Url::parse(&document.base).unwrap();
            for record in &document.project.records {
                for target in record
                    .body
                    .split("](")
                    .skip(1)
                    .filter_map(|value| value.split_once(')').map(|(target, _)| target))
                {
                    let resolved = base.join(target).unwrap();
                    let mut location = resolved.clone();
                    location.set_fragment(None);
                    if !included.contains(location.as_str()) {
                        links.insert(resolved.to_string());
                    }
                }
            }
        }
        links
    };
    let original_context = contextual_links(&publication);
    assert!(!original_context.is_empty());
    let destination = temporary.path().join("export");
    let exported = publication.export(&library, &destination, false).unwrap();
    let entry = exported.strip_prefix(&destination).unwrap().to_owned();
    let moved = temporary.path().join("moved");
    fs::rename(&destination, &moved).unwrap();
    let fresh_library = Library::new(temporary.path().join("fresh-library"));
    let reopened = Publication::load(
        &fresh_library,
        moved.join(entry).to_str().unwrap(),
        &[],
        false,
    )
    .unwrap();
    assert_eq!(learning_edges(&reopened, "hephaestus"), expected_map);
    assert_eq!(learning_edges(&reopened, "hephaestus-core"), expected_core);
    assert_eq!(reopened.documents.len(), 3752);
    assert_eq!(learning_edges(&reopened, "math-structures"), structure_edges);
    assert_eq!(learning_edges(&reopened, "mathematics-branches"), mathematics_branches);
    let reopened_structure = reopened.documents.iter().flat_map(|d| &d.project.records)
        .find(|r| r.slug.as_deref() == Some("math-structures-study")).unwrap();
    for program in structure_programs {
        assert!(reopened_structure.body.contains(&program));
    }
    assert_eq!(
        learning_edges(&reopened, "quantitative-literacy"),
        original_quantitative
    );
    assert_eq!(contextual_links(&reopened), original_context);
    assert_eq!(external_references(&reopened), original_sources);
    let reopened_stock = reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .find(|record| record.slug.as_deref() == Some("computing-foundation-study"))
        .unwrap();
    assert!(reopened_stock.body.contains(&stock_program));
    let reopened_catalog = reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .find(|record| record.slug.as_deref() == Some("computing-collection-study"))
        .unwrap();
    for program in catalog_programs {
        assert!(reopened_catalog.body.contains(&program));
    }
    let reopened_allocation = reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .find(|record| record.slug.as_deref() == Some("inquiry-design-study"))
        .unwrap();
    for program in allocation_programs {
        assert!(reopened_allocation.body.contains(&program));
    }
    let reopened_change = reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .find(|record| record.slug.as_deref() == Some("math-change-study"))
        .unwrap();
    for program in change_programs {
        assert!(reopened_change.body.contains(&program));
    }
    let reopened_thermal = reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .find(|record| record.slug.as_deref() == Some("physics-thermal-study"))
        .unwrap();
    for program in thermal_programs {
        assert!(reopened_thermal.body.contains(&program));
    }
    let reopened_reaction = reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .find(|record| record.slug.as_deref() == Some("chemistry-dynamics-study"))
        .unwrap();
    for program in reaction_programs {
        assert!(reopened_reaction.body.contains(&program));
    }
    let reopened_machine = reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .find(|record| record.slug.as_deref() == Some("computing-machine-study"))
        .unwrap();
    for program in machine_programs {
        assert!(reopened_machine.body.contains(&program));
    }
    assert!(reopened.documents.iter().flat_map(|document| &document.project.records).any(
        |record| record.slug.as_deref() == Some("rc-floating")
            && record.body.contains(&floating_program)
    ));
    let reopened_network = reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .find(|record| record.slug.as_deref() == Some("engineering-network-study"))
        .unwrap();
    for program in network_programs {
        assert!(reopened_network.body.contains(&program));
    }
    let reopened_uncertainty = reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .find(|record| record.slug.as_deref() == Some("inquiry-uncertainty-study"))
        .unwrap();
    for program in uncertainty_programs {
        assert!(reopened_uncertainty.body.contains(&program));
    }
    let reopened_periodic = reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .find(|record| record.slug.as_deref() == Some("math-periodic-study"))
        .unwrap();
    for program in periodic_programs {
        assert!(reopened_periodic.body.contains(&program));
    }
    let reopened_waves = reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .find(|record| record.slug.as_deref() == Some("physics-wave-study"))
        .unwrap();
    for program in wave_programs {
        assert!(reopened_waves.body.contains(&program));
    }
    let reopened_solutions = reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .find(|record| record.slug.as_deref() == Some("chemistry-solution-study"))
        .unwrap();
    for program in solution_programs {
        assert!(reopened_solutions.body.contains(&program));
    }
    let reopened_memory = reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .find(|record| record.slug.as_deref() == Some("computing-memory-study"))
        .unwrap();
    for program in space_programs {
        assert!(reopened_memory.body.contains(&program));
    }
    let reopened_material = reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .find(|record| record.slug.as_deref() == Some("engineering-material-study"))
        .unwrap();
    for program in axial_programs {
        assert!(reopened_material.body.contains(&program));
    }
    let reopened_density = reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .find(|record| record.slug.as_deref() == Some("inquiry-density-study"))
        .unwrap();
    for program in density_programs {
        assert!(reopened_density.body.contains(&program));
    }
    let reopened_linear_space = reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .find(|record| record.slug.as_deref() == Some("math-linear-system-study"))
        .unwrap();
    for program in linear_space_programs {
        assert!(reopened_linear_space.body.contains(&program));
    }
    let reopened_electric = reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .find(|record| record.slug.as_deref() == Some("physics-electric-study"))
        .unwrap();
    for program in electric_programs {
        assert!(reopened_electric.body.contains(&program));
    }
    let reopened_redox = reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .find(|record| record.slug.as_deref() == Some("chemistry-electron-study"))
        .unwrap();
    for program in redox_programs {
        assert!(reopened_redox.body.contains(&program));
    }
    let reopened_durable = reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .find(|record| record.slug.as_deref() == Some("computing-durable-study"))
        .unwrap();
    for program in durable_programs {
        assert!(reopened_durable.body.contains(&program));
    }
    let reopened_realization = reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .find(|record| record.slug.as_deref() == Some("engineering-electric-study"))
        .unwrap();
    for program in realization_programs {
        assert!(reopened_realization.body.contains(&program));
    }
    let reopened_fitting = reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .find(|record| record.slug.as_deref() == Some("inquiry-fitting-study"))
        .unwrap();
    for program in fitting_programs {
        assert!(reopened_fitting.body.contains(&program));
    }
    let reopened_multivariable = reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .find(|record| record.slug.as_deref() == Some("math-multivariable-study"))
        .unwrap();
    for program in multivariable_programs {
        assert!(reopened_multivariable.body.contains(&program));
    }
    let reopened_entropy = reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .find(|record| record.slug.as_deref() == Some("physics-entropy-study"))
        .unwrap();
    for program in entropy_programs {
        assert!(reopened_entropy.body.contains(&program));
    }
    let reopened_free_energy = reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .find(|record| record.slug.as_deref() == Some("chemistry-free-energy-study"))
        .unwrap();
    for program in free_energy_programs {
        assert!(reopened_free_energy.body.contains(&program));
    }
    let reopened_concurrency = reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .find(|record| record.slug.as_deref() == Some("computing-concurrency-study"))
        .unwrap();
    for program in concurrency_programs {
        assert!(reopened_concurrency.body.contains(&program));
    }
    let reopened_feedback = reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .find(|record| record.slug.as_deref() == Some("engineering-feedback-study"))
        .unwrap();
    for program in feedback_programs {
        assert!(reopened_feedback.body.contains(&program));
    }
    let reopened_reporting = reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .find(|record| record.slug.as_deref() == Some("inquiry-reporting-study"))
        .unwrap();
    for program in reporting_programs {
        assert!(reopened_reporting.body.contains(&program));
    }
    let reopened_discrete = reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .find(|record| record.slug.as_deref() == Some("math-discrete-study"))
        .unwrap();
    for program in discrete_programs {
        assert!(reopened_discrete.body.contains(&program));
    }
    let reopened_rotation = reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .find(|record| record.slug.as_deref() == Some("physics-rotation-study"))
        .unwrap();
    for program in rotation_programs {
        assert!(reopened_rotation.body.contains(&program));
    }
    let reopened_identity = reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .find(|record| record.slug.as_deref() == Some("chemistry-identity-study"))
        .unwrap();
    for program in identity_programs {
        assert!(reopened_identity.body.contains(&program));
    }
    let reopened_communication = reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .find(|record| record.slug.as_deref() == Some("computing-communication-study"))
        .unwrap();
    for program in communication_programs {
        assert!(reopened_communication.body.contains(&program));
    }
    let reopened_production = reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .find(|record| record.slug.as_deref() == Some("engineering-production-study"))
        .unwrap();
    for program in production_programs {
        assert!(reopened_production.body.contains(&program));
    }
    for (slug, body) in learning_material {
        let restored = reopened
            .documents
            .iter()
            .flat_map(|document| &document.project.records)
            .find(|record| record.slug.as_deref() == Some(slug))
            .unwrap();
        assert_eq!(learning_body(&restored.body), body, "{slug}");
    }
    let reopened_series = reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .find(|record| record.slug.as_deref() == Some("math-series-study"))
        .unwrap();
    for program in series_programs {
        assert!(reopened_series.body.contains(&program));
    }
    let reopened_statistical = reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .find(|record| record.slug.as_deref() == Some("physics-statistical-study"))
        .unwrap();
    for program in statistical_programs {
        assert!(reopened_statistical.body.contains(&program));
    }
    let reopened_phase = reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .find(|record| record.slug.as_deref() == Some("chemistry-phase-study"))
        .unwrap();
    for program in phase_programs {
        assert!(reopened_phase.body.contains(&program));
    }
    let reopened_shared_phase = reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .find(|record| record.slug.as_deref() == Some("mat-regular-example"))
        .unwrap();
    assert!(reopened_shared_phase.body.contains(&shared_phase_program));
    let reopened_abstraction = reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .find(|record| record.slug.as_deref() == Some("computing-abstraction-study"))
        .unwrap();
    for program in abstraction_programs {
        assert!(reopened_abstraction.body.contains(&program));
    }
    let reopened_service = reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .find(|record| record.slug.as_deref() == Some("engineering-service-study"))
        .unwrap();
    for program in service_programs {
        assert!(reopened_service.body.contains(&program));
    }
    let reopened_causal = reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .find(|record| record.slug.as_deref() == Some("inquiry-causal-study"))
        .unwrap();
    for program in causal_programs {
        assert!(reopened_causal.body.contains(&program));
    }
    let reopened_operators = reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .find(|record| record.slug.as_deref() == Some("math-operator-study"))
        .unwrap();
    for program in operator_programs {
        assert!(reopened_operators.body.contains(&program));
    }
    let reopened_quantum = reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .find(|record| record.slug.as_deref() == Some("physics-quantum-study"))
        .unwrap();
    for program in quantum_programs {
        assert!(reopened_quantum.body.contains(&program));
    }
    let reopened_trust = reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .find(|record| record.slug.as_deref() == Some("computing-trust-study"))
        .unwrap();
    for program in trust_programs {
        assert!(reopened_trust.body.contains(&program));
    }
    let reopened_operation = reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .find(|record| record.slug.as_deref() == Some("engineering-operation-study"))
        .unwrap();
    for program in operation_programs {
        assert!(reopened_operation.body.contains(&program));
    }
    let reopened_fields = reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .find(|record| record.slug.as_deref() == Some("math-fields-study"))
        .unwrap();
    for program in field_programs {
        assert!(reopened_fields.body.contains(&program));
    }
    let reopened_magnetic = reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .find(|record| record.slug.as_deref() == Some("physics-magnetic-study"))
        .unwrap();
    for program in magnetic_programs {
        assert!(reopened_magnetic.body.contains(&program));
    }
    let reopened_spacetime = reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .find(|record| record.slug.as_deref() == Some("physics-spacetime-study"))
        .unwrap();
    for program in spacetime_programs {
        assert!(reopened_spacetime.body.contains(&program));
    }
    let reopened_astronomy = reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .find(|record| record.slug.as_deref() == Some("physics-astronomy-study"))
        .unwrap();
    for program in astronomy_programs {
        assert!(reopened_astronomy.body.contains(&program));
    }
    for (predicate, original) in shared_routes {
        assert_eq!(learning_edges(&reopened, predicate), original);
    }
    let locations: BTreeMap<_, _> = reopened
        .documents
        .iter()
        .flat_map(|document| {
            document
                .project
                .records
                .iter()
                .map(|record| (record.uid.as_str(), document.base.as_str()))
        })
        .collect();
    for record in reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
    {
        for assertion in &record.assertions {
            if ["contains", "math-structures", "mathematics-branches", "hephaestus", "hephaestus-core", "quantitative-literacy", "mechanical-engineering", "fluid-dynamics", "general-chemistry", "reproducible-computation", "metrology", "electrical-power-engineering", "water-and-sanitation"]
                .contains(&assertion.predicate.as_str())
            {
                let (location, uid) = assertion
                    .object_uri
                    .as_ref()
                    .unwrap()
                    .rsplit_once('#')
                    .unwrap();
                assert_eq!(location, locations[uid]);
            }
        }
    }
    assert!(reopened.resources().unwrap().is_empty());
}

#[test]
fn mining_preserves_shared_routes_and_three_original_examples_after_relocation() {
    let trails = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../institute/trails");
    let directory = trails.join("ergon/hephaestus/engineering/mining-and-mineral-engineering");
    let expected_documents = [
        directory.clone(),
        trails.join("ergon/gaia/earth/mineralogy-and-petrology"),
        trails.join("ergon/gaia/earth/geology"),
        trails.join("ergon/hephaestus/inquiry/quantitative-literacy"),
        trails.join("ergon/hephaestus/inquiry/metrology"),
        trails.join("ergon/hephaestus/inquiry/reproducible-computation"),
        trails.join("ergon/hephaestus/chemistry/general-chemistry"),
    ]
    .iter()
    .map(|path| Publication::files(path).unwrap().len())
    .sum::<usize>();
    let temporary = tempfile::tempdir().unwrap();
    let library = Library::new(temporary.path().join("library"));
    let audit = Publication::audit(&library, &directory, &[]).unwrap();
    assert!(audit.issues.is_empty(), "{:?}", audit.issues);
    assert!(
        !audit
            .unchecked
            .iter()
            .any(|issue| issue.contains("directed navigation path")),
        "{:?}",
        audit.unchecked
    );
    for entry in [
        "Mining-and-mineral-engineering.lingua",
        "planning/An-exhaustively-checkable-small-mine-schedule.lingua",
        "processing/Flotation-kinetics-residence-time-and-parameter-evidence.lingua",
        "research/Advanced-waste-water-tailings-and-closure-study.lingua",
        "sources/Tratamento-de-Minerios-CETEM.lingua",
    ] {
        let publication = Publication::load(
            &library,
            directory.join(entry).to_str().unwrap(),
            &[],
            false,
        )
        .unwrap();
        assert_eq!(publication.documents.len(), expected_documents, "{entry}");
        assert!(publication.parameters.is_empty());
        assert!(publication.instances.is_empty());
        assert!(publication.resources().unwrap().is_empty());
        assert!(publication.documents.iter().all(|document| {
            document.project.frequencies.is_empty() && document.project.rules.is_empty()
        }));
    }
    let publication = Publication::load(
        &library,
        directory
            .join("Mining-and-mineral-engineering.lingua")
            .to_str()
            .unwrap(),
        &[],
        false,
    )
    .unwrap();
    let records: BTreeMap<_, _> = publication
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .map(|record| (record.slug.as_deref().unwrap(), record))
        .collect();
    assert_eq!(
        records["mining-and-mineral-engineering"].uid,
        "r_22HJPS3RTK5EX1J55YJP75PHYA"
    );
    let predicates = [
        "quantitative-literacy",
        "metrology",
        "reproducible-computation",
        "general-chemistry",
        "geology",
        "mineralogy-and-petrology",
        "mining-and-mineral-engineering",
    ];
    let routes: BTreeMap<_, _> = predicates
        .into_iter()
        .map(|predicate| (predicate, learning_edges(&publication, predicate)))
        .collect();
    let edges = &routes["mining-and-mineral-engineering"];
    let mut expected: BTreeSet<_> = publication
        .documents
        .iter()
        .filter(|document| {
            document
                .uri
                .contains("/ergon/hephaestus/engineering/mining-and-mineral-engineering/")
        })
        .filter(|document| !document.uri.contains("/sources/"))
        .flat_map(|document| &document.project.records)
        .filter(|record| record.slug.as_deref() != Some("mine-sources"))
        .map(|record| record.uid.clone())
        .collect();
    for (shared, continuation, original) in [
        ("geo-maps", "mine-survey", "geology"),
        ("met-sampling-target", "mine-sampling", "metrology"),
        (
            "mp-liberation",
            "mine-geometallurgy",
            "mineralogy-and-petrology",
        ),
        (
            "rc-numericaltests",
            "mine-schedule-example",
            "reproducible-computation",
        ),
        ("geo-stress-strain", "mine-rock-mass", "geology"),
        ("geo-groundwater-flow", "mine-dewatering", "geology"),
        ("ql-balances", "mine-flowsheet", "quantitative-literacy"),
        (
            "mp-tailings",
            "mine-waste-geochemistry",
            "mineralogy-and-petrology",
        ),
    ] {
        let uid = records[shared].uid.clone();
        expected.insert(uid.clone());
        assert!(edges.contains(&(uid.clone(), records[continuation].uid.clone())));
        assert!(routes[original].iter().any(|(from, _)| *from == uid));
        assert!(
            records[shared]
                .body
                .contains("Continue in mining and mineral engineering:")
        );
    }
    let selected: BTreeSet<_> = edges
        .iter()
        .flat_map(|(from, to)| [from.clone(), to.clone()])
        .collect();
    assert_eq!(selected, expected);
    let mut reached = BTreeSet::new();
    let mut pending = vec![records["mining-and-mineral-engineering"].uid.clone()];
    while let Some(uid) = pending.pop() {
        if reached.insert(uid.clone()) {
            pending.extend(
                edges
                    .iter()
                    .filter(|(from, _)| *from == uid)
                    .map(|(_, to)| to.clone()),
            );
        }
    }
    assert_eq!(reached, selected);
    assert_eq!(selected.len(), 116);
    for excluded in [
        "quantitative-literacy",
        "metrology",
        "reproducible-computation",
        "general-chemistry",
        "geology",
        "mineralogy-and-petrology",
        "mine-sources",
        "mine-psu",
        "mine-haul-road-manual",
    ] {
        assert!(!selected.contains(&records[excluded].uid));
    }
    for (predicate, route) in &routes {
        if *predicate != "mining-and-mineral-engineering" {
            assert!(edges.is_disjoint(route));
        }
    }
    let examples = |publication: &Publication| {
        publication
            .documents
            .iter()
            .flat_map(|document| &document.project.records)
            .filter_map(|record| {
                let file = match record.slug.as_deref()? {
                    "mine-schedule-example" => "mine_schedule.py",
                    "mine-network-example" => "ventilation_network.py",
                    "mine-balance-example" => "two_products.py",
                    _ => return None,
                };
                Some((
                    file.to_owned(),
                    record
                        .body
                        .split(&format!("```python file={file}\n"))
                        .nth(1)
                        .unwrap()
                        .split("\n```")
                        .next()
                        .unwrap()
                        .to_owned(),
                ))
            })
            .collect::<BTreeMap<_, _>>()
    };
    let original_examples = examples(&publication);
    assert_eq!(original_examples.len(), 3);
    let references = external_references(&publication);
    assert!(!references.is_empty());
    assert!(!directory.join("books").exists());
    assert!(!directory.join("views").exists());
    let destination = temporary.path().join("export");
    let exported = publication.export(&library, &destination, false).unwrap();
    let entry = exported.strip_prefix(&destination).unwrap().to_owned();
    let moved = temporary.path().join("moved");
    fs::rename(destination, &moved).unwrap();
    let relocated_library = Library::new(temporary.path().join("relocated-library"));
    let reopened = Publication::load(
        &relocated_library,
        moved.join(entry).to_str().unwrap(),
        &[],
        false,
    )
    .unwrap();
    assert_eq!(reopened.documents.len(), expected_documents);
    assert_eq!(external_references(&reopened), references);
    assert_eq!(examples(&reopened), original_examples);
    for (predicate, original) in routes {
        assert_eq!(learning_edges(&reopened, predicate), original);
    }
    let locations: BTreeMap<_, _> = reopened
        .documents
        .iter()
        .flat_map(|document| {
            document
                .project
                .records
                .iter()
                .map(|record| (record.uid.as_str(), document.base.as_str()))
        })
        .collect();
    for record in reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
    {
        for assertion in &record.assertions {
            if predicates.contains(&assertion.predicate.as_str()) {
                let (location, uid) = assertion
                    .object_uri
                    .as_ref()
                    .unwrap()
                    .rsplit_once('#')
                    .unwrap();
                assert_eq!(location, locations[uid]);
            }
        }
    }
    assert!(!moved.join("files").exists());
}

#[test]
fn mineralogy_petrology_preserves_shared_learning_and_two_original_examples_after_relocation() {
    let trails = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../institute/trails");
    let directory = trails.join("ergon/gaia/earth/mineralogy-and-petrology");
    let expected_documents = [
        directory.clone(),
        trails.join("ergon/gaia/earth/geology"),
        trails.join("ergon/hephaestus/inquiry/quantitative-literacy"),
        trails.join("ergon/hephaestus/inquiry/metrology"),
        trails.join("ergon/hephaestus/inquiry/reproducible-computation"),
        trails.join("ergon/hephaestus/chemistry/general-chemistry"),
    ]
    .iter()
    .map(|path| Publication::files(path).unwrap().len())
    .sum::<usize>();
    let temporary = tempfile::tempdir().unwrap();
    let library = Library::new(temporary.path().join("library"));
    let audit = Publication::audit(&library, &directory, &[]).unwrap();
    assert!(audit.issues.is_empty(), "{:?}", audit.issues);
    assert!(
        !audit
            .unchecked
            .iter()
            .any(|issue| issue.contains("directed navigation path")),
        "{:?}",
        audit.unchecked
    );
    for entry in [
        "Mineralogy-and-petrology.lingua",
        "crystals/Space-groups-settings-and-structural-models.lingua",
        "analysis/An-inspectable-oxide-normalization.lingua",
        "research/Advanced-metamorphic-model-and-petrochronology-study.lingua",
        "sources/Guide-to-Thin-Section-Microscopy.lingua",
    ] {
        let publication = Publication::load(
            &library,
            directory.join(entry).to_str().unwrap(),
            &[],
            false,
        )
        .unwrap();
        assert_eq!(publication.documents.len(), expected_documents, "{entry}");
        assert!(publication.parameters.is_empty());
        assert!(publication.instances.is_empty());
        assert!(publication.resources().unwrap().is_empty());
        assert!(publication.documents.iter().all(|document| {
            document.project.frequencies.is_empty() && document.project.rules.is_empty()
        }));
    }
    let publication = Publication::load(
        &library,
        directory
            .join("Mineralogy-and-petrology.lingua")
            .to_str()
            .unwrap(),
        &[],
        false,
    )
    .unwrap();
    let records: BTreeMap<_, _> = publication
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .map(|record| (record.slug.as_deref().unwrap(), record))
        .collect();
    assert_eq!(
        records["mineralogy-and-petrology"].uid,
        "r_44F7X6GPMW6ZSCCN6D71QSVKBN"
    );
    let predicates = [
        "quantitative-literacy",
        "metrology",
        "reproducible-computation",
        "general-chemistry",
        "geology",
        "mineralogy-and-petrology",
    ];
    let routes: BTreeMap<_, _> = predicates
        .into_iter()
        .map(|predicate| (predicate, learning_edges(&publication, predicate)))
        .collect();
    let edges = &routes["mineralogy-and-petrology"];
    let mut expected: BTreeSet<_> = publication
        .documents
        .iter()
        .filter(|document| {
            document
                .uri
                .contains("/ergon/gaia/earth/mineralogy-and-petrology/")
        })
        .filter(|document| !document.uri.contains("/sources/"))
        .flat_map(|document| &document.project.records)
        .filter(|record| record.slug.as_deref() != Some("mp-sources"))
        .map(|record| record.uid.clone())
        .collect();
    for (shared, continuation, original) in [
        ("geo-mineral-id", "mp-definition", "geology"),
        ("gc-crystals", "mp-lattice", "general-chemistry"),
        ("geo-rock-texture", "mp-modal", "geology"),
        ("met-chemistry-case", "mp-microprobe", "metrology"),
        ("gc-phases", "mp-phase-rule", "general-chemistry"),
        (
            "rc-numericaltests",
            "mp-equilibrium-example",
            "reproducible-computation",
        ),
        ("geo-igneous", "mp-magma", "geology"),
        ("geo-metamorphic", "mp-assemblages", "geology"),
    ] {
        let uid = records[shared].uid.clone();
        expected.insert(uid.clone());
        assert!(edges.contains(&(uid.clone(), records[continuation].uid.clone())));
        assert!(routes[original].iter().any(|(from, _)| *from == uid));
        assert!(
            records[shared]
                .body
                .contains("Continue in mineralogy and petrology:")
        );
    }
    let selected: BTreeSet<_> = edges
        .iter()
        .flat_map(|(from, to)| [from.clone(), to.clone()])
        .collect();
    assert_eq!(selected, expected);
    let mut reached = BTreeSet::new();
    let mut pending = vec![records["mineralogy-and-petrology"].uid.clone()];
    while let Some(uid) = pending.pop() {
        if reached.insert(uid.clone()) {
            pending.extend(
                edges
                    .iter()
                    .filter(|(from, _)| *from == uid)
                    .map(|(_, to)| to.clone()),
            );
        }
    }
    assert_eq!(reached, selected);
    assert_eq!(selected.len(), 97);
    for excluded in [
        "quantitative-literacy",
        "metrology",
        "reproducible-computation",
        "general-chemistry",
        "geology",
        "mp-sources",
        "mp-mineralogy",
    ] {
        assert!(!selected.contains(&records[excluded].uid));
    }
    for (predicate, route) in &routes {
        if *predicate != "mineralogy-and-petrology" {
            assert!(edges.is_disjoint(route));
        }
    }
    let examples = |publication: &Publication| {
        publication
            .documents
            .iter()
            .flat_map(|document| &document.project.records)
            .filter_map(|record| {
                let slug = record.slug.as_deref()?;
                let file = match slug {
                    "mp-formula-example" => "mineral_formula.py",
                    "mp-equilibrium-example" => "two_phase.py",
                    _ => return None,
                };
                Some((
                    file.to_owned(),
                    record
                        .body
                        .split(&format!("```python file={file}\n"))
                        .nth(1)
                        .unwrap()
                        .split("\n```")
                        .next()
                        .unwrap()
                        .to_owned(),
                ))
            })
            .collect::<BTreeMap<_, _>>()
    };
    let original_examples = examples(&publication);
    assert_eq!(original_examples.len(), 2);
    assert!(
        original_examples["mineral_formula.py"].contains("def normalize(oxides, oxygen_basis):")
    );
    assert!(
        original_examples["two_phase.py"].contains("def equilibrate(bulk, offset=math.log(4)):")
    );
    let references = external_references(&publication);
    assert!(!references.is_empty());
    assert!(!directory.join("books").exists());
    assert!(!directory.join("views").exists());
    let destination = temporary.path().join("export");
    let exported = publication.export(&library, &destination, false).unwrap();
    let entry = exported.strip_prefix(&destination).unwrap().to_owned();
    let moved = temporary.path().join("moved");
    fs::rename(destination, &moved).unwrap();
    let relocated_library = Library::new(temporary.path().join("relocated-library"));
    let reopened = Publication::load(
        &relocated_library,
        moved.join(entry).to_str().unwrap(),
        &[],
        false,
    )
    .unwrap();
    assert_eq!(reopened.documents.len(), expected_documents);
    assert_eq!(external_references(&reopened), references);
    assert_eq!(examples(&reopened), original_examples);
    for (predicate, original) in routes {
        assert_eq!(learning_edges(&reopened, predicate), original);
    }
    let locations: BTreeMap<_, _> = reopened
        .documents
        .iter()
        .flat_map(|document| {
            document
                .project
                .records
                .iter()
                .map(|record| (record.uid.as_str(), document.base.as_str()))
        })
        .collect();
    for record in reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
    {
        for assertion in &record.assertions {
            if predicates.contains(&assertion.predicate.as_str()) {
                let (location, uid) = assertion
                    .object_uri
                    .as_ref()
                    .unwrap()
                    .rsplit_once('#')
                    .unwrap();
                assert_eq!(location, locations[uid]);
            }
        }
    }
    assert!(!moved.join("files").exists());
}

#[test]
fn quantitative_readings_discover_relocate_and_select_without_absorbing_sources() {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../institute/trails/ergon/hephaestus/inquiry/quantitative-literacy");
    let temporary = tempfile::tempdir().unwrap();
    let library = Library::new(temporary.path().join("library"));
    let files = Publication::files(&directory).unwrap();
    let audit = Publication::audit(&library, &directory, &[]).unwrap();
    assert_eq!(audit.documents, files.len());
    assert!(audit.issues.is_empty(), "{:?}", audit.issues);
    assert!(
        !audit
            .unchecked
            .iter()
            .any(|issue| issue.contains("directed navigation path")),
        "{:?}",
        audit.unchecked
    );
    for entry in [
        "Quantitative-literacy.lingua",
        "foundations/Units-and-dimensions.lingua",
        "models/Dimensionless-groups.lingua",
        "study/Advanced-quantitative-study.lingua",
        "sources/SI-Brochure.lingua",
    ] {
        let publication = Publication::load(
            &library,
            directory.join(entry).to_str().unwrap(),
            &[],
            false,
        )
        .unwrap();
        assert_eq!(publication.documents.len(), files.len(), "{entry}");
        assert!(publication.parameters.is_empty());
        assert!(publication.instances.is_empty());
        assert!(publication.documents.iter().all(|document| {
            document.project.frequencies.is_empty() && document.project.rules.is_empty()
        }));
    }
    let publication = Publication::load(
        &library,
        directory
            .join("Quantitative-literacy.lingua")
            .to_str()
            .unwrap(),
        &[],
        false,
    )
    .unwrap();
    let records: BTreeMap<_, _> = publication
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .map(|record| (record.uid.clone(), record))
        .collect();
    let mut expected = BTreeSet::new();
    let mut edges = BTreeMap::<String, BTreeSet<String>>::new();
    for document in &publication.documents {
        let path = PathBuf::from(document.uri.strip_prefix("file://").unwrap());
        if path.parent().unwrap().file_name().unwrap() != "sources" {
            for record in &document.project.records {
                if record.slug.as_deref() != Some("ql-sources") {
                    expected.insert(record.uid.clone());
                }
            }
        }
        for record in &document.project.records {
            for assertion in &record.assertions {
                if assertion.predicate == "quantitative-literacy" {
                    let target = assertion
                        .object_uri
                        .as_ref()
                        .unwrap()
                        .rsplit_once('#')
                        .unwrap()
                        .1;
                    assert!(records.contains_key(target), "{target}");
                    edges
                        .entry(record.uid.clone())
                        .or_default()
                        .insert(target.to_owned());
                }
            }
        }
    }
    let selected: BTreeSet<_> = edges
        .iter()
        .flat_map(|(from, to)| std::iter::once(from.clone()).chain(to.iter().cloned()))
        .collect();
    assert_eq!(selected, expected);
    let mut reached = BTreeSet::new();
    let mut pending = vec!["r_4XVN56PQC13JGNFPV0CSGXDCZ2".to_owned()];
    while let Some(uid) = pending.pop() {
        if reached.insert(uid.clone()) {
            if let Some(next) = edges.get(&uid) {
                pending.extend(next.iter().cloned());
            }
        }
    }
    assert_eq!(reached, selected);
    let external_links: BTreeSet<_> = records
        .values()
        .flat_map(|record| {
            record
                .body
                .split("](https://")
                .skip(1)
                .filter_map(|value| value.split_once(')').map(|(url, _)| url.to_owned()))
        })
        .collect();
    assert!(!external_links.is_empty());
    assert!(!directory.join("books").exists());
    assert!(!directory.join("views").exists());
    let destination = temporary.path().join("export");
    let exported = publication.export(&library, &destination, false).unwrap();
    let entry = exported.strip_prefix(&destination).unwrap().to_owned();
    let moved = temporary.path().join("relocated");
    fs::rename(destination, &moved).unwrap();
    let relocated_library = Library::new(temporary.path().join("relocated-library"));
    let reopened = Publication::load(
        &relocated_library,
        moved.join(entry).to_str().unwrap(),
        &[],
        false,
    )
    .unwrap();
    let relocated_identities: BTreeSet<_> = reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .map(|record| record.uid.clone())
        .collect();
    assert_eq!(relocated_identities, records.keys().cloned().collect());
    let relocated_links: BTreeSet<_> = reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .flat_map(|record| {
            record
                .body
                .split("](https://")
                .skip(1)
                .filter_map(|value| value.split_once(')').map(|(url, _)| url.to_owned()))
        })
        .collect();
    assert_eq!(relocated_links, external_links);
    assert!(!moved.join("files").exists());
}

#[test]
fn geology_reuses_foundations_and_preserves_its_selection_and_geometry_after_relocation() {
    let trails = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../institute/trails");
    let directory = trails.join("ergon/gaia/earth/geology");
    let expected_documents = [
        directory.clone(),
        trails.join("ergon/hephaestus/inquiry/quantitative-literacy"),
        trails.join("ergon/hephaestus/inquiry/metrology"),
        trails.join("ergon/hephaestus/inquiry/reproducible-computation"),
        trails.join("ergon/hephaestus/chemistry/general-chemistry"),
    ]
    .iter()
    .map(|path| Publication::files(path).unwrap().len())
    .sum::<usize>();
    let temporary = tempfile::tempdir().unwrap();
    let library = Library::new(temporary.path().join("library"));
    let audit = Publication::audit(&library, &directory, &[]).unwrap();
    assert!(audit.issues.is_empty(), "{:?}", audit.issues);
    assert!(
        !audit
            .unchecked
            .iter()
            .any(|issue| issue.contains("directed navigation path")),
        "{:?}",
        audit.unchecked
    );
    for entry in [
        "Geology.lingua",
        "field/Field-records-and-specimen-identity.lingua",
        "structures/An-inspectable-three-point-plane.lingua",
        "research/Independent-geological-scholarship.lingua",
        "sources/Geodiversidade-do-Brasil.lingua",
    ] {
        let publication = Publication::load(
            &library,
            directory.join(entry).to_str().unwrap(),
            &[],
            false,
        )
        .unwrap();
        assert_eq!(publication.documents.len(), expected_documents, "{entry}");
        assert!(publication.parameters.is_empty());
        assert!(publication.instances.is_empty());
        assert!(publication.resources().unwrap().is_empty());
        assert!(publication.documents.iter().all(|document| {
            document.project.frequencies.is_empty() && document.project.rules.is_empty()
        }));
    }
    let publication = Publication::load(
        &library,
        directory.join("Geology.lingua").to_str().unwrap(),
        &[],
        false,
    )
    .unwrap();
    let records: BTreeMap<_, _> = publication
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .map(|record| (record.slug.as_deref().unwrap(), record))
        .collect();
    assert_eq!(records["geology"].uid, "r_3CK43VQS79NK0STWNPSCS8YM06");
    let predicates = [
        "quantitative-literacy",
        "metrology",
        "reproducible-computation",
        "general-chemistry",
        "geology",
    ];
    let routes: BTreeMap<_, _> = predicates
        .into_iter()
        .map(|predicate| (predicate, learning_edges(&publication, predicate)))
        .collect();
    let edges = &routes["geology"];
    let mut expected: BTreeSet<_> = publication
        .documents
        .iter()
        .filter(|document| document.uri.contains("/ergon/gaia/earth/geology/"))
        .filter(|document| !document.uri.contains("/sources/"))
        .flat_map(|document| &document.project.records)
        .filter(|record| record.slug.as_deref() != Some("geo-sources"))
        .map(|record| record.uid.clone())
        .collect();
    for (shared, continuation, original) in [
        ("ql-graphs", "geo-maps", "quantitative-literacy"),
        ("gc-crystals", "geo-mineral-id", "general-chemistry"),
        ("met-sampling-target", "geo-sampling", "metrology"),
        ("gc-nuclear", "geo-age", "general-chemistry"),
        (
            "rc-numericaltests",
            "geo-plane-example",
            "reproducible-computation",
        ),
    ] {
        let uid = records[shared].uid.clone();
        expected.insert(uid.clone());
        assert!(edges.contains(&(uid.clone(), records[continuation].uid.clone())));
        assert!(routes[original].iter().any(|(from, _)| *from == uid));
        assert!(records[shared].body.contains("Continue in geology:"));
    }
    let selected: BTreeSet<_> = edges
        .iter()
        .flat_map(|(from, to)| [from.clone(), to.clone()])
        .collect();
    assert_eq!(selected, expected);
    let mut reached = BTreeSet::new();
    let mut pending = vec![records["geology"].uid.clone()];
    while let Some(uid) = pending.pop() {
        if reached.insert(uid.clone()) {
            pending.extend(
                edges
                    .iter()
                    .filter(|(from, _)| *from == uid)
                    .map(|(_, to)| to.clone()),
            );
        }
    }
    assert_eq!(reached, selected);
    assert_eq!(selected.len(), 80);
    for excluded in [
        "quantitative-literacy",
        "metrology",
        "reproducible-computation",
        "general-chemistry",
        "geo-sources",
        "geo-physical",
    ] {
        assert!(!selected.contains(&records[excluded].uid));
    }
    for (predicate, route) in &routes {
        if *predicate != "geology" {
            assert!(edges.is_disjoint(route));
        }
    }
    let code = |publication: &Publication| {
        publication
            .documents
            .iter()
            .flat_map(|document| &document.project.records)
            .find(|record| record.slug.as_deref() == Some("geo-plane-example"))
            .unwrap()
            .body
            .split("```python file=geological_plane.py\n")
            .nth(1)
            .unwrap()
            .split("\n```")
            .next()
            .unwrap()
            .to_owned()
    };
    let original_code = code(&publication);
    assert!(original_code.contains("def fit_plane(points):"));
    let references = external_references(&publication);
    assert!(!references.is_empty());
    assert!(!directory.join("books").exists());
    assert!(!directory.join("views").exists());
    let destination = temporary.path().join("export");
    let exported = publication.export(&library, &destination, false).unwrap();
    let entry = exported.strip_prefix(&destination).unwrap().to_owned();
    let moved = temporary.path().join("moved");
    fs::rename(destination, &moved).unwrap();
    let relocated_library = Library::new(temporary.path().join("relocated-library"));
    let reopened = Publication::load(
        &relocated_library,
        moved.join(entry).to_str().unwrap(),
        &[],
        false,
    )
    .unwrap();
    assert_eq!(reopened.documents.len(), expected_documents);
    assert_eq!(external_references(&reopened), references);
    assert_eq!(code(&reopened), original_code);
    for (predicate, original) in routes {
        assert_eq!(learning_edges(&reopened, predicate), original);
    }
    let locations: BTreeMap<_, _> = reopened
        .documents
        .iter()
        .flat_map(|document| {
            document
                .project
                .records
                .iter()
                .map(|record| (record.uid.as_str(), document.base.as_str()))
        })
        .collect();
    for record in reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
    {
        for assertion in &record.assertions {
            if predicates.contains(&assertion.predicate.as_str()) {
                let (location, uid) = assertion
                    .object_uri
                    .as_ref()
                    .unwrap()
                    .rsplit_once('#')
                    .unwrap();
                assert_eq!(location, locations[uid]);
            }
        }
    }
    assert!(!moved.join("files").exists());
}

fn learning_edges(publication: &Publication, predicate: &str) -> BTreeSet<(String, String)> {
    publication
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .flat_map(|record| {
            record
                .assertions
                .iter()
                .filter(|assertion| assertion.predicate == predicate)
                .map(|assertion| {
                    (
                        record.uid.clone(),
                        assertion
                            .object_uri
                            .as_ref()
                            .unwrap()
                            .rsplit_once('#')
                            .unwrap()
                            .1
                            .to_owned(),
                    )
                })
        })
        .collect()
}

fn external_references(publication: &Publication) -> BTreeSet<String> {
    publication
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .flat_map(|record| {
            record
                .body
                .split("](https://")
                .skip(1)
                .filter_map(|value| value.split_once(')').map(|(url, _)| url.to_owned()))
        })
        .collect()
}

#[test]
fn metrology_reuses_shared_steps_with_its_own_continuations_and_relocates_them() {
    let inquiry = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../institute/trails/ergon/hephaestus/inquiry");
    let directory = inquiry.join("metrology");
    let quantitative = inquiry.join("quantitative-literacy");
    let temporary = tempfile::tempdir().unwrap();
    let library = Library::new(temporary.path().join("library"));
    let expected_documents = Publication::files(&directory).unwrap().len()
        + Publication::files(&quantitative).unwrap().len();
    let audit = Publication::audit(&library, &directory, &[]).unwrap();
    assert!(audit.issues.is_empty(), "{:?}", audit.issues);
    assert!(
        !audit
            .unchecked
            .iter()
            .any(|issue| issue.contains("directed navigation path")),
        "{:?}",
        audit.unchecked
    );
    for entry in [
        "Metrology.lingua",
        "foundations/Question-and-measurand.lingua",
        "uncertainty/Multivariate-and-constraints.lingua",
        "research/Independent-research.lingua",
        "sources/GUM-and-Amendment.lingua",
    ] {
        let publication = Publication::load(
            &library,
            directory.join(entry).to_str().unwrap(),
            &[],
            false,
        )
        .unwrap();
        assert_eq!(publication.documents.len(), expected_documents, "{entry}");
        assert!(publication.parameters.is_empty());
        assert!(publication.instances.is_empty());
        assert!(publication.resources().unwrap().is_empty());
        assert!(publication.documents.iter().all(|document| {
            document.project.frequencies.is_empty() && document.project.rules.is_empty()
        }));
    }
    let publication = Publication::load(
        &library,
        directory.join("Metrology.lingua").to_str().unwrap(),
        &[],
        false,
    )
    .unwrap();
    let records: BTreeMap<_, _> = publication
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .map(|record| (record.slug.as_deref().unwrap(), record))
        .collect();
    let reused = [
        ("ql-quantity", "met-measurand"),
        ("ql-units", "met-si-realizations"),
        ("ql-repeated-observations", "met-rr"),
        ("ql-small-changes", "met-sensitivity"),
    ];
    let mut expected: BTreeSet<_> = publication
        .documents
        .iter()
        .filter(|document| {
            document
                .uri
                .contains("/ergon/hephaestus/inquiry/metrology/")
        })
        .filter(|document| !document.uri.contains("/sources/"))
        .flat_map(|document| &document.project.records)
        .filter(|record| record.slug.as_deref() != Some("met-sources"))
        .map(|record| record.uid.clone())
        .collect();
    let edges = learning_edges(&publication, "metrology");
    let quantitative_edges = learning_edges(&publication, "quantitative-literacy");
    for (shared, continuation) in reused {
        let uid = records[shared].uid.clone();
        expected.insert(uid.clone());
        assert!(edges.contains(&(uid.clone(), records[continuation].uid.clone())));
        assert!(quantitative_edges.iter().any(|(from, _)| *from == uid));
        assert!(edges.is_disjoint(&quantitative_edges));
    }
    let selected: BTreeSet<_> = edges
        .iter()
        .flat_map(|(from, to)| [from.clone(), to.clone()])
        .collect();
    assert_eq!(selected, expected);
    let mut reached = BTreeSet::new();
    let mut pending = vec![records["metrology"].uid.clone()];
    while let Some(uid) = pending.pop() {
        if reached.insert(uid.clone()) {
            pending.extend(
                edges
                    .iter()
                    .filter(|(from, _)| *from == uid)
                    .map(|(_, to)| to.clone()),
            );
        }
    }
    assert_eq!(reached, selected);
    assert!(!selected.contains(&records["quantitative-literacy"].uid));
    assert!(!selected.contains(&records["met-gum"].uid));
    let references = external_references(&publication);
    assert!(!references.is_empty());
    assert!(!directory.join("books").exists());
    assert!(!directory.join("views").exists());
    let destination = temporary.path().join("export");
    let exported = publication.export(&library, &destination, false).unwrap();
    let entry = exported.strip_prefix(&destination).unwrap().to_owned();
    let moved = temporary.path().join("moved");
    fs::rename(destination, &moved).unwrap();
    let relocated_library = Library::new(temporary.path().join("relocated-library"));
    let reopened = Publication::load(
        &relocated_library,
        moved.join(entry).to_str().unwrap(),
        &[],
        false,
    )
    .unwrap();
    assert_eq!(reopened.documents.len(), expected_documents);
    assert_eq!(external_references(&reopened), references);
    assert_eq!(learning_edges(&reopened, "metrology"), edges);
    assert_eq!(
        learning_edges(&reopened, "quantitative-literacy"),
        quantitative_edges
    );
    let locations: BTreeMap<_, _> = reopened
        .documents
        .iter()
        .flat_map(|document| {
            document
                .project
                .records
                .iter()
                .map(|record| (record.uid.as_str(), document.base.as_str()))
        })
        .collect();
    for record in reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
    {
        for assertion in &record.assertions {
            if matches!(
                assertion.predicate.as_str(),
                "metrology" | "quantitative-literacy"
            ) {
                let (location, uid) = assertion
                    .object_uri
                    .as_ref()
                    .unwrap()
                    .rsplit_once('#')
                    .unwrap();
                assert_eq!(location, locations[uid]);
            }
        }
    }
    assert!(!moved.join("files").exists());
}

#[test]
fn reproducible_computation_preserves_shared_routes_and_inline_examples_after_relocation() {
    let inquiry = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../institute/trails/ergon/hephaestus/inquiry");
    let directory = inquiry.join("reproducible-computation");
    let expected_documents = [
        &directory,
        &inquiry.join("quantitative-literacy"),
        &inquiry.join("metrology"),
    ]
    .into_iter()
    .map(|path| Publication::files(path).unwrap().len())
    .sum::<usize>();
    let temporary = tempfile::tempdir().unwrap();
    let library = Library::new(temporary.path().join("library"));
    let audit = Publication::audit(&library, &directory, &[]).unwrap();
    assert!(audit.issues.is_empty(), "{:?}", audit.issues);
    assert!(
        !audit
            .unchecked
            .iter()
            .any(|issue| issue.contains("directed navigation path")),
        "{:?}",
        audit.unchecked
    );
    for entry in [
        "Reproducible-computation.lingua",
        "data/A-complete-small-analysis.lingua",
        "numerics/Random-streams-and-task-identity.lingua",
        "research/Independent-investigation-and-revision.lingua",
        "sources/Scientific-Python-Sundnes.lingua",
    ] {
        let publication = Publication::load(
            &library,
            directory.join(entry).to_str().unwrap(),
            &[],
            false,
        )
        .unwrap();
        assert_eq!(publication.documents.len(), expected_documents, "{entry}");
        assert!(publication.parameters.is_empty());
        assert!(publication.instances.is_empty());
        assert!(publication.resources().unwrap().is_empty());
        assert!(publication.documents.iter().all(|document| {
            document.project.frequencies.is_empty() && document.project.rules.is_empty()
        }));
    }
    let publication = Publication::load(
        &library,
        directory
            .join("Reproducible-computation.lingua")
            .to_str()
            .unwrap(),
        &[],
        false,
    )
    .unwrap();
    let records: BTreeMap<_, _> = publication
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .map(|record| (record.slug.as_deref().unwrap(), record))
        .collect();
    let predicates = [
        "quantitative-literacy",
        "metrology",
        "reproducible-computation",
    ];
    let routes: BTreeMap<_, _> = predicates
        .into_iter()
        .map(|predicate| (predicate, learning_edges(&publication, predicate)))
        .collect();
    let edges = &routes["reproducible-computation"];
    let mut expected: BTreeSet<_> = publication
        .documents
        .iter()
        .filter(|document| {
            document
                .uri
                .contains("/ergon/hephaestus/inquiry/reproducible-computation/")
        })
        .filter(|document| !document.uri.contains("/sources/"))
        .flat_map(|document| &document.project.records)
        .filter(|record| record.slug.as_deref() != Some("rc-sources"))
        .map(|record| record.uid.clone())
        .collect();
    for (shared, continuation, original) in [
        ("ql-data", "rc-schema", "quantitative-literacy"),
        ("ql-sensitivity", "rc-conditioning", "quantitative-literacy"),
        (
            "ql-discretization",
            "rc-convergence",
            "quantitative-literacy",
        ),
        ("met-monte-carlo", "rc-streams", "metrology"),
    ] {
        let uid = records[shared].uid.clone();
        expected.insert(uid.clone());
        assert!(edges.contains(&(uid.clone(), records[continuation].uid.clone())));
        assert!(routes[original].iter().any(|(from, _)| *from == uid));
        assert!(
            records[shared]
                .body
                .contains("Continue in reproducible computation:")
        );
    }
    assert!(edges.is_disjoint(&routes["quantitative-literacy"]));
    assert!(edges.is_disjoint(&routes["metrology"]));
    let selected: BTreeSet<_> = edges
        .iter()
        .flat_map(|(from, to)| [from.clone(), to.clone()])
        .collect();
    assert_eq!(selected, expected);
    let mut reached = BTreeSet::new();
    let mut pending = vec![records["reproducible-computation"].uid.clone()];
    while let Some(uid) = pending.pop() {
        if reached.insert(uid.clone()) {
            pending.extend(
                edges
                    .iter()
                    .filter(|(from, _)| *from == uid)
                    .map(|(_, to)| to.clone()),
            );
        }
    }
    assert_eq!(reached, selected);
    for excluded in [
        "quantitative-literacy",
        "metrology",
        "rc-sources",
        "rc-sundnes",
    ] {
        assert!(!selected.contains(&records[excluded].uid));
    }
    let examples = |publication: &Publication| -> BTreeMap<String, Vec<String>> {
        publication
            .documents
            .iter()
            .flat_map(|document| &document.project.records)
            .filter(|record| record.body.contains("```"))
            .map(|record| {
                (
                    record.uid.clone(),
                    record
                        .body
                        .split("```")
                        .skip(1)
                        .step_by(2)
                        .map(str::to_owned)
                        .collect(),
                )
            })
            .collect()
    };
    let original_examples = examples(&publication);
    assert_eq!(original_examples.len(), 7);
    let references = external_references(&publication);
    assert!(!references.is_empty());
    assert!(!directory.join("books").exists());
    assert!(!directory.join("views").exists());
    let destination = temporary.path().join("export");
    let exported = publication.export(&library, &destination, false).unwrap();
    let entry = exported.strip_prefix(&destination).unwrap().to_owned();
    let moved = temporary.path().join("moved");
    fs::rename(destination, &moved).unwrap();
    let relocated_library = Library::new(temporary.path().join("relocated-library"));
    let reopened = Publication::load(
        &relocated_library,
        moved.join(entry).to_str().unwrap(),
        &[],
        false,
    )
    .unwrap();
    assert_eq!(reopened.documents.len(), expected_documents);
    assert_eq!(examples(&reopened), original_examples);
    assert_eq!(external_references(&reopened), references);
    for (predicate, original) in routes {
        assert_eq!(learning_edges(&reopened, predicate), original);
    }
    let locations: BTreeMap<_, _> = reopened
        .documents
        .iter()
        .flat_map(|document| {
            document
                .project
                .records
                .iter()
                .map(|record| (record.uid.as_str(), document.base.as_str()))
        })
        .collect();
    for record in reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
    {
        for assertion in &record.assertions {
            if predicates.contains(&assertion.predicate.as_str()) {
                let (location, uid) = assertion
                    .object_uri
                    .as_ref()
                    .unwrap()
                    .rsplit_once('#')
                    .unwrap();
                assert_eq!(location, locations[uid]);
            }
        }
    }
    assert!(!moved.join("files").exists());
}

#[test]
fn chemistry_discovers_shared_foundations_and_relocates_its_bounded_learning_selection() {
    let trails = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../institute/trails");
    let directory = trails.join("ergon/hephaestus/chemistry/general-chemistry");
    let expected_documents = [
        directory.clone(),
        trails.join("ergon/hephaestus/inquiry/quantitative-literacy"),
        trails.join("ergon/hephaestus/inquiry/metrology"),
        trails.join("ergon/hephaestus/inquiry/reproducible-computation"),
    ]
    .iter()
    .map(|path| Publication::files(path).unwrap().len())
    .sum::<usize>();
    let temporary = tempfile::tempdir().unwrap();
    let library = Library::new(temporary.path().join("library"));
    let audit = Publication::audit(&library, &directory, &[]).unwrap();
    assert!(audit.issues.is_empty(), "{:?}", audit.issues);
    assert!(
        !audit
            .unchecked
            .iter()
            .any(|issue| issue.contains("directed navigation path")),
        "{:?}",
        audit.unchecked
    );
    for entry in [
        "General-chemistry.lingua",
        "reactions/Reaction-extent-and-species-balances.lingua",
        "equilibrium/An-inspectable-equilibrium-solver.lingua",
        "research/Independent-scholarship-and-revision.lingua",
        "sources/Quimica-Geral-I-UECE.lingua",
    ] {
        let publication = Publication::load(
            &library,
            directory.join(entry).to_str().unwrap(),
            &[],
            false,
        )
        .unwrap();
        assert_eq!(publication.documents.len(), expected_documents, "{entry}");
        assert!(publication.parameters.is_empty());
        assert!(publication.instances.is_empty());
        assert!(publication.resources().unwrap().is_empty());
        assert!(publication.documents.iter().all(|document| {
            document.project.frequencies.is_empty() && document.project.rules.is_empty()
        }));
    }
    let publication = Publication::load(
        &library,
        directory.join("General-chemistry.lingua").to_str().unwrap(),
        &[],
        false,
    )
    .unwrap();
    let records: BTreeMap<_, _> = publication
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .map(|record| (record.slug.as_deref().unwrap(), record))
        .collect();
    assert_eq!(
        records["general-chemistry"].uid,
        "r_206GS6CVXD8WSY1VX7PRF7136V"
    );
    let predicates = [
        "quantitative-literacy",
        "metrology",
        "reproducible-computation",
        "general-chemistry",
    ];
    let routes: BTreeMap<_, _> = predicates
        .into_iter()
        .map(|predicate| (predicate, learning_edges(&publication, predicate)))
        .collect();
    let edges = &routes["general-chemistry"];
    let mut expected: BTreeSet<_> = publication
        .documents
        .iter()
        .filter(|document| {
            document
                .uri
                .contains("/ergon/hephaestus/chemistry/general-chemistry/")
        })
        .filter(|document| !document.uri.contains("/sources/"))
        .flat_map(|document| &document.project.records)
        .filter(|record| record.slug.as_deref() != Some("gc-sources"))
        .map(|record| record.uid.clone())
        .collect();
    for (shared, continuation, original) in [
        ("ql-units", "gc-amount", "quantitative-literacy"),
        ("ql-balances", "gc-extent", "quantitative-literacy"),
        (
            "ql-concentrations",
            "gc-preparation",
            "quantitative-literacy",
        ),
        ("met-chemistry-case", "gc-spectra", "metrology"),
        ("rc-numericaltests", "gc-solver", "reproducible-computation"),
    ] {
        let uid = records[shared].uid.clone();
        expected.insert(uid.clone());
        assert!(edges.contains(&(uid.clone(), records[continuation].uid.clone())));
        assert!(routes[original].iter().any(|(from, _)| *from == uid));
        assert!(
            records[shared]
                .body
                .contains("Continue in general chemistry:")
        );
    }
    let selected: BTreeSet<_> = edges
        .iter()
        .flat_map(|(from, to)| [from.clone(), to.clone()])
        .collect();
    assert_eq!(selected, expected);
    let mut reached = BTreeSet::new();
    let mut pending = vec![records["general-chemistry"].uid.clone()];
    while let Some(uid) = pending.pop() {
        if reached.insert(uid.clone()) {
            pending.extend(
                edges
                    .iter()
                    .filter(|(from, _)| *from == uid)
                    .map(|(_, to)| to.clone()),
            );
        }
    }
    assert_eq!(reached, selected);
    assert_eq!(selected.len(), 92);
    for excluded in [
        "quantitative-literacy",
        "metrology",
        "reproducible-computation",
        "gc-sources",
        "gc-openstax",
    ] {
        assert!(!selected.contains(&records[excluded].uid));
    }
    for (predicate, route) in &routes {
        if *predicate != "general-chemistry" {
            assert!(edges.is_disjoint(route));
        }
    }
    let code = |publication: &Publication| {
        publication
            .documents
            .iter()
            .flat_map(|document| &document.project.records)
            .find(|record| record.slug.as_deref() == Some("gc-solver"))
            .unwrap()
            .body
            .split("```python file=equilibrium.py\n")
            .nth(1)
            .unwrap()
            .split("\n```")
            .next()
            .unwrap()
            .to_owned()
    };
    let original_code = code(&publication);
    assert!(original_code.contains("def solve(added_base_ml):"));
    let references = external_references(&publication);
    assert!(!references.is_empty());
    assert!(!directory.join("books").exists());
    assert!(!directory.join("views").exists());
    let destination = temporary.path().join("export");
    let exported = publication.export(&library, &destination, false).unwrap();
    let entry = exported.strip_prefix(&destination).unwrap().to_owned();
    let moved = temporary.path().join("moved");
    fs::rename(destination, &moved).unwrap();
    let relocated_library = Library::new(temporary.path().join("relocated-library"));
    let reopened = Publication::load(
        &relocated_library,
        moved.join(entry).to_str().unwrap(),
        &[],
        false,
    )
    .unwrap();
    assert_eq!(reopened.documents.len(), expected_documents);
    assert_eq!(external_references(&reopened), references);
    assert_eq!(code(&reopened), original_code);
    for (predicate, original) in routes {
        assert_eq!(learning_edges(&reopened, predicate), original);
    }
    let locations: BTreeMap<_, _> = reopened
        .documents
        .iter()
        .flat_map(|document| {
            document
                .project
                .records
                .iter()
                .map(|record| (record.uid.as_str(), document.base.as_str()))
        })
        .collect();
    for record in reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
    {
        for assertion in &record.assertions {
            if predicates.contains(&assertion.predicate.as_str()) {
                let (location, uid) = assertion
                    .object_uri
                    .as_ref()
                    .unwrap()
                    .rsplit_once('#')
                    .unwrap();
                assert_eq!(location, locations[uid]);
            }
        }
    }
    assert!(!moved.join("files").exists());
}

#[test]
fn metallurgy_preserves_shared_routes_and_three_original_examples_after_relocation() {
    let trails = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../institute/trails");
    let directory = trails.join("ergon/hephaestus/materials/metals-and-metallurgical-production");
    let expected_documents = [
        directory.clone(),
        trails.join("ergon/hephaestus/engineering/mining-and-mineral-engineering"),
        trails.join("ergon/gaia/earth/mineralogy-and-petrology"),
        trails.join("ergon/gaia/earth/geology"),
        trails.join("ergon/hephaestus/inquiry/quantitative-literacy"),
        trails.join("ergon/hephaestus/inquiry/metrology"),
        trails.join("ergon/hephaestus/inquiry/reproducible-computation"),
        trails.join("ergon/hephaestus/chemistry/general-chemistry"),
    ]
    .iter()
    .map(|path| Publication::files(path).unwrap().len())
    .sum::<usize>();
    let temporary = tempfile::tempdir().unwrap();
    let library = Library::new(temporary.path().join("library"));
    let audit = Publication::audit(&library, &directory, &[]).unwrap();
    assert!(audit.issues.is_empty(), "{:?}", audit.issues);
    assert!(
        !audit
            .unchecked
            .iter()
            .any(|issue| issue.contains("directed navigation path")),
        "{:?}",
        audit.unchecked
    );
    for entry in [
        "Metals-and-metallurgical-production.lingua",
        "balances/An-inspectable-three-feed-charge.lingua",
        "solidification/An-inspectable-equilibrium-and-Scheil-comparison.lingua",
        "research/Advanced-thermodynamic-diffusion-and-phase-field-model-study.lingua",
        "sources/Phase-Transitions-Bhadeshia-Yan.lingua",
    ] {
        let publication = Publication::load(
            &library,
            directory.join(entry).to_str().unwrap(),
            &[],
            false,
        )
        .unwrap();
        assert_eq!(publication.documents.len(), expected_documents, "{entry}");
        assert!(publication.parameters.is_empty());
        assert!(publication.instances.is_empty());
        assert!(publication.resources().unwrap().is_empty());
        assert!(publication.documents.iter().all(|document| {
            document.project.frequencies.is_empty() && document.project.rules.is_empty()
        }));
    }
    let publication = Publication::load(
        &library,
        directory
            .join("Metals-and-metallurgical-production.lingua")
            .to_str()
            .unwrap(),
        &[],
        false,
    )
    .unwrap();
    let records: BTreeMap<_, _> = publication
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .map(|record| (record.slug.as_deref().unwrap(), record))
        .collect();
    assert_eq!(
        records["metals-and-metallurgical-production"].uid,
        "r_59BDAQXT81552YF5F8TH4KQQH1"
    );
    let predicates = [
        "quantitative-literacy",
        "metrology",
        "reproducible-computation",
        "general-chemistry",
        "geology",
        "mineralogy-and-petrology",
        "mining-and-mineral-engineering",
        "metals-and-metallurgical-production",
    ];
    let routes: BTreeMap<_, _> = predicates
        .into_iter()
        .map(|predicate| (predicate, learning_edges(&publication, predicate)))
        .collect();
    let edges = &routes["metals-and-metallurgical-production"];
    let mut expected: BTreeSet<_> = publication
        .documents
        .iter()
        .filter(|document| {
            document
                .uri
                .contains("/ergon/hephaestus/materials/metals-and-metallurgical-production/")
        })
        .filter(|document| !document.uri.contains("/sources/"))
        .flat_map(|document| &document.project.records)
        .filter(|record| record.slug.as_deref() != Some("metal-sources"))
        .map(|record| record.uid.clone())
        .collect();
    for (shared, continuation, original) in [
        (
            "mine-handoffs",
            "metal-feed",
            "mining-and-mineral-engineering",
        ),
        ("ql-balances", "metal-balance", "quantitative-literacy"),
        (
            "gc-chemical-potential",
            "metal-activities",
            "general-chemistry",
        ),
        (
            "mp-lever",
            "metal-phase-diagrams",
            "mineralogy-and-petrology",
        ),
        ("gc-transport", "metal-transport", "general-chemistry"),
        (
            "rc-numericaltests",
            "metal-diffusion-example",
            "reproducible-computation",
        ),
        (
            "mine-calcination",
            "metal-roasting",
            "mining-and-mineral-engineering",
        ),
        ("gc-nernst", "metal-electrode-kinetics", "general-chemistry"),
        (
            "gc-electrolysis",
            "metal-current-efficiency",
            "general-chemistry",
        ),
        (
            "mp-mapping",
            "metal-metallography",
            "mineralogy-and-petrology",
        ),
        (
            "mp-microprobe",
            "metal-chemical-analysis",
            "mineralogy-and-petrology",
        ),
        ("met-force-pressure", "metal-tensile", "metrology"),
        (
            "mine-accounting",
            "metal-plant-accounting",
            "mining-and-mineral-engineering",
        ),
    ] {
        let uid = records[shared].uid.clone();
        expected.insert(uid.clone());
        assert!(edges.contains(&(uid.clone(), records[continuation].uid.clone())));
        assert!(routes[original].iter().any(|(from, _)| *from == uid));
        assert!(
            records[shared]
                .body
                .contains("Continue in metals and metallurgical production:")
        );
    }
    let selected: BTreeSet<_> = edges
        .iter()
        .flat_map(|(from, to)| [from.clone(), to.clone()])
        .collect();
    assert_eq!(selected, expected);
    let mut reached = BTreeSet::new();
    let mut pending = vec![records["metals-and-metallurgical-production"].uid.clone()];
    while let Some(uid) = pending.pop() {
        if reached.insert(uid.clone()) {
            pending.extend(
                edges
                    .iter()
                    .filter(|(from, _)| *from == uid)
                    .map(|(_, to)| to.clone()),
            );
        }
    }
    assert_eq!(reached, selected);
    assert_eq!(selected.len(), 136);
    for excluded in [
        "quantitative-literacy",
        "metrology",
        "reproducible-computation",
        "general-chemistry",
        "geology",
        "mineralogy-and-petrology",
        "mining-and-mineral-engineering",
        "metal-sources",
        "metal-phase-book",
        "metal-extract-course",
    ] {
        assert!(!selected.contains(&records[excluded].uid));
    }
    for (predicate, route) in &routes {
        if *predicate != "metals-and-metallurgical-production" {
            assert!(edges.is_disjoint(route));
        }
    }
    let examples = |publication: &Publication| {
        publication
            .documents
            .iter()
            .flat_map(|document| &document.project.records)
            .filter_map(|record| {
                let file = match record.slug.as_deref()? {
                    "metal-charge-example" => "three_feed_charge.py",
                    "metal-diffusion-example" => "periodic_diffusion.py",
                    "metal-segregation-example" => "segregation_limits.py",
                    _ => return None,
                };
                Some((
                    file.to_owned(),
                    record
                        .body
                        .split(&format!("```python file={file}\n"))
                        .nth(1)
                        .unwrap()
                        .split("\n```")
                        .next()
                        .unwrap()
                        .to_owned(),
                ))
            })
            .collect::<BTreeMap<_, _>>()
    };
    let original_examples = examples(&publication);
    assert_eq!(original_examples.len(), 3);
    let references = external_references(&publication);
    assert!(!references.is_empty());
    assert!(!directory.join("books").exists());
    assert!(!directory.join("views").exists());
    let destination = temporary.path().join("export");
    let exported = publication.export(&library, &destination, false).unwrap();
    let entry = exported.strip_prefix(&destination).unwrap().to_owned();
    let moved = temporary.path().join("moved");
    fs::rename(destination, &moved).unwrap();
    let relocated_library = Library::new(temporary.path().join("relocated-library"));
    let reopened = Publication::load(
        &relocated_library,
        moved.join(entry).to_str().unwrap(),
        &[],
        false,
    )
    .unwrap();
    assert_eq!(reopened.documents.len(), expected_documents);
    assert_eq!(external_references(&reopened), references);
    assert_eq!(examples(&reopened), original_examples);
    for (predicate, original) in routes {
        assert_eq!(learning_edges(&reopened, predicate), original);
    }
    let locations: BTreeMap<_, _> = reopened
        .documents
        .iter()
        .flat_map(|document| {
            document
                .project
                .records
                .iter()
                .map(|record| (record.uid.as_str(), document.base.as_str()))
        })
        .collect();
    for record in reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
    {
        for assertion in &record.assertions {
            if predicates.contains(&assertion.predicate.as_str()) {
                let (location, uid) = assertion
                    .object_uri
                    .as_ref()
                    .unwrap()
                    .rsplit_once('#')
                    .unwrap();
                assert_eq!(location, locations[uid]);
            }
        }
    }
    assert!(!moved.join("files").exists());
}

#[test]
fn industrial_chemistry_preserves_shared_routes_and_three_original_examples_after_relocation() {
    let trails = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../institute/trails");
    let directory = trails.join("ergon/hephaestus/chemistry/industrial-chemistry");
    let expected_documents = [
        directory.clone(),
        trails.join("ergon/hephaestus/materials/metals-and-metallurgical-production"),
        trails.join("ergon/hephaestus/engineering/mining-and-mineral-engineering"),
        trails.join("ergon/gaia/earth/mineralogy-and-petrology"),
        trails.join("ergon/gaia/earth/geology"),
        trails.join("ergon/hephaestus/inquiry/quantitative-literacy"),
        trails.join("ergon/hephaestus/inquiry/metrology"),
        trails.join("ergon/hephaestus/inquiry/reproducible-computation"),
        trails.join("ergon/hephaestus/chemistry/general-chemistry"),
    ]
    .iter()
    .map(|path| Publication::files(path).unwrap().len())
    .sum::<usize>();
    let temporary = tempfile::tempdir().unwrap();
    let library = Library::new(temporary.path().join("library"));
    let audit = Publication::audit(&library, &directory, &[]).unwrap();
    assert!(audit.issues.is_empty(), "{:?}", audit.issues);
    assert!(
        !audit
            .unchecked
            .iter()
            .any(|issue| issue.contains("directed navigation path")),
        "{:?}",
        audit.unchecked
    );
    for entry in [
        "Industrial-chemistry.lingua",
        "balances/An-inspectable-recycle-and-purge-account.lingua",
        "separations/An-inspectable-fixed-ratio-flash.lingua",
        "research/Advanced-industrial-thermodynamics-and-phase-model-study.lingua",
        "sources/Chemical-Reactor-Analysis-Rawlings-Ekerdt.lingua",
    ] {
        let publication = Publication::load(
            &library,
            directory.join(entry).to_str().unwrap(),
            &[],
            false,
        )
        .unwrap();
        assert_eq!(publication.documents.len(), expected_documents, "{entry}");
        assert!(publication.parameters.is_empty());
        assert!(publication.instances.is_empty());
        assert!(publication.resources().unwrap().is_empty());
        assert!(publication.documents.iter().all(|document| {
            document.project.frequencies.is_empty() && document.project.rules.is_empty()
        }));
    }
    let publication = Publication::load(
        &library,
        directory
            .join("Industrial-chemistry.lingua")
            .to_str()
            .unwrap(),
        &[],
        false,
    )
    .unwrap();
    let records: BTreeMap<_, _> = publication
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .map(|record| (record.slug.as_deref().unwrap(), record))
        .collect();
    assert_eq!(
        records["industrial-chemistry"].uid,
        "r_3AKBAFB2ABJ0671QW6ZBQ0B618"
    );
    let predicates = [
        "quantitative-literacy",
        "metrology",
        "reproducible-computation",
        "general-chemistry",
        "geology",
        "mineralogy-and-petrology",
        "mining-and-mineral-engineering",
        "metals-and-metallurgical-production",
        "industrial-chemistry",
    ];
    let routes: BTreeMap<_, _> = predicates
        .into_iter()
        .map(|predicate| (predicate, learning_edges(&publication, predicate)))
        .collect();
    let edges = &routes["industrial-chemistry"];
    let mut expected: BTreeSet<_> = publication
        .documents
        .iter()
        .filter(|document| {
            document
                .uri
                .contains("/ergon/hephaestus/chemistry/industrial-chemistry/")
        })
        .filter(|document| !document.uri.contains("/sources/"))
        .flat_map(|document| &document.project.records)
        .filter(|record| record.slug.as_deref() != Some("ic-sources"))
        .map(|record| record.uid.clone())
        .collect();
    for (shared, continuation, original) in [
        ("ql-units", "ic-quantities", "quantitative-literacy"),
        ("met-sampling-target", "ic-feed", "metrology"),
        ("ql-balances", "ic-elements", "quantitative-literacy"),
        ("gc-extent", "ic-stoichiometry", "general-chemistry"),
        ("gc-yield", "ic-yield", "general-chemistry"),
        (
            "rc-numericaltests",
            "ic-recycle-example",
            "reproducible-computation",
        ),
        (
            "gc-chemical-potential",
            "ic-reaction-equilibrium",
            "general-chemistry",
        ),
        ("gc-phases", "ic-phase-equilibrium", "general-chemistry"),
        ("gc-catalysis", "ic-catalyst-life", "general-chemistry"),
        ("gc-transport", "ic-scale-transfer", "general-chemistry"),
        (
            "gc-separations",
            "ic-separation-trains",
            "general-chemistry",
        ),
        ("met-chemistry-case", "ic-process-analysis", "metrology"),
        ("met-conformity", "ic-release", "metrology"),
    ] {
        let uid = records[shared].uid.clone();
        expected.insert(uid.clone());
        assert!(edges.contains(&(uid.clone(), records[continuation].uid.clone())));
        assert!(routes[original].iter().any(|(from, _)| *from == uid));
        assert!(
            records[shared]
                .body
                .contains("Continue in industrial chemistry:")
        );
    }
    let selected: BTreeSet<_> = edges
        .iter()
        .flat_map(|(from, to)| [from.clone(), to.clone()])
        .collect();
    assert_eq!(selected, expected);
    let mut reached = BTreeSet::new();
    let mut pending = vec![records["industrial-chemistry"].uid.clone()];
    while let Some(uid) = pending.pop() {
        if reached.insert(uid.clone()) {
            pending.extend(
                edges
                    .iter()
                    .filter(|(from, _)| *from == uid)
                    .map(|(_, to)| to.clone()),
            );
        }
    }
    assert_eq!(reached, selected);
    assert_eq!(selected.len(), 140);
    for excluded in [
        "quantitative-literacy",
        "metrology",
        "reproducible-computation",
        "general-chemistry",
        "geology",
        "mineralogy-and-petrology",
        "mining-and-mineral-engineering",
        "metals-and-metallurgical-production",
        "ic-sources",
        "ic-noun-one",
        "ic-reactor-book",
    ] {
        assert!(!selected.contains(&records[excluded].uid));
    }
    for (predicate, route) in &routes {
        if *predicate != "industrial-chemistry" {
            assert!(edges.is_disjoint(route));
        }
    }
    let examples = |publication: &Publication| {
        publication
            .documents
            .iter()
            .flat_map(|document| &document.project.records)
            .filter_map(|record| {
                let file = match record.slug.as_deref()? {
                    "ic-recycle-example" => "recycle_purge.py",
                    "ic-flash-example" => "fixed_ratio_flash.py",
                    "ic-cascade-example" => "consecutive_cascade.py",
                    _ => return None,
                };
                Some((
                    file.to_owned(),
                    record
                        .body
                        .split(&format!("```python file={file}\n"))
                        .nth(1)
                        .unwrap()
                        .split("\n```")
                        .next()
                        .unwrap()
                        .to_owned(),
                ))
            })
            .collect::<BTreeMap<_, _>>()
    };
    let original_examples = examples(&publication);
    assert_eq!(original_examples.len(), 3);
    let references = external_references(&publication);
    assert!(!references.is_empty());
    assert!(!directory.join("books").exists());
    assert!(!directory.join("views").exists());
    let destination = temporary.path().join("export");
    let exported = publication.export(&library, &destination, false).unwrap();
    let entry = exported.strip_prefix(&destination).unwrap().to_owned();
    let moved = temporary.path().join("moved");
    fs::rename(destination, &moved).unwrap();
    let relocated_library = Library::new(temporary.path().join("relocated-library"));
    let reopened = Publication::load(
        &relocated_library,
        moved.join(entry).to_str().unwrap(),
        &[],
        false,
    )
    .unwrap();
    assert_eq!(reopened.documents.len(), expected_documents);
    assert_eq!(external_references(&reopened), references);
    assert_eq!(examples(&reopened), original_examples);
    for (predicate, original) in routes {
        assert_eq!(learning_edges(&reopened, predicate), original);
    }
    let locations: BTreeMap<_, _> = reopened
        .documents
        .iter()
        .flat_map(|document| {
            document
                .project
                .records
                .iter()
                .map(|record| (record.uid.as_str(), document.base.as_str()))
        })
        .collect();
    for record in reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
    {
        for assertion in &record.assertions {
            if predicates.contains(&assertion.predicate.as_str()) {
                let (location, uid) = assertion
                    .object_uri
                    .as_ref()
                    .unwrap()
                    .rsplit_once('#')
                    .unwrap();
                assert_eq!(location, locations[uid]);
            }
        }
    }
    assert!(!moved.join("files").exists());
}

#[test]
fn chemical_supply_preserves_shared_routes_and_three_original_examples_after_relocation() {
    let trails = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../institute/trails");
    let directory =
        trails.join("ergon/hephaestus/materials/chemical-feedstocks-and-supply-systems");
    let expected_documents = [
        directory.clone(),
        trails.join("ergon/hephaestus/chemistry/industrial-chemistry"),
        trails.join("ergon/hephaestus/materials/metals-and-metallurgical-production"),
        trails.join("ergon/hephaestus/engineering/mining-and-mineral-engineering"),
        trails.join("ergon/gaia/earth/mineralogy-and-petrology"),
        trails.join("ergon/gaia/earth/geology"),
        trails.join("ergon/hephaestus/inquiry/quantitative-literacy"),
        trails.join("ergon/hephaestus/inquiry/metrology"),
        trails.join("ergon/hephaestus/inquiry/reproducible-computation"),
        trails.join("ergon/hephaestus/chemistry/general-chemistry"),
    ]
    .iter()
    .map(|path| Publication::files(path).unwrap().len())
    .sum::<usize>();
    let temporary = tempfile::tempdir().unwrap();
    let library = Library::new(temporary.path().join("library"));
    let audit = Publication::audit(&library, &directory, &[]).unwrap();
    assert!(audit.issues.is_empty(), "{:?}", audit.issues);
    assert!(
        !audit
            .unchecked
            .iter()
            .any(|issue| issue.contains("directed navigation path")),
        "{:?}",
        audit.unchecked
    );
    for entry in [
        "Chemical-feedstocks-and-supply-systems.lingua",
        "inventory/An-inspectable-lot-release-and-expiry-account.lingua",
        "inventory/An-inspectable-exact-replenishment-demand-distribution.lingua",
        "research/Advanced-common-dependency-and-restoration-study.lingua",
        "sources/Inventory-Analytics-Rossi.lingua",
    ] {
        let publication = Publication::load(
            &library,
            directory.join(entry).to_str().unwrap(),
            &[],
            false,
        )
        .unwrap();
        assert_eq!(publication.documents.len(), expected_documents, "{entry}");
        assert!(publication.parameters.is_empty());
        assert!(publication.instances.is_empty());
        assert!(publication.resources().unwrap().is_empty());
        assert!(publication.documents.iter().all(|document| {
            document.project.frequencies.is_empty() && document.project.rules.is_empty()
        }));
    }
    let publication = Publication::load(
        &library,
        directory
            .join("Chemical-feedstocks-and-supply-systems.lingua")
            .to_str()
            .unwrap(),
        &[],
        false,
    )
    .unwrap();
    let records: BTreeMap<_, _> = publication
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .map(|record| (record.slug.as_deref().unwrap(), record))
        .collect();
    assert_eq!(
        records["chemical-supply-systems"].uid,
        "r_05V6FYWMN0VND552CQE94HHA2Q"
    );
    let predicates = [
        "quantitative-literacy",
        "metrology",
        "reproducible-computation",
        "general-chemistry",
        "geology",
        "mineralogy-and-petrology",
        "mining-and-mineral-engineering",
        "metals-and-metallurgical-production",
        "industrial-chemistry",
        "chemical-supply-systems",
    ];
    let routes: BTreeMap<_, _> = predicates
        .into_iter()
        .map(|predicate| (predicate, learning_edges(&publication, predicate)))
        .collect();
    let edges = &routes["chemical-supply-systems"];
    let mut expected: BTreeSet<_> = publication
        .documents
        .iter()
        .filter(|document| {
            document
                .uri
                .contains("/ergon/hephaestus/materials/chemical-feedstocks-and-supply-systems/")
        })
        .filter(|document| !document.uri.contains("/sources/"))
        .flat_map(|document| &document.project.records)
        .filter(|record| record.slug.as_deref() != Some("cs-sources"))
        .map(|record| record.uid.clone())
        .collect();
    for (shared, continuation, original) in [
        ("ql-units", "cs-quantity", "quantitative-literacy"),
        ("ic-specification", "cs-agreement", "industrial-chemistry"),
        ("ic-air-gases", "cs-gas-supply", "industrial-chemistry"),
        (
            "ic-chloralkali-process",
            "cs-coproduct",
            "industrial-chemistry",
        ),
        ("ic-methanol", "cs-organic-supply", "industrial-chemistry"),
        ("ic-monomers", "cs-stabilized-feed", "industrial-chemistry"),
        ("met-sampling-target", "cs-receiving-sampling", "metrology"),
        ("met-conformity", "cs-release", "metrology"),
        (
            "ic-transfers",
            "cs-storage-envelope",
            "industrial-chemistry",
        ),
        (
            "ic-equipment",
            "cs-contact-materials",
            "industrial-chemistry",
        ),
        (
            "ql-balances",
            "cs-inventory-account",
            "quantitative-literacy",
        ),
        (
            "rc-numericaltests",
            "cs-lot-example",
            "reproducible-computation",
        ),
        ("ic-utilities", "cs-dependencies", "industrial-chemistry"),
        ("ic-recovery", "cs-circular", "industrial-chemistry"),
    ] {
        let uid = records[shared].uid.clone();
        expected.insert(uid.clone());
        assert!(edges.contains(&(uid.clone(), records[continuation].uid.clone())));
        assert!(routes[original].iter().any(|(from, _)| *from == uid));
        assert!(
            records[shared]
                .body
                .contains("Continue in chemical supply systems:")
        );
    }
    let selected: BTreeSet<_> = edges
        .iter()
        .flat_map(|(from, to)| [from.clone(), to.clone()])
        .collect();
    assert_eq!(selected, expected);
    let mut reached = BTreeSet::new();
    let mut pending = vec![records["chemical-supply-systems"].uid.clone()];
    while let Some(uid) = pending.pop() {
        if reached.insert(uid.clone()) {
            pending.extend(
                edges
                    .iter()
                    .filter(|(from, _)| *from == uid)
                    .map(|(_, to)| to.clone()),
            );
        }
    }
    assert_eq!(reached, selected);
    assert_eq!(selected.len(), 104);
    for excluded in [
        "quantitative-literacy",
        "metrology",
        "reproducible-computation",
        "general-chemistry",
        "geology",
        "mineralogy-and-petrology",
        "mining-and-mineral-engineering",
        "metals-and-metallurgical-production",
        "industrial-chemistry",
        "cs-sources",
        "cs-rossi",
        "cs-warehouse",
    ] {
        assert!(!selected.contains(&records[excluded].uid));
    }
    for (predicate, route) in &routes {
        if *predicate != "chemical-supply-systems" {
            assert!(edges.is_disjoint(route));
        }
    }
    let examples = |publication: &Publication| {
        publication
            .documents
            .iter()
            .flat_map(|document| &document.project.records)
            .filter_map(|record| {
                let file = match record.slug.as_deref()? {
                    "cs-lot-example" => "lot_release_expiry.py",
                    "cs-lead-example" => "exact_replenishment_demand.py",
                    "cs-dual-source" => "two_source_reservation.py",
                    _ => return None,
                };
                Some((
                    file.to_owned(),
                    record
                        .body
                        .split(&format!("```python file={file}\n"))
                        .nth(1)
                        .unwrap()
                        .split("\n```")
                        .next()
                        .unwrap()
                        .to_owned(),
                ))
            })
            .collect::<BTreeMap<_, _>>()
    };
    let original_examples = examples(&publication);
    assert_eq!(original_examples.len(), 3);
    let references = external_references(&publication);
    assert!(!references.is_empty());
    assert!(!directory.join("books").exists());
    assert!(!directory.join("views").exists());
    let destination = temporary.path().join("export");
    let exported = publication.export(&library, &destination, false).unwrap();
    let entry = exported.strip_prefix(&destination).unwrap().to_owned();
    let moved = temporary.path().join("moved");
    fs::rename(destination, &moved).unwrap();
    let relocated_library = Library::new(temporary.path().join("relocated-library"));
    let reopened = Publication::load(
        &relocated_library,
        moved.join(entry).to_str().unwrap(),
        &[],
        false,
    )
    .unwrap();
    assert_eq!(reopened.documents.len(), expected_documents);
    assert_eq!(external_references(&reopened), references);
    assert_eq!(examples(&reopened), original_examples);
    for (predicate, original) in routes {
        assert_eq!(learning_edges(&reopened, predicate), original);
    }
    let locations: BTreeMap<_, _> = reopened
        .documents
        .iter()
        .flat_map(|document| {
            document
                .project
                .records
                .iter()
                .map(|record| (record.uid.as_str(), document.base.as_str()))
        })
        .collect();
    for record in reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
    {
        for assertion in &record.assertions {
            if predicates.contains(&assertion.predicate.as_str()) {
                let (location, uid) = assertion
                    .object_uri
                    .as_ref()
                    .unwrap()
                    .rsplit_once('#')
                    .unwrap();
                assert_eq!(location, locations[uid]);
            }
        }
    }
    assert!(!moved.join("files").exists());
}

#[test]
fn electromagnetism_preserves_shared_routes_and_three_original_examples_after_relocation() {
    let trails = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../institute/trails");
    let directory = trails.join("ergon/hephaestus/physics/electromagnetism");
    let expected_documents = [
        directory.clone(),
        trails.join("ergon/hephaestus/inquiry/quantitative-literacy"),
        trails.join("ergon/hephaestus/inquiry/metrology"),
        trails.join("ergon/hephaestus/inquiry/reproducible-computation"),
    ]
    .iter()
    .map(|path| Publication::files(path).unwrap().len())
    .sum::<usize>();
    let temporary = tempfile::tempdir().unwrap();
    let library = Library::new(temporary.path().join("library"));
    let audit = Publication::audit(&library, &directory, &[]).unwrap();
    assert!(audit.issues.is_empty(), "{:?}", audit.issues);
    assert!(
        !audit
            .unchecked
            .iter()
            .any(|issue| issue.contains("directed navigation path")),
        "{:?}",
        audit.unchecked
    );
    for entry in [
        "Electromagnetism.lingua",
        "computation/An-inspectable-conservative-electrostatic-solver.lingua",
        "computation/An-inspectable-periodic-Yee-wave.lingua",
        "research/Advanced-time-domain-dispersion-PML-and-validation-study.lingua",
        "sources/Measuring-Permittivity-and-Permeability-NIST-TN-1536.lingua",
    ] {
        let publication = Publication::load(
            &library,
            directory.join(entry).to_str().unwrap(),
            &[],
            false,
        )
        .unwrap();
        assert_eq!(publication.documents.len(), expected_documents, "{entry}");
        assert!(publication.parameters.is_empty());
        assert!(publication.instances.is_empty());
        assert!(publication.resources().unwrap().is_empty());
        assert!(publication.documents.iter().all(|document| {
            document.project.frequencies.is_empty() && document.project.rules.is_empty()
        }));
    }
    let publication = Publication::load(
        &library,
        directory.join("Electromagnetism.lingua").to_str().unwrap(),
        &[],
        false,
    )
    .unwrap();
    let records: BTreeMap<_, _> = publication
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .map(|record| (record.slug.as_deref().unwrap(), record))
        .collect();
    assert_eq!(
        records["electromagnetism"].uid,
        "r_5TJPESCTTNAKY66JH2QNQYN1PY"
    );
    let predicates = [
        "quantitative-literacy",
        "metrology",
        "reproducible-computation",
        "electromagnetism",
    ];
    let routes: BTreeMap<_, _> = predicates
        .into_iter()
        .map(|predicate| (predicate, learning_edges(&publication, predicate)))
        .collect();
    let edges = &routes["electromagnetism"];
    let mut expected: BTreeSet<_> = publication
        .documents
        .iter()
        .filter(|document| {
            document
                .uri
                .contains("/ergon/hephaestus/physics/electromagnetism/")
        })
        .filter(|document| !document.uri.contains("/sources/"))
        .flat_map(|document| &document.project.records)
        .filter(|record| record.slug.as_deref() != Some("em-sources"))
        .map(|record| record.uid.clone())
        .collect();
    for (shared, continuation, original) in [
        ("ql-units", "em-constants", "quantitative-literacy"),
        ("ql-vectors", "em-vectorfields", "quantitative-literacy"),
        ("ql-small-changes", "em-calculus", "quantitative-literacy"),
        ("ql-rates", "em-continuity", "quantitative-literacy"),
        ("ql-energy", "em-potential", "quantitative-literacy"),
        ("met-electrical-case", "em-probing", "metrology"),
        ("met-time-frequency", "em-phasors", "metrology"),
        ("met-optical", "em-detection", "metrology"),
        (
            "ql-discretization",
            "em-numerical-fields",
            "quantitative-literacy",
        ),
        (
            "rc-numericaltests",
            "em-poisson-example",
            "reproducible-computation",
        ),
        ("met-model-discrepancy", "em-uncertainty", "metrology"),
    ] {
        let uid = records[shared].uid.clone();
        expected.insert(uid.clone());
        assert!(edges.contains(&(uid.clone(), records[continuation].uid.clone())));
        assert!(routes[original].iter().any(|(from, _)| *from == uid));
        assert!(
            records[shared]
                .body
                .contains("Continue in electromagnetism:")
        );
    }
    let selected: BTreeSet<_> = edges
        .iter()
        .flat_map(|(from, to)| [from.clone(), to.clone()])
        .collect();
    assert_eq!(selected, expected);
    let mut reached = BTreeSet::new();
    let mut pending = vec![records["electromagnetism"].uid.clone()];
    while let Some(uid) = pending.pop() {
        if reached.insert(uid.clone()) {
            pending.extend(
                edges
                    .iter()
                    .filter(|(from, _)| *from == uid)
                    .map(|(_, to)| to.clone()),
            );
        }
    }
    assert_eq!(reached, selected);
    assert_eq!(selected.len(), 116);
    for excluded in [
        "quantitative-literacy",
        "metrology",
        "reproducible-computation",
        "em-sources",
        "em-openstax",
        "em-nist1536",
    ] {
        assert!(!selected.contains(&records[excluded].uid));
    }
    for (predicate, route) in &routes {
        if *predicate != "electromagnetism" {
            assert!(edges.is_disjoint(route));
        }
    }
    let examples = |publication: &Publication| {
        publication
            .documents
            .iter()
            .flat_map(|document| &document.project.records)
            .filter_map(|record| {
                let file = match record.slug.as_deref()? {
                    "em-poisson-example" => "electrostatic_faces.py",
                    "em-layer-example" => "dielectric_stack.py",
                    "em-yee-example" => "periodic_yee.py",
                    _ => return None,
                };
                Some((
                    file.to_owned(),
                    record
                        .body
                        .split(&format!("```python file={file}\n"))
                        .nth(1)
                        .unwrap()
                        .split("\n```")
                        .next()
                        .unwrap()
                        .to_owned(),
                ))
            })
            .collect::<BTreeMap<_, _>>()
    };
    let original_examples = examples(&publication);
    assert_eq!(original_examples.len(), 3);
    let references = external_references(&publication);
    assert!(!references.is_empty());
    assert!(!directory.join("books").exists());
    assert!(!directory.join("views").exists());
    let destination = temporary.path().join("export");
    let exported = publication.export(&library, &destination, false).unwrap();
    let entry = exported.strip_prefix(&destination).unwrap().to_owned();
    let moved = temporary.path().join("moved");
    fs::rename(destination, &moved).unwrap();
    let relocated_library = Library::new(temporary.path().join("relocated-library"));
    let reopened = Publication::load(
        &relocated_library,
        moved.join(entry).to_str().unwrap(),
        &[],
        false,
    )
    .unwrap();
    assert_eq!(reopened.documents.len(), expected_documents);
    assert_eq!(external_references(&reopened), references);
    assert_eq!(examples(&reopened), original_examples);
    for (predicate, original) in routes {
        assert_eq!(learning_edges(&reopened, predicate), original);
    }
    let locations: BTreeMap<_, _> = reopened
        .documents
        .iter()
        .flat_map(|document| {
            document
                .project
                .records
                .iter()
                .map(|record| (record.uid.as_str(), document.base.as_str()))
        })
        .collect();
    for record in reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
    {
        for assertion in &record.assertions {
            if predicates.contains(&assertion.predicate.as_str()) {
                let (location, uid) = assertion
                    .object_uri
                    .as_ref()
                    .unwrap()
                    .rsplit_once('#')
                    .unwrap();
                assert_eq!(location, locations[uid]);
            }
        }
    }
    assert!(!moved.join("files").exists());
}

#[test]
fn power_engineering_preserves_shared_routes_and_three_original_examples_after_relocation() {
    let trails = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../institute/trails");
    let directory = trails.join("ergon/hephaestus/engineering/electrical-power-engineering");
    let expected_documents = [
        directory.clone(),
        trails.join("ergon/hephaestus/physics/electromagnetism"),
        trails.join("ergon/hephaestus/inquiry/quantitative-literacy"),
        trails.join("ergon/hephaestus/inquiry/metrology"),
        trails.join("ergon/hephaestus/inquiry/reproducible-computation"),
    ]
    .iter()
    .map(|path| Publication::files(path).unwrap().len())
    .sum::<usize>();
    let temporary = tempfile::tempdir().unwrap();
    let library = Library::new(temporary.path().join("library"));
    let audit = Publication::audit(&library, &directory, &[]).unwrap();
    assert!(audit.issues.is_empty(), "{:?}", audit.issues);
    assert!(
        !audit
            .unchecked
            .iter()
            .any(|issue| issue.contains("directed navigation path")),
        "{:?}",
        audit.unchecked
    );
    for entry in [
        "Electrical-power-engineering.lingua",
        "computation/An-inspectable-two-bus-AC-power-flow.lingua",
        "computation/An-inspectable-constrained-three-bus-dispatch.lingua",
        "research/Advanced-islanding-black-start-and-restoration-study.lingua",
        "sources/MATPOWER-User-Manual-8-1.lingua",
    ] {
        let publication = Publication::load(
            &library,
            directory.join(entry).to_str().unwrap(),
            &[],
            false,
        )
        .unwrap();
        assert_eq!(publication.documents.len(), expected_documents, "{entry}");
        assert!(publication.parameters.is_empty());
        assert!(publication.instances.is_empty());
        assert!(publication.resources().unwrap().is_empty());
        assert!(publication.documents.iter().all(|document| {
            document.project.frequencies.is_empty() && document.project.rules.is_empty()
        }));
    }
    let publication = Publication::load(
        &library,
        directory
            .join("Electrical-power-engineering.lingua")
            .to_str()
            .unwrap(),
        &[],
        false,
    )
    .unwrap();
    let records: BTreeMap<_, _> = publication
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .map(|record| (record.slug.as_deref().unwrap(), record))
        .collect();
    assert_eq!(
        records["electrical-power-engineering"].uid,
        "r_300EPNGATK7PHWNZCQFBXGZFCQ"
    );
    let predicates = [
        "quantitative-literacy",
        "metrology",
        "reproducible-computation",
        "electromagnetism",
        "electrical-power-engineering",
    ];
    let routes: BTreeMap<_, _> = predicates
        .into_iter()
        .map(|predicate| (predicate, learning_edges(&publication, predicate)))
        .collect();
    let edges = &routes["electrical-power-engineering"];
    let mut expected: BTreeSet<_> = publication
        .documents
        .iter()
        .filter(|document| {
            document
                .uri
                .contains("/ergon/hephaestus/engineering/electrical-power-engineering/")
        })
        .filter(|document| !document.uri.contains("/sources/"))
        .flat_map(|document| &document.project.records)
        .filter(|record| record.slug.as_deref() != Some("pe-sources"))
        .map(|record| record.uid.clone())
        .collect();
    for (shared, continuation, original) in [
        ("ql-units", "pe-basis", "quantitative-literacy"),
        ("ql-energy", "pe-energy-account", "quantitative-literacy"),
        ("em-dc-network", "pe-circuits", "electromagnetism"),
        ("em-phasors", "pe-conventions", "electromagnetism"),
        ("em-impedance", "pe-complex-power", "electromagnetism"),
        ("em-inductance", "pe-transformers", "electromagnetism"),
        ("em-current-model", "pe-conductors", "electromagnetism"),
        (
            "rc-numericaltests",
            "pe-two-bus-example",
            "reproducible-computation",
        ),
        ("met-electrical-case", "pe-metering", "metrology"),
        ("met-time-frequency", "pe-pmu", "metrology"),
        ("em-quasistatic", "pe-fault-transient", "electromagnetism"),
        (
            "em-magnetic-matter",
            "pe-magnetic-design",
            "electromagnetism",
        ),
        ("em-electromechanics", "pe-synchronous", "electromagnetism"),
        ("em-poynting", "pe-dc-link", "electromagnetism"),
        (
            "ql-sensitivity",
            "met-model-discrepancy",
            "quantitative-literacy",
        ),
        ("met-model-discrepancy", "pe-uncertainty", "metrology"),
        ("met-conformity", "pe-quality-evidence", "metrology"),
    ] {
        let uid = records[shared].uid.clone();
        expected.insert(uid.clone());
        assert!(edges.contains(&(uid.clone(), records[continuation].uid.clone())));
        assert!(routes[original].iter().any(|(from, _)| *from == uid));
        assert!(
            records[shared]
                .body
                .contains("Continue in electrical power engineering:")
        );
    }
    let selected: BTreeSet<_> = edges
        .iter()
        .flat_map(|(from, to)| [from.clone(), to.clone()])
        .collect();
    assert_eq!(selected, expected);
    let mut reached = BTreeSet::new();
    let mut pending = vec![records["electrical-power-engineering"].uid.clone()];
    while let Some(uid) = pending.pop() {
        if reached.insert(uid.clone()) {
            pending.extend(
                edges
                    .iter()
                    .filter(|(from, _)| *from == uid)
                    .map(|(_, to)| to.clone()),
            );
        }
    }
    assert_eq!(reached, selected);
    assert_eq!(selected.len(), 149);
    for excluded in [
        "quantitative-literacy",
        "metrology",
        "reproducible-computation",
        "electromagnetism",
        "pe-sources",
        "pe-matpower",
        "pe-bichels",
    ] {
        assert!(!selected.contains(&records[excluded].uid));
    }
    for (predicate, route) in &routes {
        if *predicate != "electrical-power-engineering" {
            assert!(edges.is_disjoint(route));
        }
    }
    let examples = |publication: &Publication| {
        publication
            .documents
            .iter()
            .flat_map(|document| &document.project.records)
            .filter_map(|record| {
                let file = match record.slug.as_deref()? {
                    "pe-two-bus-example" => "two_bus_ac.py",
                    "pe-dc-dispatch-example" => "three_bus_dispatch.py",
                    "pe-swing-example" => "fault_clearing_swing.py",
                    _ => return None,
                };
                Some((
                    file.to_owned(),
                    record
                        .body
                        .split(&format!("```python file={file}\n"))
                        .nth(1)
                        .unwrap()
                        .split("\n```")
                        .next()
                        .unwrap()
                        .to_owned(),
                ))
            })
            .collect::<BTreeMap<_, _>>()
    };
    let original_examples = examples(&publication);
    assert_eq!(original_examples.len(), 3);
    let references = external_references(&publication);
    assert!(!references.is_empty());
    assert!(!directory.join("books").exists());
    assert!(!directory.join("views").exists());
    let destination = temporary.path().join("export");
    let exported = publication.export(&library, &destination, false).unwrap();
    let entry = exported.strip_prefix(&destination).unwrap().to_owned();
    let moved = temporary.path().join("moved");
    fs::rename(destination, &moved).unwrap();
    let relocated_library = Library::new(temporary.path().join("relocated-library"));
    let reopened = Publication::load(
        &relocated_library,
        moved.join(entry).to_str().unwrap(),
        &[],
        false,
    )
    .unwrap();
    assert_eq!(reopened.documents.len(), expected_documents);
    assert_eq!(external_references(&reopened), references);
    assert_eq!(examples(&reopened), original_examples);
    for (predicate, original) in routes {
        assert_eq!(learning_edges(&reopened, predicate), original);
    }
    let locations: BTreeMap<_, _> = reopened
        .documents
        .iter()
        .flat_map(|document| {
            document
                .project
                .records
                .iter()
                .map(|record| (record.uid.as_str(), document.base.as_str()))
        })
        .collect();
    for record in reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
    {
        for assertion in &record.assertions {
            if predicates.contains(&assertion.predicate.as_str()) {
                let (location, uid) = assertion
                    .object_uri
                    .as_ref()
                    .unwrap()
                    .rsplit_once('#')
                    .unwrap();
                assert_eq!(location, locations[uid]);
            }
        }
    }
    assert!(!moved.join("files").exists());
}

#[test]
fn thermodynamics_preserves_six_routes_and_four_original_examples_after_relocation() {
    let trails = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../institute/trails");
    let directory = trails.join("ergon/hephaestus/physics/thermodynamics-and-heat-transfer");
    let expected_documents = [
        directory.clone(),
        trails.join("ergon/hephaestus/physics/electromagnetism"),
        trails.join("ergon/hephaestus/chemistry/general-chemistry"),
        trails.join("ergon/hephaestus/inquiry/quantitative-literacy"),
        trails.join("ergon/hephaestus/inquiry/metrology"),
        trails.join("ergon/hephaestus/inquiry/reproducible-computation"),
    ]
    .iter()
    .map(|path| Publication::files(path).unwrap().len())
    .sum::<usize>();
    let temporary = tempfile::tempdir().unwrap();
    let library = Library::new(temporary.path().join("library"));
    let audit = Publication::audit(&library, &directory, &[]).unwrap();
    assert!(audit.issues.is_empty(), "{:?}", audit.issues);
    assert!(
        !audit
            .unchecked
            .iter()
            .any(|issue| issue.contains("directed navigation path")),
        "{:?}",
        audit.unchecked
    );
    for entry in [
        "Thermodynamics-and-heat-transfer.lingua",
        "computation/An-inspectable-finite-body-work-and-entropy-account.lingua",
        "computation/An-inspectable-conservative-transient-layered-slab.lingua",
        "research/Thermoreflectance-film-interface-and-transducer-identification.lingua",
        "sources/Thermodynamics-and-Chemistry-DeVoe.lingua",
    ] {
        let publication = Publication::load(
            &library,
            directory.join(entry).to_str().unwrap(),
            &[],
            false,
        )
        .unwrap();
        assert_eq!(publication.documents.len(), expected_documents, "{entry}");
        assert!(publication.parameters.is_empty());
        assert!(publication.instances.is_empty());
        assert!(publication.resources().unwrap().is_empty());
        assert!(publication.documents.iter().all(|document| {
            document.project.frequencies.is_empty() && document.project.rules.is_empty()
        }));
    }
    let publication = Publication::load(
        &library,
        directory
            .join("Thermodynamics-and-heat-transfer.lingua")
            .to_str()
            .unwrap(),
        &[],
        false,
    )
    .unwrap();
    let records: BTreeMap<_, _> = publication
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .map(|record| (record.slug.as_deref().unwrap(), record))
        .collect();
    assert_eq!(
        records["thermodynamics-and-heat-transfer"].uid,
        "r_6KQBZNJM6WNV9A69R3DZY1NH7E"
    );
    let predicates = [
        "quantitative-literacy",
        "metrology",
        "reproducible-computation",
        "electromagnetism",
        "general-chemistry",
        "thermodynamics-and-heat-transfer",
    ];
    let routes: BTreeMap<_, _> = predicates
        .into_iter()
        .map(|predicate| (predicate, learning_edges(&publication, predicate)))
        .collect();
    let edges = &routes["thermodynamics-and-heat-transfer"];
    let mut expected: BTreeSet<_> = publication
        .documents
        .iter()
        .filter(|document| {
            document
                .uri
                .contains("/ergon/hephaestus/physics/thermodynamics-and-heat-transfer/")
        })
        .filter(|document| !document.uri.contains("/sources/"))
        .flat_map(|document| &document.project.records)
        .filter(|record| record.slug.as_deref() != Some("th-sources"))
        .map(|record| record.uid.clone())
        .collect();
    for (shared, continuation, original) in [
        ("ql-units", "th-scales", "quantitative-literacy"),
        ("ql-energy", "th-energy", "quantitative-literacy"),
        ("ql-balances", "th-control", "quantitative-literacy"),
        (
            "ql-small-changes",
            "th-mathematics",
            "quantitative-literacy",
        ),
        ("ql-dimensionless", "th-similarity", "quantitative-literacy"),
        (
            "ql-discretization",
            "th-discretization",
            "quantitative-literacy",
        ),
        ("ql-identifiability", "th-inverse", "quantitative-literacy"),
        (
            "ql-sensitivity",
            "met-model-discrepancy",
            "quantitative-literacy",
        ),
        ("met-temperature", "th-measurement", "metrology"),
        ("met-optical", "th-infrared", "metrology"),
        ("met-model-discrepancy", "th-uncertainty", "metrology"),
        (
            "rc-numericaltests",
            "th-slab-example",
            "reproducible-computation",
        ),
        ("gc-energy", "th-conventions", "general-chemistry"),
        ("gc-calorimetry", "th-capacity", "general-chemistry"),
        ("gc-gases", "th-eos", "general-chemistry"),
        ("gc-entropy", "th-entropy-balance", "general-chemistry"),
        ("gc-gibbs", "th-potentials", "general-chemistry"),
        ("gc-phases", "th-phase-properties", "general-chemistry"),
        ("gc-chemical-potential", "th-mixtures", "general-chemistry"),
        ("em-fresnel", "th-spectral", "electromagnetism"),
    ] {
        let uid = records[shared].uid.clone();
        expected.insert(uid.clone());
        assert!(edges.contains(&(uid.clone(), records[continuation].uid.clone())));
        assert!(routes[original].iter().any(|(from, _)| *from == uid));
        assert!(
            records[shared]
                .body
                .contains("Continue in thermodynamics and heat transfer:")
        );
    }
    let selected: BTreeSet<_> = edges
        .iter()
        .flat_map(|(from, to)| [from.clone(), to.clone()])
        .collect();
    assert_eq!(selected, expected);
    let mut reached = BTreeSet::new();
    let mut pending = vec![records["thermodynamics-and-heat-transfer"].uid.clone()];
    while let Some(uid) = pending.pop() {
        if reached.insert(uid.clone()) {
            pending.extend(
                edges
                    .iter()
                    .filter(|(from, _)| *from == uid)
                    .map(|(_, to)| to.clone()),
            );
        }
    }
    assert_eq!(reached, selected);
    assert_eq!(selected.len(), 160);
    for excluded in [
        "quantitative-literacy",
        "metrology",
        "reproducible-computation",
        "general-chemistry",
        "electromagnetism",
        "th-sources",
        "th-yan",
        "th-heatpipe-handbook",
    ] {
        assert!(!selected.contains(&records[excluded].uid));
    }
    for (predicate, route) in &routes {
        if *predicate != "thermodynamics-and-heat-transfer" {
            assert!(edges.is_disjoint(route));
        }
    }
    let examples = |publication: &Publication| {
        publication
            .documents
            .iter()
            .flat_map(|document| &document.project.records)
            .filter_map(|record| {
                let file = match record.slug.as_deref()? {
                    "th-reservoir-example" => "finite_thermal_bodies.py",
                    "th-slab-example" => "layered_slab.py",
                    "th-radiation-example" => "grey_enclosure.py",
                    "th-exchanger-example" => "counterflow_exchanger.py",
                    _ => return None,
                };
                Some((
                    file.to_owned(),
                    record
                        .body
                        .split(&format!("```python file={file}\n"))
                        .nth(1)
                        .unwrap()
                        .split("\n```")
                        .next()
                        .unwrap()
                        .to_owned(),
                ))
            })
            .collect::<BTreeMap<_, _>>()
    };
    let original_examples = examples(&publication);
    assert_eq!(original_examples.len(), 4);
    let references = external_references(&publication);
    assert!(!references.is_empty());
    assert!(!directory.join("books").exists());
    assert!(!directory.join("views").exists());
    let destination = temporary.path().join("export");
    let exported = publication.export(&library, &destination, false).unwrap();
    let entry = exported.strip_prefix(&destination).unwrap().to_owned();
    let moved = temporary.path().join("moved");
    fs::rename(destination, &moved).unwrap();
    let relocated_library = Library::new(temporary.path().join("relocated-library"));
    let reopened = Publication::load(
        &relocated_library,
        moved.join(entry).to_str().unwrap(),
        &[],
        false,
    )
    .unwrap();
    assert_eq!(reopened.documents.len(), expected_documents);
    assert_eq!(external_references(&reopened), references);
    assert_eq!(examples(&reopened), original_examples);
    for (predicate, original) in routes {
        assert_eq!(learning_edges(&reopened, predicate), original);
    }
    let locations: BTreeMap<_, _> = reopened
        .documents
        .iter()
        .flat_map(|document| {
            document
                .project
                .records
                .iter()
                .map(|record| (record.uid.as_str(), document.base.as_str()))
        })
        .collect();
    for record in reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
    {
        for assertion in &record.assertions {
            if predicates.contains(&assertion.predicate.as_str()) {
                let (location, uid) = assertion
                    .object_uri
                    .as_ref()
                    .unwrap()
                    .rsplit_once('#')
                    .unwrap();
                assert_eq!(location, locations[uid]);
            }
        }
    }
    assert!(!moved.join("files").exists());
}

#[test]
fn fluid_dynamics_preserves_seven_routes_and_four_original_examples_after_relocation() {
    let trails = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../institute/trails");
    let directory = trails.join("ergon/hephaestus/physics/fluid-dynamics");
    let expected_documents = [
        directory.clone(),
        trails.join("ergon/hephaestus/physics/thermodynamics-and-heat-transfer"),
        trails.join("ergon/hephaestus/physics/electromagnetism"),
        trails.join("ergon/hephaestus/chemistry/general-chemistry"),
        trails.join("ergon/hephaestus/inquiry/quantitative-literacy"),
        trails.join("ergon/hephaestus/inquiry/metrology"),
        trails.join("ergon/hephaestus/inquiry/reproducible-computation"),
    ]
    .iter()
    .map(|path| Publication::files(path).unwrap().len())
    .sum::<usize>();
    let temporary = tempfile::tempdir().unwrap();
    let library = Library::new(temporary.path().join("library"));
    let audit = Publication::audit(&library, &directory, &[]).unwrap();
    assert!(audit.issues.is_empty(), "{:?}", audit.issues);
    assert!(
        !audit
            .unchecked
            .iter()
            .any(|issue| issue.contains("directed navigation path")),
        "{:?}",
        audit.unchecked
    );
    for entry in [
        "Fluid-dynamics.lingua",
        "computation/An-inspectable-nozzle-with-an-internal-normal-shock.lingua",
        "computation/An-inspectable-conservative-wet-shallow-water-solver.lingua",
        "research/Advanced-rheometry-constitutive-inference-and-processing-flow-study.lingua",
        "sources/Mecanica-dos-Fluidos-e-Transportes-Gobbi-Dias.lingua",
    ] {
        let publication = Publication::load(
            &library,
            directory.join(entry).to_str().unwrap(),
            &[],
            false,
        )
        .unwrap();
        assert_eq!(publication.documents.len(), expected_documents, "{entry}");
        assert!(publication.parameters.is_empty());
        assert!(publication.instances.is_empty());
        assert!(publication.resources().unwrap().is_empty());
        assert!(publication.documents.iter().all(|document| {
            document.project.frequencies.is_empty() && document.project.rules.is_empty()
        }));
    }
    let publication = Publication::load(
        &library,
        directory.join("Fluid-dynamics.lingua").to_str().unwrap(),
        &[],
        false,
    )
    .unwrap();
    let records: BTreeMap<_, _> = publication
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .map(|record| (record.slug.as_deref().unwrap(), record))
        .collect();
    assert_eq!(
        records["fluid-dynamics"].uid,
        "r_7PQW1WDHK708BFV23MD8ET61VZ"
    );
    let predicates = [
        "quantitative-literacy",
        "metrology",
        "reproducible-computation",
        "electromagnetism",
        "general-chemistry",
        "thermodynamics-and-heat-transfer",
        "fluid-dynamics",
    ];
    let routes: BTreeMap<_, _> = predicates
        .into_iter()
        .map(|predicate| (predicate, learning_edges(&publication, predicate)))
        .collect();
    let edges = &routes["fluid-dynamics"];
    let mut expected: BTreeSet<_> = publication
        .documents
        .iter()
        .filter(|document| {
            document
                .uri
                .contains("/ergon/hephaestus/physics/fluid-dynamics/")
        })
        .filter(|document| !document.uri.contains("/sources/"))
        .flat_map(|document| &document.project.records)
        .filter(|record| record.slug.as_deref() != Some("fd-sources"))
        .map(|record| record.uid.clone())
        .collect();
    for (shared, continuation, original) in [
        ("ql-units", "fd-vocabulary", "quantitative-literacy"),
        ("ql-vectors", "ql-small-changes", "quantitative-literacy"),
        ("ql-small-changes", "fd-fields", "quantitative-literacy"),
        (
            "ql-balances",
            "fd-transport-theorem",
            "quantitative-literacy",
        ),
        (
            "ql-dimensionless",
            "fd-dimensionless",
            "quantitative-literacy",
        ),
        ("ql-scaling", "fd-model-similarity", "quantitative-literacy"),
        (
            "ql-discretization",
            "fd-finite-volume",
            "quantitative-literacy",
        ),
        ("ql-identifiability", "fd-inverse", "quantitative-literacy"),
        ("met-force-pressure", "fd-pressure", "metrology"),
        ("met-mass-flow", "fd-flow-measurement", "metrology"),
        ("met-dynamic", "fd-dynamic-evidence", "metrology"),
        ("met-optical", "fd-piv-methods", "metrology"),
        ("met-model-discrepancy", "fd-uncertainty", "metrology"),
        (
            "rc-numericaltests",
            "fd-fluid-verification",
            "reproducible-computation",
        ),
        ("em-calculus", "fd-tensors", "electromagnetism"),
        (
            "th-control",
            "fd-mechanical-energy",
            "thermodynamics-and-heat-transfer",
        ),
        (
            "th-local-entropy",
            "fd-compressibility",
            "thermodynamics-and-heat-transfer",
        ),
        (
            "th-property-model",
            "fd-sound",
            "thermodynamics-and-heat-transfer",
        ),
        (
            "th-fluid-energy",
            "fd-scalar-transport",
            "thermodynamics-and-heat-transfer",
        ),
    ] {
        let uid = records[shared].uid.clone();
        expected.insert(uid.clone());
        assert!(edges.contains(&(uid.clone(), records[continuation].uid.clone())));
        assert!(routes[original].iter().any(|(from, _)| *from == uid));
        assert!(records[shared].body.contains("Continue in fluid dynamics:"));
    }
    let selected: BTreeSet<_> = edges
        .iter()
        .flat_map(|(from, to)| [from.clone(), to.clone()])
        .collect();
    assert_eq!(selected, expected);
    let mut reached = BTreeSet::new();
    let mut pending = vec![records["fluid-dynamics"].uid.clone()];
    while let Some(uid) = pending.pop() {
        if reached.insert(uid.clone()) {
            pending.extend(
                edges
                    .iter()
                    .filter(|(from, _)| *from == uid)
                    .map(|(_, to)| to.clone()),
            );
        }
    }
    assert_eq!(reached, selected);
    assert_eq!(selected.len(), 167);
    for excluded in [
        "thermodynamics-and-heat-transfer",
        "quantitative-literacy",
        "metrology",
        "reproducible-computation",
        "general-chemistry",
        "electromagnetism",
        "fd-sources",
        "fd-tong",
        "fd-rheology",
    ] {
        assert!(!selected.contains(&records[excluded].uid));
    }
    for (predicate, route) in &routes {
        if *predicate != "fluid-dynamics" {
            assert!(edges.is_disjoint(route));
        }
    }
    let examples = |publication: &Publication| {
        publication
            .documents
            .iter()
            .flat_map(|document| &document.project.records)
            .filter_map(|record| {
                let file = match record.slug.as_deref()? {
                    "fd-pipe-example" => "pipe_operating_point.py",
                    "fd-nozzle-example" => "nozzle_with_shock.py",
                    "fd-blasius-example" => "blasius.py",
                    "fd-shallow-example" => "shallow_water.py",
                    _ => return None,
                };
                Some((
                    file.to_owned(),
                    record
                        .body
                        .split(&format!("```python file={file}\n"))
                        .nth(1)
                        .unwrap()
                        .split("\n```")
                        .next()
                        .unwrap()
                        .to_owned(),
                ))
            })
            .collect::<BTreeMap<_, _>>()
    };
    let original_examples = examples(&publication);
    assert_eq!(original_examples.len(), 4);
    let references = external_references(&publication);
    assert!(!references.is_empty());
    assert!(!directory.join("books").exists());
    assert!(!directory.join("views").exists());
    let destination = temporary.path().join("export");
    let exported = publication.export(&library, &destination, false).unwrap();
    let entry = exported.strip_prefix(&destination).unwrap().to_owned();
    let moved = temporary.path().join("moved");
    fs::rename(destination, &moved).unwrap();
    let relocated_library = Library::new(temporary.path().join("relocated-library"));
    let reopened = Publication::load(
        &relocated_library,
        moved.join(entry).to_str().unwrap(),
        &[],
        false,
    )
    .unwrap();
    assert_eq!(reopened.documents.len(), expected_documents);
    assert_eq!(external_references(&reopened), references);
    assert_eq!(examples(&reopened), original_examples);
    for (predicate, original) in routes {
        assert_eq!(learning_edges(&reopened, predicate), original);
    }
    let locations: BTreeMap<_, _> = reopened
        .documents
        .iter()
        .flat_map(|document| {
            document
                .project
                .records
                .iter()
                .map(|record| (record.uid.as_str(), document.base.as_str()))
        })
        .collect();
    for record in reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
    {
        for assertion in &record.assertions {
            if predicates.contains(&assertion.predicate.as_str()) {
                let (location, uid) = assertion
                    .object_uri
                    .as_ref()
                    .unwrap()
                    .rsplit_once('#')
                    .unwrap();
                assert_eq!(location, locations[uid]);
            }
        }
    }
    assert!(!moved.join("files").exists());
}

#[test]
fn mechanical_engineering_preserves_shared_routes_and_original_examples_after_relocation() {
    let trails = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../institute/trails");
    let directory =
        trails.join("ergon/hephaestus/engineering/mechanical-engineering-and-machine-design");
    let expected_documents = [
        directory.clone(),
        trails.join("ergon/hephaestus/inquiry/quantitative-literacy"),
        trails.join("ergon/hephaestus/inquiry/metrology"),
        trails.join("ergon/hephaestus/inquiry/reproducible-computation"),
        trails.join("ergon/hephaestus/chemistry/general-chemistry"),
        trails.join("ergon/gaia/earth/geology"),
        trails.join("ergon/gaia/earth/mineralogy-and-petrology"),
        trails.join("ergon/hephaestus/engineering/mining-and-mineral-engineering"),
        trails.join("ergon/hephaestus/materials/metals-and-metallurgical-production"),
    ]
    .iter()
    .map(|path| Publication::files(path).unwrap().len())
    .sum::<usize>();
    let temporary = tempfile::tempdir().unwrap();
    let library = Library::new(temporary.path().join("library"));
    let publication = Publication::load(
        &library,
        directory
            .join("Mechanical-engineering-and-machine-design.lingua")
            .to_str()
            .unwrap(),
        &[],
        false,
    )
    .unwrap();
    assert_eq!(publication.documents.len(), expected_documents);
    assert!(publication.parameters.is_empty());
    assert!(publication.instances.is_empty());
    assert!(publication.resources().unwrap().is_empty());
    let records: BTreeMap<_, _> = publication
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .map(|record| (record.slug.as_deref().unwrap(), record))
        .collect();
    let predicates = [
        "mechanical-engineering",
        "quantitative-literacy",
        "metrology",
        "reproducible-computation",
        "general-chemistry",
        "geology",
        "mineralogy-and-petrology",
        "mining-and-mineral-engineering",
        "metals-and-metallurgical-production",
    ];
    let routes: BTreeMap<_, _> = predicates
        .into_iter()
        .map(|predicate| (predicate, learning_edges(&publication, predicate)))
        .collect();
    let edges = &routes["mechanical-engineering"];
    let selected: BTreeSet<_> = edges
        .iter()
        .flat_map(|(from, to)| [from.clone(), to.clone()])
        .collect();
    let mut expected: BTreeSet<_> = publication
        .documents
        .iter()
        .filter(|document| {
            document
                .uri
                .contains("/engineering/mechanical-engineering-and-machine-design/")
                && !document.uri.contains("/sources/")
        })
        .flat_map(|document| &document.project.records)
        .filter(|record| record.slug.as_deref() != Some("mech-sources"))
        .map(|record| record.uid.clone())
        .collect();
    for (shared, continuation, original) in [
        ("ql-units", "mech-language", "quantitative-literacy"),
        ("ql-geometry", "mech-dimensions", "quantitative-literacy"),
        ("ql-vectors", "mech-force", "quantitative-literacy"),
        (
            "ql-small-changes",
            "mech-kinematics",
            "quantitative-literacy",
        ),
        ("ql-rates", "mech-initial-value", "quantitative-literacy"),
        ("ql-energy", "mech-work-energy", "quantitative-literacy"),
        (
            "metal-tensile",
            "mech-material-state",
            "metals-and-metallurgical-production",
        ),
        (
            "metal-fracture-impact",
            "mech-fracture-plastic",
            "metals-and-metallurgical-production",
        ),
        (
            "metal-fatigue",
            "mech-fatigue-models",
            "metals-and-metallurgical-production",
        ),
        (
            "met-dimensional-case",
            "mech-inspection-decision",
            "metrology",
        ),
        (
            "metal-joining",
            "mech-final-material-state",
            "metals-and-metallurgical-production",
        ),
    ] {
        expected.insert(records[shared].uid.clone());
        assert!(edges.contains(&(
            records[shared].uid.clone(),
            records[continuation].uid.clone()
        )));
        assert!(
            routes[original]
                .iter()
                .any(|(from, _)| *from == records[shared].uid)
        );
    }
    assert_eq!(selected, expected);
    let mut reached = BTreeSet::new();
    let mut pending = vec![records["mechanical-engineering"].uid.clone()];
    while let Some(uid) = pending.pop() {
        if reached.insert(uid.clone()) {
            pending.extend(
                edges
                    .iter()
                    .filter(|(from, _)| *from == uid)
                    .map(|(_, to)| to.clone()),
            );
        }
    }
    assert_eq!(reached, selected);
    let identities: BTreeSet<_> = records.values().map(|record| record.uid.clone()).collect();
    for entry in [
        "foundations/Read-machines-and-mechanical-design-from-zero.lingua",
        "computation/An-inspectable-thermoelastic-axial-bar.lingua",
        "computation/An-inspectable-feed-axis-motion-and-energy-account.lingua",
        "sources/Precision-Machine-Design-Hale-Thesis.lingua",
        "research/Independent-mechanical-investigation-and-revision.lingua",
    ] {
        let discovered = Publication::load(
            &library,
            directory.join(entry).to_str().unwrap(),
            &[],
            false,
        )
        .unwrap();
        assert_eq!(discovered.documents.len(), expected_documents, "{entry}");
        let found: BTreeSet<_> = discovered
            .documents
            .iter()
            .flat_map(|document| &document.project.records)
            .map(|record| record.uid.clone())
            .collect();
        assert_eq!(found, identities, "{entry}");
    }
    let examples: BTreeMap<_, _> = [
        "mech-beam-example",
        "mech-oscillator-example",
        "mech-bar-example",
        "mech-axis-example",
    ]
    .into_iter()
    .map(|slug| {
        let example = records[slug];
        let code = example
            .body
            .split("```python\n")
            .nth(1)
            .unwrap()
            .split("\n```")
            .next()
            .unwrap()
            .to_owned();
        (example.uid.clone(), code)
    })
    .collect();
    let references = external_references(&publication);
    let destination = temporary.path().join("export");
    let exported = publication.export(&library, &destination, false).unwrap();
    let entry = exported.strip_prefix(&destination).unwrap().to_owned();
    let moved = temporary.path().join("moved");
    fs::rename(destination, &moved).unwrap();
    let reopened = Publication::load(
        &Library::new(temporary.path().join("fresh-library")),
        moved.join(entry).to_str().unwrap(),
        &[],
        false,
    )
    .unwrap();
    assert_eq!(external_references(&reopened), references);
    for (predicate, original) in routes {
        assert_eq!(
            learning_edges(&reopened, predicate),
            original,
            "{predicate}"
        );
    }
    let locations: BTreeMap<_, _> = reopened
        .documents
        .iter()
        .flat_map(|document| {
            document
                .project
                .records
                .iter()
                .map(|record| (record.uid.as_str(), document.base.as_str()))
        })
        .collect();
    for record in reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
    {
        if let Some(code) = examples.get(&record.uid) {
            assert!(record.body.contains(code));
        }
        for assertion in &record.assertions {
            if predicates.contains(&assertion.predicate.as_str()) {
                let (location, uid) = assertion
                    .object_uri
                    .as_ref()
                    .unwrap()
                    .rsplit_once('#')
                    .unwrap();
                assert_eq!(location, locations[uid]);
            }
        }
    }
    assert!(!moved.join("files").exists());
}

#[test]
fn materials_engineering_preserves_shared_routes_and_original_examples_after_relocation() {
    let trails = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../institute/trails");
    let directory = trails.join("ergon/hephaestus/engineering/materials-engineering");
    let expected_documents = [
        directory.clone(),
        trails.join("ergon/hephaestus/physics/fluid-dynamics"),
        trails.join("ergon/hephaestus/physics/electromagnetism"),
        trails.join("ergon/hephaestus/physics/thermodynamics-and-heat-transfer"),
        trails.join("ergon/hephaestus/engineering/mechanical-engineering-and-machine-design"),
        trails.join("ergon/hephaestus/engineering/mining-and-mineral-engineering"),
        trails.join("ergon/hephaestus/materials/metals-and-metallurgical-production"),
        trails.join("ergon/hephaestus/inquiry/quantitative-literacy"),
        trails.join("ergon/hephaestus/inquiry/metrology"),
        trails.join("ergon/hephaestus/inquiry/reproducible-computation"),
        trails.join("ergon/hephaestus/chemistry/general-chemistry"),
        trails.join("ergon/gaia/earth/geology"),
        trails.join("ergon/gaia/earth/mineralogy-and-petrology"),
    ]
    .iter()
    .map(|path| Publication::files(path).unwrap().len())
    .sum::<usize>();
    let temporary = tempfile::tempdir().unwrap();
    let library = Library::new(temporary.path().join("library"));
    let publication = Publication::load(
        &library,
        directory
            .join("Materials-engineering.lingua")
            .to_str()
            .unwrap(),
        &[],
        false,
    )
    .unwrap();
    assert_eq!(publication.documents.len(), expected_documents);
    assert!(publication.parameters.is_empty());
    assert!(publication.instances.is_empty());
    assert!(publication.resources().unwrap().is_empty());
    let records: BTreeMap<_, _> = publication
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .map(|record| (record.slug.as_deref().unwrap(), record))
        .collect();
    assert_eq!(
        records["materials-engineering"].uid,
        "r_05BF5JM8141RKF1VT5998YFZQ9"
    );
    let predicates = [
        "materials-engineering",
        "fluid-dynamics",
        "electromagnetism",
        "thermodynamics-and-heat-transfer",
        "mechanical-engineering",
        "mining-and-mineral-engineering",
        "metals-and-metallurgical-production",
        "quantitative-literacy",
        "metrology",
        "reproducible-computation",
        "general-chemistry",
        "geology",
        "mineralogy-and-petrology",
    ];
    let routes: BTreeMap<_, _> = predicates
        .into_iter()
        .map(|predicate| (predicate, learning_edges(&publication, predicate)))
        .collect();
    let edges = &routes["materials-engineering"];
    let selected: BTreeSet<_> = edges
        .iter()
        .flat_map(|(from, to)| [from.clone(), to.clone()])
        .collect();
    let mut expected: BTreeSet<_> = publication
        .documents
        .iter()
        .filter(|document| {
            document.uri.contains("/engineering/materials-engineering/")
                && !document.uri.contains("/sources/")
        })
        .flat_map(|document| &document.project.records)
        .filter(|record| record.slug.as_deref() != Some("mat-sources"))
        .map(|record| record.uid.clone())
        .collect();
    for (shared, continuation, original) in [
        (
            "rc-paper",
            "mat-independent-investigation",
            "reproducible-computation",
        ),
        (
            "mech-design-alternatives",
            "mat-selection-uncertainty",
            "mechanical-engineering",
        ),
        (
            "mech-production-resources",
            "mat-service-state",
            "mechanical-engineering",
        ),
        (
            "mech-condition-evidence",
            "mech-maintenance",
            "mechanical-engineering",
        ),
        (
            "mech-maintenance",
            "mat-material-failure",
            "mechanical-engineering",
        ),
        (
            "mech-failure-investigation",
            "mat-material-repair",
            "mechanical-engineering",
        ),
        (
            "mech-restoration",
            "mat-material-reuse",
            "mechanical-engineering",
        ),
        (
            "mech-recovery",
            "mat-recovery-entry",
            "mechanical-engineering",
        ),
        (
            "metal-recycling",
            "mat-recovery-rates",
            "metals-and-metallurgical-production",
        ),
        ("gc-redox-balance", "gc-nernst", "general-chemistry"),
        ("gc-nernst", "mat-degradation-kinetics", "general-chemistry"),
        (
            "metal-electrode-kinetics",
            "mat-mixed-balance",
            "metals-and-metallurgical-production",
        ),
        ("gc-corrosion", "mat-local-corrosion", "general-chemistry"),
        (
            "metal-corrosion",
            "mat-hydrogen",
            "metals-and-metallurgical-production",
        ),
        ("mech-tribology", "metal-wear", "mechanical-engineering"),
        (
            "metal-wear",
            "mat-erosion",
            "metals-and-metallurgical-production",
        ),
        (
            "fd-cavitation-dynamics",
            "mat-coupled-damage",
            "fluid-dynamics",
        ),
        (
            "mech-process-plan",
            "mat-processing-balance",
            "mechanical-engineering",
        ),
        (
            "th-lumped",
            "mat-process-atmosphere",
            "thermodynamics-and-heat-transfer",
        ),
        (
            "metal-casting",
            "mat-shape-deformation",
            "metals-and-metallurgical-production",
        ),
        (
            "metal-forging-rolling",
            "metal-extrusion-drawing-sheet",
            "metals-and-metallurgical-production",
        ),
        (
            "metal-extrusion-drawing-sheet",
            "mat-thermal-processing",
            "metals-and-metallurgical-production",
        ),
        (
            "mech-surface-finishing",
            "mat-processing-rheology",
            "mechanical-engineering",
        ),
        (
            "fd-generalized-newtonian",
            "fd-objective-models",
            "fluid-dynamics",
        ),
        ("fd-objective-models", "fd-rheometry", "fluid-dynamics"),
        ("fd-rheometry", "mat-polymer-shaping", "fluid-dynamics"),
        ("fd-porous", "mat-composite-process", "fluid-dynamics"),
        (
            "metal-powder",
            "mat-slurry-processing",
            "metals-and-metallurgical-production",
        ),
        (
            "metal-joining",
            "mat-coatings",
            "metals-and-metallurgical-production",
        ),
        (
            "metal-additive",
            "mat-process-qualification",
            "metals-and-metallurgical-production",
        ),
        (
            "mech-process-variation",
            "mat-process-scaleup",
            "mechanical-engineering",
        ),
        (
            "th-statistical",
            "mat-capacity",
            "thermodynamics-and-heat-transfer",
        ),
        (
            "th-kinetic-transport",
            "th-interfaces-nano",
            "thermodynamics-and-heat-transfer",
        ),
        (
            "th-interfaces-nano",
            "mat-thermal-measure",
            "thermodynamics-and-heat-transfer",
        ),
        ("em-current-model", "em-hall", "electromagnetism"),
        ("em-hall", "mat-disordered-transport", "electromagnetism"),
        ("em-polarization", "em-matter-models", "electromagnetism"),
        ("em-matter-models", "em-loss", "electromagnetism"),
        ("em-loss", "mat-dielectric-loss", "electromagnetism"),
        ("em-magnetic-matter", "mat-exchange", "electromagnetism"),
        (
            "th-coupled",
            "mat-functional-inference",
            "thermodynamics-and-heat-transfer",
        ),
        (
            "metal-metallurgical-states",
            "mat-ceramic-families",
            "metals-and-metallurgical-production",
        ),
        (
            "mech-tensile-measures",
            "metal-tensile",
            "mechanical-engineering",
        ),
        (
            "metal-tensile",
            "mech-strain-field",
            "metals-and-metallurgical-production",
        ),
        (
            "mech-strain-field",
            "mech-stress-field",
            "mechanical-engineering",
        ),
        (
            "mech-stress-field",
            "mech-elastic-law",
            "mechanical-engineering",
        ),
        (
            "mech-elastic-law",
            "mat-elastic-energy",
            "mechanical-engineering",
        ),
        (
            "mech-time-dependent",
            "mat-hereditary",
            "mechanical-engineering",
        ),
        (
            "metal-dislocations",
            "metal-strengthening",
            "metals-and-metallurgical-production",
        ),
        (
            "metal-strengthening",
            "metal-precipitation-structure",
            "metals-and-metallurgical-production",
        ),
        (
            "metal-precipitation-structure",
            "mat-obstacle-comparison",
            "metals-and-metallurgical-production",
        ),
        (
            "mech-fracture-linear",
            "metal-fracture-impact",
            "mechanical-engineering",
        ),
        (
            "metal-fracture-impact",
            "mech-fracture-plastic",
            "metals-and-metallurgical-production",
        ),
        (
            "mech-fracture-plastic",
            "mat-toughening",
            "mechanical-engineering",
        ),
        (
            "metal-fatigue",
            "mat-cyclic-families",
            "metals-and-metallurgical-production",
        ),
        (
            "metal-creep",
            "mat-creep-damage",
            "metals-and-metallurgical-production",
        ),
        ("ql-units", "mat-language", "quantitative-literacy"),
        ("gc-energy", "mat-material-energy", "general-chemistry"),
        ("gc-entropy", "mat-potentials", "general-chemistry"),
        (
            "gc-chemical-potential",
            "mat-partial-properties",
            "general-chemistry",
        ),
        ("gc-arrhenius", "mat-random-walk", "general-chemistry"),
        (
            "metal-restoration",
            "mat-thermal-analysis",
            "metals-and-metallurgical-production",
        ),
        ("gc-matter", "mat-fractions", "general-chemistry"),
        ("met-measurand", "mat-representative", "metrology"),
        ("gc-mo", "mat-structure-entry", "general-chemistry"),
        ("gc-crystals", "mp-lattice", "general-chemistry"),
        ("mp-lattice", "mat-packing", "mineralogy-and-petrology"),
        ("mp-metric", "mp-indices", "mineralogy-and-petrology"),
        (
            "mp-indices",
            "mat-plane-mechanisms",
            "mineralogy-and-petrology",
        ),
        ("mp-symmetry", "mat-anisotropy", "mineralogy-and-petrology"),
        (
            "mp-bragg",
            "mat-structure-factor",
            "mineralogy-and-petrology",
        ),
        (
            "mp-cif",
            "mat-structure-evidence",
            "mineralogy-and-petrology",
        ),
    ] {
        expected.insert(records[shared].uid.clone());
        assert!(edges.contains(&(
            records[shared].uid.clone(),
            records[continuation].uid.clone()
        )));
        assert!(
            routes[original]
                .iter()
                .any(|(from, _)| *from == records[shared].uid)
        );
    }
    assert_eq!(selected, expected);
    let mut reached = BTreeSet::new();
    let mut pending = vec![records["materials-engineering"].uid.clone()];
    while let Some(uid) = pending.pop() {
        if reached.insert(uid.clone()) {
            pending.extend(
                edges
                    .iter()
                    .filter(|(from, _)| *from == uid)
                    .map(|(_, to)| to.clone()),
            );
        }
    }
    assert_eq!(reached, selected);
    let identities: BTreeSet<_> = records.values().map(|record| record.uid.clone()).collect();
    for entry in [
        "foundations/Read-materials-from-everyday-objects-to-research.lingua",
        "computation/An-inspectable-cubic-diffraction-and-basis-account.lingua",
        "sources/Mesoscale-Tensile-Blind-Predictions-AM-Bench.lingua",
        "selection/An-inspectable-beam-selection-example.lingua",
        "research/Materials-engineering-completion-and-specialist-continuations.lingua",
    ] {
        let discovered = Publication::load(
            &library,
            directory.join(entry).to_str().unwrap(),
            &[],
            false,
        )
        .unwrap();
        assert_eq!(discovered.documents.len(), expected_documents, "{entry}");
        let found: BTreeSet<_> = discovered
            .documents
            .iter()
            .flat_map(|document| &document.project.records)
            .map(|record| record.uid.clone())
            .collect();
        assert_eq!(found, identities, "{entry}");
    }
    let examples: BTreeMap<_, _> = [
        "mat-diffraction-example",
        "mat-regular-example",
        "mat-diffusion-example",
        "mat-history-example",
        "mat-bands-example",
        "mat-cure-example",
        "mat-corrosion-example",
    ]
    .into_iter()
    .map(|slug| {
        let example = records[slug];
        let code = example
            .body
            .split("```python\n")
            .nth(1)
            .unwrap()
            .split("\n```")
            .next()
            .unwrap()
            .to_owned();
        (example.uid.clone(), code)
    })
    .collect();
    for slug in [
        "metal-calphad",
        "metal-phase-book",
        "metal-cambridge",
        "mech-roylance",
        "mech-bower-solids",
        "mat-doitpoms-response",
        "mat-wood-handbook",
        "mat-ceramic-breviary",
        "th-nano",
        "mat-tong-quantum",
        "mat-tong-statistical",
        "mat-tong-solid",
        "mat-simon-solid",
        "mat-mit-functional",
        "mat-ralph-magnetism",
        "mat-doitpoms-functional",
        "mat-ifusp-solid",
        "mat-mit-processing",
        "mat-ou-manufacturing",
        "mat-rheo-cure",
        "mat-cure-inference",
        "mat-vcctl",
        "mat-mit-microfab",
        "mat-ald",
        "mat-aqueous",
        "mat-lister",
        "mat-npl-degradation",
        "mat-hydrogen-source",
        "mat-oxide-source",
        "mat-weather-protocol",
        "mat-weather-life",
        "mat-glass-corrosion",
        "mat-aar",
        "mat-nasa-durability",
        "mat-selection-mit",
        "mat-data-book",
        "mat-indices-book",
        "mat-selection-doe",
        "mat-ilcd",
        "mat-sustainable-metals",
        "mat-plastics-recovery",
        "mat-aggregate-recovery",
        "mat-atomistic-course",
        "mat-dft-precision",
        "mat-pfhub",
        "mat-am-residual",
        "mat-am-tensile",
        "mat-informatics",
    ] {
        assert!(records.contains_key(slug));
        assert!(!selected.contains(&records[slug].uid));
    }
    let references = external_references(&publication);
    let destination = temporary.path().join("export");
    let exported = publication.export(&library, &destination, false).unwrap();
    let entry = exported.strip_prefix(&destination).unwrap().to_owned();
    let moved = temporary.path().join("moved");
    fs::rename(destination, &moved).unwrap();
    let reopened = Publication::load(
        &Library::new(temporary.path().join("fresh-library")),
        moved.join(entry).to_str().unwrap(),
        &[],
        false,
    )
    .unwrap();
    assert_eq!(external_references(&reopened), references);
    for (predicate, original) in routes {
        assert_eq!(
            learning_edges(&reopened, predicate),
            original,
            "{predicate}"
        );
    }
    let locations: BTreeMap<_, _> = reopened
        .documents
        .iter()
        .flat_map(|document| {
            document
                .project
                .records
                .iter()
                .map(|record| (record.uid.as_str(), document.base.as_str()))
        })
        .collect();
    for record in reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
    {
        if let Some(code) = examples.get(&record.uid) {
            assert!(record.body.contains(code));
        }
        for assertion in &record.assertions {
            if predicates.contains(&assertion.predicate.as_str()) {
                let (location, uid) = assertion
                    .object_uri
                    .as_ref()
                    .unwrap()
                    .rsplit_once('#')
                    .unwrap();
                assert_eq!(location, locations[uid]);
            }
        }
    }
    assert!(!moved.join("files").exists());
}

#[test]
fn water_and_sanitation_preserves_its_selection_and_shared_routes_after_relocation() {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../institute/trails/ergon/hephaestus/utilities/water-and-sanitation");
    let temporary = tempfile::tempdir().unwrap();
    let library = Library::new(temporary.path().join("library"));
    let publication = Publication::load(
        &library,
        directory
            .join("Water-sanitation-and-industrial-utilities.lingua")
            .to_str()
            .unwrap(),
        &[],
        false,
    )
    .unwrap();
    assert!(publication.parameters.is_empty());
    assert!(publication.instances.is_empty());
    assert!(publication.resources().unwrap().is_empty());
    let records: BTreeMap<_, _> = publication
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .map(|record| (record.slug.as_deref().unwrap(), record))
        .collect();
    let edges = learning_edges(&publication, "water-and-sanitation");
    let selected: BTreeSet<_> = edges
        .iter()
        .flat_map(|(from, to)| [from.clone(), to.clone()])
        .collect();
    let mut expected: BTreeSet<_> = publication
        .documents
        .iter()
        .filter(|document| {
            document.uri.contains("/utilities/water-and-sanitation/")
                && !document.uri.contains("/sources/")
        })
        .flat_map(|document| &document.project.records)
        .filter(|record| record.slug.as_deref() != Some("wu-sources"))
        .map(|record| record.uid.clone())
        .collect();
    for slug in [
        "ql-units",
        "ql-balances",
        "gc-solubility",
        "gc-carbonate",
        "gc-activities",
        "gc-balances-equilibrium",
        "met-chemistry-case",
        "met-sampling-target",
        "met-sampling-uncertainty",
        "gc-rate-laws",
        "fd-fittings",
        "th-exchanger-reality",
        "pe-feedback-control",
    ] {
        expected.insert(records[slug].uid.clone());
    }
    assert_eq!(selected, expected);
    let mut reached = BTreeSet::new();
    let mut pending = vec![records["water-and-sanitation"].uid.clone()];
    while let Some(uid) = pending.pop() {
        if reached.insert(uid.clone()) {
            pending.extend(
                edges
                    .iter()
                    .filter(|(from, _)| *from == uid)
                    .map(|(_, to)| to.clone()),
            );
        }
    }
    assert_eq!(reached, selected);
    for (shared, next, original) in [
        ("gc-rate-laws", "wu-mixed-tank", "general-chemistry"),
        ("fd-fittings", "wu-treatment-dosing", "fluid-dynamics"),
        (
            "th-exchanger-reality",
            "wu-steam-balance",
            "thermodynamics-and-heat-transfer",
        ),
        (
            "pe-feedback-control",
            "wu-water-control",
            "electrical-power-engineering",
        ),
    ] {
        assert!(edges.contains(&(records[shared].uid.clone(), records[next].uid.clone())));
        assert!(
            learning_edges(&publication, original)
                .iter()
                .any(|(from, _)| *from == records[shared].uid)
        );
    }
    let identities: BTreeSet<_> = records.values().map(|record| record.uid.clone()).collect();
    for entry in [
        "industrial/An-inspectable-reuse-loop-and-concentrate-account.lingua",
        "operations/An-original-storage-and-service-history.lingua",
        "sources/Benchmark-Simulation-Model-2-IWA.lingua",
    ] {
        let discovered = Publication::load(
            &library,
            directory.join(entry).to_str().unwrap(),
            &[],
            false,
        )
        .unwrap();
        let discovered_identities: BTreeSet<_> = discovered
            .documents
            .iter()
            .flat_map(|document| &document.project.records)
            .map(|record| record.uid.clone())
            .collect();
        assert_eq!(discovered_identities, identities, "{entry}");
    }
    let predicates = [
        "water-and-sanitation",
        "general-chemistry",
        "fluid-dynamics",
        "quantitative-literacy",
        "metrology",
        "reproducible-computation",
        "geology",
        "electromagnetism",
        "electrical-power-engineering",
        "thermodynamics-and-heat-transfer",
    ];
    let routes: BTreeMap<_, _> = predicates
        .into_iter()
        .map(|predicate| (predicate, learning_edges(&publication, predicate)))
        .collect();
    let original_examples: BTreeMap<_, _> = [
        "wu-reactor-comparison",
        "wu-distribution-example",
        "wu-biomass-example",
        "wu-batch-example",
        "wu-reuse-example",
        "wu-service-example",
    ]
    .into_iter()
    .map(|slug| {
        (
            records[slug].uid.clone(),
            records[slug]
                .body
                .split("```python\n")
                .nth(1)
                .unwrap()
                .split("\n```")
                .next()
                .unwrap()
                .to_owned(),
        )
    })
    .collect();
    let references = external_references(&publication);
    let destination = temporary.path().join("export");
    let exported = publication.export(&library, &destination, false).unwrap();
    let entry = exported.strip_prefix(&destination).unwrap().to_owned();
    let moved = temporary.path().join("moved");
    fs::rename(destination, &moved).unwrap();
    let reopened = Publication::load(
        &Library::new(temporary.path().join("fresh-library")),
        moved.join(entry).to_str().unwrap(),
        &[],
        false,
    )
    .unwrap();
    assert_eq!(external_references(&reopened), references);
    for (predicate, original) in routes {
        assert_eq!(
            learning_edges(&reopened, predicate),
            original,
            "{predicate}"
        );
    }
    let locations: BTreeMap<_, _> = reopened
        .documents
        .iter()
        .flat_map(|document| {
            document
                .project
                .records
                .iter()
                .map(|record| (record.uid.as_str(), document.base.as_str()))
        })
        .collect();
    for record in reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
    {
        if let Some(code) = original_examples.get(&record.uid) {
            assert!(record.body.contains(code));
        }
        for assertion in &record.assertions {
            if predicates.contains(&assertion.predicate.as_str()) {
                let (location, uid) = assertion
                    .object_uri
                    .as_ref()
                    .unwrap()
                    .rsplit_once('#')
                    .unwrap();
                assert_eq!(location, locations[uid]);
            }
        }
    }
    assert!(!moved.join("files").exists());
}

#[test]
fn industrial_gases_preserves_the_complete_route_and_evidence_after_relocation() {
    let trails = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../institute/trails");
    let directory =
        trails.join("ergon/hephaestus/utilities/industrial-gases-vacuum-and-high-purity-supplies");
    let expected_documents = [
        directory.clone(),
        trails.join("ergon/hephaestus/engineering/materials-engineering"),
        trails.join("ergon/hephaestus/engineering/mechanical-engineering-and-machine-design"),
        trails.join("ergon/hephaestus/engineering/mining-and-mineral-engineering"),
        trails.join("ergon/hephaestus/materials/metals-and-metallurgical-production"),
        trails.join("ergon/hephaestus/materials/chemical-feedstocks-and-supply-systems"),
        trails.join("ergon/hephaestus/physics/fluid-dynamics"),
        trails.join("ergon/hephaestus/physics/electromagnetism"),
        trails.join("ergon/hephaestus/physics/thermodynamics-and-heat-transfer"),
        trails.join("ergon/hephaestus/inquiry/quantitative-literacy"),
        trails.join("ergon/hephaestus/inquiry/metrology"),
        trails.join("ergon/hephaestus/inquiry/reproducible-computation"),
        trails.join("ergon/hephaestus/chemistry/general-chemistry"),
        trails.join("ergon/hephaestus/chemistry/industrial-chemistry"),
        trails.join("ergon/gaia/earth/geology"),
        trails.join("ergon/gaia/earth/mineralogy-and-petrology"),
    ]
    .iter()
    .map(|path| Publication::files(path).unwrap().len())
    .sum::<usize>();
    let temporary = tempfile::tempdir().unwrap();
    let library = Library::new(temporary.path().join("library"));
    let publication = Publication::load(
        &library,
        directory
            .join("Industrial-gases-and-vacuum.lingua")
            .to_str()
            .unwrap(),
        &[],
        false,
    )
    .unwrap();
    assert_eq!(publication.documents.len(), expected_documents);
    assert!(publication.parameters.is_empty());
    assert!(publication.instances.is_empty());
    assert!(publication.resources().unwrap().is_empty());
    assert!(publication.documents.iter().all(
        |document| document.project.frequencies.is_empty() && document.project.rules.is_empty()
    ));
    let records: BTreeMap<_, _> = publication
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
        .map(|record| (record.slug.as_deref().unwrap(), record))
        .collect();
    assert_eq!(
        records["industrial-gases-and-vacuum"].uid,
        "r_3GHQJEYMYEN6JDKSF4PPJ403RF"
    );
    let predicates = [
        "industrial-gases-and-vacuum",
        "materials-engineering",
        "mechanical-engineering",
        "mining-and-mineral-engineering",
        "metals-and-metallurgical-production",
        "chemical-supply-systems",
        "fluid-dynamics",
        "electromagnetism",
        "thermodynamics-and-heat-transfer",
        "quantitative-literacy",
        "metrology",
        "reproducible-computation",
        "general-chemistry",
        "industrial-chemistry",
        "geology",
        "mineralogy-and-petrology",
    ];
    let routes: BTreeMap<_, _> = predicates
        .into_iter()
        .map(|predicate| (predicate, learning_edges(&publication, predicate)))
        .collect();
    let edges = &routes["industrial-gases-and-vacuum"];
    let selected: BTreeSet<_> = edges
        .iter()
        .flat_map(|(from, to)| [from.clone(), to.clone()])
        .collect();
    let mut expected: BTreeSet<_> = publication
        .documents
        .iter()
        .filter(|document| {
            document
                .uri
                .contains("/utilities/industrial-gases-vacuum-and-high-purity-supplies/")
                && !document.uri.contains("/sources/")
        })
        .flat_map(|document| &document.project.records)
        .filter(|record| record.slug.as_deref() != Some("gas-sources"))
        .map(|record| record.uid.clone())
        .collect();
    assert_eq!(expected.len(), 224);
    for (shared, continuation, original) in [
        (
            "cs-common-cause",
            "gas-redundancy",
            "chemical-supply-systems",
        ),
        (
            "cs-gas-supply",
            "gas-supply-boundary",
            "chemical-supply-systems",
        ),
        ("cs-loading", "gas-cryo-transfer", "chemical-supply-systems"),
        ("fd-compressible-energy", "fd-nozzles", "fluid-dynamics"),
        ("fd-nozzles", "gas-delivery-choking", "fluid-dynamics"),
        ("fd-porous", "gas-bed-pressure", "fluid-dynamics"),
        ("fd-pressure", "gas-pressure", "fluid-dynamics"),
        ("gc-arrhenius", "gas-residence", "general-chemistry"),
        (
            "ic-adsorption-membranes",
            "gas-adsorption-entry",
            "industrial-chemistry",
        ),
        (
            "ic-air-gases",
            "gas-production-entry",
            "industrial-chemistry",
        ),
        ("ic-control", "gas-operating-states", "industrial-chemistry"),
        ("ic-high-purity", "gas-specialty", "industrial-chemistry"),
        (
            "ic-hydrogen",
            "gas-hydrogen-service",
            "industrial-chemistry",
        ),
        (
            "ic-phase-equilibrium",
            "gas-phase-separation",
            "industrial-chemistry",
        ),
        ("ic-quantities", "gas-amount", "industrial-chemistry"),
        (
            "mat-diffusion-example",
            "gas-bulk-diffusion",
            "materials-engineering",
        ),
        (
            "mat-diffusion-law",
            "mat-diffusion-solutions",
            "materials-engineering",
        ),
        (
            "mat-diffusion-solutions",
            "mat-diffusion-example",
            "materials-engineering",
        ),
        (
            "mech-commissioning",
            "gas-commissioning",
            "mechanical-engineering",
        ),
        (
            "mech-condition-evidence",
            "gas-condition",
            "mechanical-engineering",
        ),
        (
            "mech-failure-investigation",
            "gas-failure-evidence",
            "mechanical-engineering",
        ),
        (
            "mech-human-access",
            "gas-failure-modes",
            "mechanical-engineering",
        ),
        (
            "mech-maintenance",
            "gas-maintenance",
            "mechanical-engineering",
        ),
        (
            "mech-restoration",
            "gas-restoration",
            "mechanical-engineering",
        ),
        ("mech-seals", "gas-vacuum-joints", "mechanical-engineering"),
        ("met-conformity", "gas-measurement-decision", "metrology"),
        ("met-inverse-bayesian", "gas-spectrum-inverse", "metrology"),
        ("met-measurand", "gas-trace-evidence", "metrology"),
        ("met-regression", "met-regression-inverse", "metrology"),
        ("met-regression-inverse", "gas-gauge-factor", "metrology"),
        ("met-response", "gas-force-gauges", "metrology"),
        ("met-sensitivity", "gas-measurement-budget", "metrology"),
        ("met-trace-chain", "gas-static-standard", "metrology"),
        ("ql-units", "fd-pressure", "quantitative-literacy"),
        (
            "rc-numericaltests",
            "gas-study-adsorption",
            "reproducible-computation",
        ),
        ("rc-paper", "gas-reproduce", "reproducible-computation"),
        (
            "th-coexistence",
            "gas-phase-inventory",
            "thermodynamics-and-heat-transfer",
        ),
        (
            "th-control",
            "gas-storage-balance",
            "thermodynamics-and-heat-transfer",
        ),
        (
            "th-flow-devices",
            "gas-compression",
            "thermodynamics-and-heat-transfer",
        ),
        (
            "th-fourier",
            "gas-heat-paths",
            "thermodynamics-and-heat-transfer",
        ),
        (
            "th-statistical",
            "gas-kinetics",
            "thermodynamics-and-heat-transfer",
        ),
    ] {
        expected.insert(records[shared].uid.clone());
        assert!(
            edges.contains(&(
                records[shared].uid.clone(),
                records[continuation].uid.clone()
            )),
            "{shared}"
        );
        assert!(
            routes[original]
                .iter()
                .any(|(from, _)| *from == records[shared].uid),
            "{shared}"
        );
    }
    assert_eq!(selected, expected);
    assert_eq!(selected.len(), 265);
    let mut reached = BTreeSet::new();
    let mut pending = vec![records["industrial-gases-and-vacuum"].uid.clone()];
    while let Some(uid) = pending.pop() {
        if reached.insert(uid.clone()) {
            pending.extend(
                edges
                    .iter()
                    .filter(|(from, _)| *from == uid)
                    .map(|(_, to)| to.clone()),
            );
        }
    }
    assert_eq!(reached, selected);
    assert!(
        !edges
            .iter()
            .any(|(from, _)| *from == records["gas-completion"].uid)
    );
    for (start, branches, end) in [
        (
            "gas-stateful-collisions",
            [
                "gas-study-measure-pressure",
                "gas-study-measure-flow",
                "gas-study-measure-composition",
            ],
            "gas-measurement-review",
        ),
        (
            "gas-service-knowledge",
            [
                "gas-study-operation",
                "gas-study-failure",
                "gas-study-recovery",
            ],
            "gas-operation-review",
        ),
        (
            "gas-integrated-entry",
            [
                "gas-study-clean-system",
                "gas-study-helium-system",
                "gas-study-air-system",
            ],
            "gas-integrated-review",
        ),
    ] {
        for branch in branches {
            assert!(edges.contains(&(records[start].uid.clone(), records[branch].uid.clone())));
            assert!(edges.contains(&(records[branch].uid.clone(), records[end].uid.clone())));
        }
    }
    let own_sources: Vec<_> = publication
        .documents
        .iter()
        .filter(|document| {
            document
                .uri
                .contains("/utilities/industrial-gases-vacuum-and-high-purity-supplies/sources/")
        })
        .flat_map(|document| &document.project.records)
        .collect();
    assert_eq!(own_sources.len(), 35);
    assert!(
        own_sources
            .iter()
            .all(|record| !selected.contains(&record.uid) && record.body.contains("](https://"))
    );
    for slug in [
        "gas-sources",
        "mechanical-engineering",
        "materials-engineering",
        "metrology",
        "rc-completion",
    ] {
        assert!(!selected.contains(&records[slug].uid), "{slug}");
    }
    let identities: BTreeSet<_> = records.values().map(|record| record.uid.clone()).collect();
    for entry in [
        "foundations/Read-gas-and-vacuum-systems-from-everyday-use-to-research.lingua",
        "computation/An-inspectable-network-pumpdown-and-gas-balance.lingua",
        "sources/Reliability-Centered-Maintenance-NASA.lingua",
        "recovery/A-lower-pressure-and-a-purer-refill-are-different-objectives.lingua",
        "research/Completion-and-continuing-gas-and-vacuum-studies.lingua",
    ] {
        let discovered = Publication::load(
            &library,
            directory.join(entry).to_str().unwrap(),
            &[],
            false,
        )
        .unwrap();
        assert_eq!(discovered.documents.len(), expected_documents, "{entry}");
        let found: BTreeSet<_> = discovered
            .documents
            .iter()
            .flat_map(|document| &document.project.records)
            .map(|record| record.uid.clone())
            .collect();
        assert_eq!(found, identities, "{entry}");
    }
    let examples = |p: &Publication| -> BTreeMap<String, Vec<String>> {
        p.documents
            .iter()
            .flat_map(|document| &document.project.records)
            .filter_map(|record| {
                let blocks: Vec<_> = record
                    .body
                    .split("```python\n")
                    .skip(1)
                    .map(|block| block.split("\n```").next().unwrap().to_owned())
                    .collect();
                (!blocks.is_empty()).then(|| (record.uid.clone(), blocks))
            })
            .collect()
    };
    let original_examples = examples(&publication);
    assert_eq!(
        original_examples[&records["gas-network-example"].uid].len(),
        1
    );
    assert!(original_examples.contains_key(&records["mat-diffusion-example"].uid));
    let references = external_references(&publication);
    let destination = temporary.path().join("export");
    let exported = publication.export(&library, &destination, false).unwrap();
    let entry = exported.strip_prefix(&destination).unwrap().to_owned();
    let moved = temporary.path().join("moved");
    fs::rename(destination, &moved).unwrap();
    let reopened = Publication::load(
        &Library::new(temporary.path().join("fresh-library")),
        moved.join(entry).to_str().unwrap(),
        &[],
        false,
    )
    .unwrap();
    assert_eq!(reopened.documents.len(), expected_documents);
    assert_eq!(external_references(&reopened), references);
    assert_eq!(examples(&reopened), original_examples);
    for (predicate, original) in routes {
        assert_eq!(
            learning_edges(&reopened, predicate),
            original,
            "{predicate}"
        );
    }
    let locations: BTreeMap<_, _> = reopened
        .documents
        .iter()
        .flat_map(|document| {
            document
                .project
                .records
                .iter()
                .map(|record| (record.uid.as_str(), document.base.as_str()))
        })
        .collect();
    for record in reopened
        .documents
        .iter()
        .flat_map(|document| &document.project.records)
    {
        for assertion in &record.assertions {
            if predicates.contains(&assertion.predicate.as_str())
                || (assertion.predicate == "source" && assertion.object_uri.is_some())
            {
                let (location, uid) = assertion
                    .object_uri
                    .as_ref()
                    .unwrap()
                    .rsplit_once('#')
                    .unwrap();
                assert_eq!(location, locations[uid]);
            }
        }
    }
    assert!(!moved.join("files").exists());
}
