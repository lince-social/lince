// non-lean-yet

#![cfg(feature = "publication")]

mod ergon {
    pub(crate) mod chemistry;
    pub(crate) mod earth;
    pub(crate) mod engineering;
    pub(crate) mod fixture;
    pub(crate) mod hephaestus;
    pub(crate) mod inquiry;
    pub(crate) mod physics;
}

use ergon::{chemistry, earth, engineering, hephaestus, inquiry, physics};

#[test]
fn hephaestus_discovers_six_areas_and_relocates_the_shared_core_without_specializations() {
    hephaestus::run();
}

#[test]
fn mining_preserves_shared_routes_and_three_original_examples_after_relocation() {
    engineering::mining::run();
}

#[test]
fn mineralogy_petrology_preserves_shared_learning_and_two_original_examples_after_relocation() {
    earth::mineralogy::run();
}

#[test]
fn quantitative_readings_discover_relocate_and_select_without_absorbing_sources() {
    inquiry::quantitative_literacy::run();
}

#[test]
fn geology_reuses_foundations_and_preserves_its_selection_and_geometry_after_relocation() {
    earth::geology::run();
}

#[test]
fn metrology_reuses_shared_steps_with_its_own_continuations_and_relocates_them() {
    inquiry::metrology::run();
}

#[test]
fn reproducible_computation_preserves_shared_routes_and_inline_examples_after_relocation() {
    inquiry::reproducible_computation::run();
}

#[test]
fn chemistry_discovers_shared_foundations_and_relocates_its_bounded_learning_selection() {
    chemistry::general::run();
}

#[test]
fn metallurgy_preserves_shared_routes_and_three_original_examples_after_relocation() {
    engineering::metallurgy::run();
}

#[test]
fn industrial_chemistry_preserves_shared_routes_and_three_original_examples_after_relocation() {
    chemistry::industrial::run();
}

#[test]
fn chemical_supply_preserves_shared_routes_and_three_original_examples_after_relocation() {
    chemistry::supply::run();
}

#[test]
fn electromagnetism_preserves_shared_routes_and_three_original_examples_after_relocation() {
    physics::electromagnetism::run();
}

#[test]
fn power_engineering_preserves_shared_routes_and_three_original_examples_after_relocation() {
    engineering::power::run();
}

#[test]
fn thermodynamics_preserves_six_routes_and_four_original_examples_after_relocation() {
    physics::thermodynamics::run();
}

#[test]
fn fluid_dynamics_preserves_seven_routes_and_four_original_examples_after_relocation() {
    physics::fluid_dynamics::run();
}

#[test]
fn mechanical_engineering_preserves_shared_routes_and_original_examples_after_relocation() {
    engineering::mechanical::run();
}

#[test]
fn materials_engineering_preserves_shared_routes_and_original_examples_after_relocation() {
    engineering::materials::run();
}

#[test]
fn water_and_sanitation_preserves_its_selection_and_shared_routes_after_relocation() {
    engineering::water_and_sanitation::run();
}

#[test]
fn industrial_gases_preserves_the_complete_route_and_evidence_after_relocation() {
    chemistry::industrial_gases::run();
}
