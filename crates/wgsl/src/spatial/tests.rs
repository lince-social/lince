// non-lean-yet

use super::*;
use crate::CALM;

#[test]
fn spatial_callbacks_validate_types_and_round_trip_settings() {
    let mesh = Mode::Mesh(MeshSettings {
        shape: Shape::Sphere,
        deformation: Some(Deformation {
            source: DEFORM_SPHERE.into(),
            max_displacement: 0.25,
        }),
        geometry: Some(GeneratedGeometry {
            source: GENERATED_TRIANGLE.into(),
            vertices: 3,
        }),
    });
    assert!(mesh.validate().is_ok());
    assert!(mesh.validate_program(MESH_SHADE).is_ok());
    assert!(mesh.validate_program(CALM).is_ok());
    assert_eq!(mesh_entry(MESH_SHADE).unwrap(), MeshEntry::Point);
    assert_eq!(mesh_entry(CALM).unwrap(), MeshEntry::Uv);
    assert_eq!(
        serde_json::from_value::<Mode>(serde_json::to_value(&mesh).unwrap()).unwrap(),
        mesh
    );
    assert!(
        mesh.validate_program(
            "fn shade_mesh(point: MeshPoint, inputs: ShaderInputs) -> f32 { return 1.0; }"
        )
        .is_err()
    );
    assert!(
        mesh.validate_program(&format!("{MESH_SHADE}{}", CALM))
            .is_err()
    );
    let volume = Mode::Volume(VolumeSettings::default());
    assert!(volume.validate().is_ok());
    assert!(volume.validate_program(VOLUME_CLOUD).is_ok());
    for quality in [Quality::Low, Quality::Medium, Quality::High] {
        let volume = Mode::Volume(VolumeSettings {
            quality,
            ..VolumeSettings::default()
        });
        assert!(
            volume.validate_program(VOLUME_CLOUD).is_ok(),
            "Cloud preset exceeds {quality:?} quality's work budget"
        );
    }
    assert!(volume.validate_program(MESH_SHADE).is_err());
    assert_eq!(
        [
            Quality::Low.steps(),
            Quality::Medium.steps(),
            Quality::High.steps()
        ],
        [16, 32, 64]
    );
}

#[test]
fn geometry_and_deformation_cannot_choose_unbounded_resources_or_work() {
    for vertices in [0, 1, 4, 4096, u32::MAX] {
        let mode = Mode::Mesh(MeshSettings {
            geometry: Some(GeneratedGeometry {
                source: GENERATED_TRIANGLE.into(),
                vertices,
            }),
            ..Default::default()
        });
        assert!(mode.validate().is_err());
    }
    for displacement in [-1.0, 0.51, f32::INFINITY, f32::NAN] {
        let mode = Mode::Mesh(MeshSettings {
            deformation: Some(Deformation {
                source: DEFORM_SPHERE.into(),
                max_displacement: displacement,
            }),
            ..Default::default()
        });
        assert!(mode.validate().is_err());
    }
    for source in [
        format!("@group(2) @binding(0) var<storage, read_write> arbitrary: array<f32>; {GENERATED_TRIANGLE}"),
        "fn generate_vertex(index: u32, vertex_count: u32, inputs: ShaderInputs) -> MeshVertex { loop {} return MeshVertex(vec3<f32>(0.0), vec3<f32>(1.0), vec2<f32>(0.0)); }".into(),
    ] {
        let mode = Mode::Mesh(MeshSettings { geometry: Some(GeneratedGeometry { source, vertices: 3 }), ..Default::default() });
        assert!(mode.validate().is_err());
    }
    let mode = Mode::Mesh(MeshSettings {
        geometry: Some(GeneratedGeometry {
            source: GENERATED_TRIANGLE.into(),
            vertices: 4095,
        }),
        ..Default::default()
    });
    assert!(mode.validate().is_ok());
    for density in [-1.0, 8.1, f32::NAN] {
        assert!(
            Mode::Volume(VolumeSettings {
                density,
                ..Default::default()
            })
            .validate()
            .is_err()
        );
    }
}

#[test]
fn volume_quality_charges_field_work_for_every_sample() {
    let mut body = String::from("let n0 = position.x;");
    for index in 1..13 {
        body.push_str(&format!(
            "let n{index} = sin(n{}) + position.y * 0.2;",
            index - 1
        ));
    }
    let source = format!(
        "fn volume_sample(position: vec3<f32>, inputs: ShaderInputs) -> vec4<f32> {{ {body} return vec4<f32>(n12); }}"
    );
    let low = Mode::Volume(VolumeSettings {
        quality: Quality::Low,
        ..Default::default()
    });
    let high = Mode::Volume(VolumeSettings {
        quality: Quality::High,
        ..Default::default()
    });
    assert!(low.validate_program(&source).is_ok());
    assert!(
        high.validate_program(&source)
            .unwrap_err()
            .contains("64 samples")
    );
    assert!(high.validate_program("fn volume_sample(position: vec3<f32>, inputs: ShaderInputs) -> vec4<f32> { return vec4<f32>(0.2); }").is_ok());
}

#[test]
fn custom_helpers_cannot_override_renderer_bounds_or_scene_depth() {
    for helper in [
        "fn length(value: vec3<f32>) -> f32 { return 0.0; }",
        "fn min(left: f32, right: f32) -> f32 { return 100000.0; }",
        "fn max(left: f32, right: f32) -> f32 { return 0.0; }",
    ] {
        let mode = Mode::Mesh(MeshSettings {
            deformation: Some(Deformation {
                source: format!(
                    "{helper} fn deform_vertex(vertex: MeshVertex, inputs: ShaderInputs) -> MeshVertex {{ return MeshVertex(vertex.position * 1000.0, vertex.normal, vertex.uv); }}"
                ),
                max_displacement: 0.1,
            }),
            ..Default::default()
        });
        assert!(
            mode.validate()
                .unwrap_err()
                .contains("reserved for renderer")
        );
    }
    let mode = Mode::Volume(VolumeSettings::default());
    assert!(mode.validate_program(&format!("fn prepass_depth(position: vec4<f32>, index: u32) -> f32 {{ return 0.0; }} {VOLUME_CLOUD}")).unwrap_err().contains("reserved for renderer"));
    assert!(
        mode.validate_program(&format!(
            "fn my_length(value: vec3<f32>) -> f32 {{ return length(value); }} {VOLUME_CLOUD}"
        ))
        .is_ok()
    );
}
