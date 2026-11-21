// non-lean-yet

use super::*;

#[cfg(feature = "shader-compiler")]
#[test]
fn renderer_lighting_calls_cannot_be_overridden_by_unused_helpers() {
    for name in [
        "mix",
        "inverseSqrt",
        "sqrt",
        "reflect",
        "textureDimensions",
        "lince_lit_color",
        "helperX_naga_oil_mod_XAAAX",
    ] {
        let source = format!("fn {name}(x: f32) -> f32 {{ return x; }}\n{CALM}");
        assert!(compile(&source).unwrap_err().contains("reserved"), "{name}");
    }
    assert!(
        compile(&format!(
            "fn user_tint(x: f32) -> f32 {{ return x; }}\n{CALM}"
        ))
        .is_ok()
    );
}

#[test]
fn catalog_identity_defaults_and_limits_are_explicit() {
    let mut catalog = Extension::single(1, Effect::default());
    for index in 1..8 {
        catalog.effects.insert(
            format!("effect-{index}"),
            NamedEffect {
                label: format!("Effect {index}"),
                effect: Effect::default(),
            },
        );
    }
    #[cfg(feature = "shader-compiler")]
    assert!(catalog.validate().is_ok());
    assert!(catalog.bounded());
    assert_eq!(catalog.selected(None), catalog.selected(Some("main")));
    assert!(catalog.selected(Some("missing")).is_none());
    catalog.default_effect = "effect-1".into();
    assert_eq!(catalog.selected(None), catalog.selected(Some("effect-1")));
    let stored = serde_json::to_value(&catalog).unwrap();
    assert_eq!(
        serde_json::from_value::<Extension>(stored).unwrap(),
        catalog
    );
    catalog
        .effects
        .insert("ninth".into(), catalog.effects["main"].clone());
    #[cfg(feature = "shader-compiler")]
    assert!(catalog.validate().is_err());
    assert!(!catalog.bounded());
    catalog.effects.remove("ninth");
    catalog.default_effect = "missing".into();
    assert!(!catalog.bounded());
    #[cfg(feature = "shader-compiler")]
    assert!(catalog.validate().is_err());
    catalog.default_effect = "main".into();
    for label in ["", "   ", "bad\nlabel"] {
        catalog.effects.get_mut("main").unwrap().label = label.into();
        assert!(!catalog.bounded());
        #[cfg(feature = "shader-compiler")]
        assert!(catalog.validate().is_err());
    }
    catalog.effects.get_mut("main").unwrap().label = "Main".into();
    let effect = catalog.effects.remove("effect-1").unwrap();
    catalog.effects.insert("invalid id".into(), effect);
    assert!(!catalog.bounded());
    #[cfg(feature = "shader-compiler")]
    assert!(catalog.validate().is_err());
}

#[test]
fn catalog_bounds_combined_source_and_encoded_data() {
    let mut effect = Effect::default();
    effect.rules.clear();
    effect.fallback = "only".into();
    effect.programs = [("only".into(), format!("{CALM}{}", " ".repeat(60_000)))].into();
    let mut catalog = Extension::single(1, effect);
    let main = catalog.effects["main"].clone();
    catalog.effects.insert("second".into(), main.clone());
    assert!(catalog.bounded());
    #[cfg(feature = "shader-compiler")]
    assert!(catalog.validate().is_ok());
    catalog.effects.insert("third".into(), main);
    #[cfg(feature = "shader-compiler")]
    assert!(catalog.validate().unwrap_err().contains("128 KiB"));
    assert!(!catalog.bounded());
    catalog.effects.remove("third");
    for named in catalog.effects.values_mut() {
        let source = named.effect.programs.get_mut("only").unwrap();
        *source = format!("{CALM}{}", "\n".repeat(60_000));
    }
    #[cfg(feature = "shader-compiler")]
    assert!(catalog.validate().unwrap_err().contains("192 KiB"));
    assert!(!catalog.bounded());
}

#[test]
fn mappings_are_opt_in_exact_directional_and_clamped() {
    assert_eq!(
        InputMappings::default().resolve(Some("-3")),
        ResolvedInputs::default()
    );
    let mut mapping = InputMappings {
        task: Some(QuantityRange {
            start: "-3".into(),
            end: "0".into(),
        }),
        progress: Some(QuantityRange {
            start: "-3".into(),
            end: "0".into(),
        }),
    };
    assert!(mapping.validate().is_ok());
    for (value, state, progress) in [
        ("-4", TaskState::Pending, 0.0),
        ("-3", TaskState::Pending, 0.0),
        ("-1.5", TaskState::Active, 0.5),
        ("0", TaskState::Completed, 1.0),
        ("7", TaskState::Completed, 1.0),
    ] {
        assert_eq!(
            mapping.resolve(Some(value)),
            ResolvedInputs {
                task: state,
                progress: Some(progress)
            }
        );
    }
    assert_eq!(
        mapping.resolve(Some("-0.000000000000000001")).task,
        TaskState::Active
    );
    mapping.task = Some(QuantityRange {
        start: "10".into(),
        end: "0".into(),
    });
    mapping.progress = mapping.task.clone();
    assert_eq!(
        mapping.resolve(Some("5")),
        ResolvedInputs {
            task: TaskState::Active,
            progress: Some(0.5)
        }
    );
    assert_eq!(mapping.resolve(Some("11")).task, TaskState::Pending);
    assert_eq!(mapping.resolve(Some("-1")).task, TaskState::Completed);
    for missing in [None, Some("NaN"), Some("1e100"), Some("invalid")] {
        assert_eq!(mapping.resolve(missing), ResolvedInputs::default());
    }
    let start = "10000000000000000";
    let end = "10000000000000002";
    mapping.progress = Some(QuantityRange {
        start: start.into(),
        end: end.into(),
    });
    assert_eq!(
        mapping.resolve(Some("10000000000000001")).progress,
        Some(0.5)
    );
    mapping.progress = Some(QuantityRange {
        start: "-17014118346046923173168730371588410573".into(),
        end: "-17014118346046923173168730371588410572".into(),
    });
    assert_eq!(
        mapping
            .resolve(Some("-17014118346046923173168730371588410572.6"))
            .progress,
        Some(0.4)
    );
    mapping.progress = Some(QuantityRange {
        start: "-170141183460469231731687303715884105727".into(),
        end: "0".into(),
    });
    assert_eq!(mapping.resolve(Some("-0.1")).progress, Some(1.0));
    mapping.progress.as_mut().unwrap().end = "170141183460469231731687303715884105727".into();
    assert!(mapping.validate().is_ok());
    assert_eq!(mapping.resolve(Some("0")).progress, Some(0.5));
    let stored = serde_json::to_value(&mapping).unwrap();
    assert_eq!(
        serde_json::from_value::<InputMappings>(stored).unwrap(),
        mapping
    );
}

#[cfg(feature = "shader-compiler")]
#[test]
fn mappings_reject_invalid_ranges_and_task_presets_compile() {
    let mut effect = Effect::default();
    for (start, end) in [("0", "0.0"), ("NaN", "1"), ("0", "1e100")] {
        effect.inputs.task = Some(QuantityRange {
            start: start.into(),
            end: end.into(),
        });
        assert!(effect.validate().is_err());
    }
    assert!(compile(REWARD).is_ok());
    assert!(compile(REMINDER).is_ok());
    assert!(compile(GRAIN).is_ok());
    assert!(compile(NUMERIC).is_ok());
    assert!(compile(CLEANING).is_ok());
}

#[test]
fn rules_use_decimal_boundaries_and_fallbacks() {
    let effect = Effect::default();
    #[cfg(feature = "shader-compiler")]
    assert!(effect.validate().is_ok());
    assert_eq!(effect.selected(Some("-0.000000000000000001")), "negative");
    assert_eq!(effect.selected(Some("0")), "nonnegative");
    assert_eq!(effect.selected(Some("1")), "nonnegative");
    assert_eq!(effect.selected(None), "neutral");
    assert_eq!(effect.selected(Some("NaN")), "neutral");
    let mut overlap = effect.clone();
    overlap.rules[0].operator = Operator::AtLeast;
    assert_eq!(overlap.selected(Some("0")), "negative");
}

#[cfg(feature = "shader-compiler")]
#[test]
fn supported_programs_reject_unbounded_work_and_extra_bindings() {
    for source in [CALM, ENERGY, GRADIENT, RIPPLE] {
        assert!(compile(source).is_ok());
    }
    assert!(compile("fn shade(uv: vec2<f32>, inputs: ShaderInputs) -> vec4<f32> { loop {} return vec4<f32>(1.0); }").is_err());
    assert!(
        compile(&format!(
            "@group(2) @binding(0) var image: texture_2d<f32>;{CALM}"
        ))
        .is_err()
    );
    let mut effect = Effect::default();
    effect.rules[0].program = "absent".into();
    assert!(effect.validate().is_err());
    assert!(gpu_quantity(Some("inf")).is_none());
    assert!(compile(&format!("var<private> huge: array<f32, 1000000>;{CALM}")).is_err());
}

#[cfg(feature = "shader-compiler")]
#[test]
fn catalog_source_limits_include_geometry_and_deformation() {
    use super::spatial::{
        DEFORM_SPHERE, Deformation, GENERATED_TRIANGLE, GeneratedGeometry, MeshSettings, Mode,
    };

    let effect = Effect {
        mode: Mode::Mesh(MeshSettings {
            deformation: Some(Deformation {
                source: format!("{DEFORM_SPHERE}{}", " ".repeat(60_000)),
                max_displacement: 0.25,
            }),
            geometry: Some(GeneratedGeometry {
                source: format!("{GENERATED_TRIANGLE}{}", " ".repeat(60_000)),
                vertices: 3,
            }),
            ..Default::default()
        }),
        ..Default::default()
    };
    assert!(effect.validate().is_ok());
    let mut catalog = Extension::single(1, effect);
    assert!(catalog.validate().is_ok());
    catalog
        .effects
        .get_mut("main")
        .unwrap()
        .effect
        .programs
        .insert("extra".into(), format!("{}{}", CALM, " ".repeat(12_000)));
    assert!(catalog.validate().unwrap_err().contains("128 KiB"));
    assert!(!catalog.bounded());
}
