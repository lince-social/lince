use super::*;
use crate::SurfaceId;
#[test]
fn perspective_mapping_round_trips_and_rejects_points_outside_the_quad() {
    let mapping = ContentMapping {
        canvas: Rectangle::new([100.0, 100.0], [600.0, 500.0]).unwrap(),
        surface: Rectangle::new([0.0, 0.0], [1200.0, 800.0]).unwrap(),
        corners: Some([
            [100.0, 100.0],
            [700.0, 180.0],
            [550.0, 600.0],
            [180.0, 450.0],
        ]),
    };
    for point in [[20.0, 30.0], [600.0, 400.0], [1180.0, 780.0]] {
        let canvas = mapping.canvas_point(point).unwrap();
        let actual = mapping.surface_point(canvas).unwrap();
        assert!((actual[0] - point[0]).abs() < 1e-8);
        assert!((actual[1] - point[1]).abs() < 1e-8);
        let shifted = mapping.translated([1366.0, 32.0]).unwrap();
        let actual = shifted
            .surface_point([canvas[0] + 1366.0, canvas[1] + 32.0])
            .unwrap();
        assert!((actual[0] - point[0]).abs() < 1e-8);
        assert!((actual[1] - point[1]).abs() < 1e-8);
    }
    assert!(mapping.surface_point([690.0, 590.0]).is_none());
    assert!(mapping.surface_point_unbounded([690.0, 590.0]).is_some());
    let invalid = ContentMapping {
        corners: Some([[1.0, 1.0]; 4]),
        ..mapping
    };
    assert!(invalid.surface_point([200.0, 200.0]).is_none());
}
#[test]
fn mapped_input_uses_the_published_content_rectangle() {
    let mapping = ContentMapping {
        corners: None,
        canvas: Rectangle::new([-400.0, 20.0], [800.0, 600.0]).unwrap(),
        surface: Rectangle::new([12.0, 8.0], [400.0, 300.0]).unwrap(),
    };
    assert_eq!(mapping.surface_point([0.0, 320.0]), Some([212.0, 158.0]));
    assert_eq!(mapping.surface_point([-400.0, 20.0]), Some([12.0, 8.0]));
    assert_eq!(mapping.surface_point([400.0, 320.0]), None);
    assert_eq!(mapping.surface_point([0.0, f64::NAN]), None);
}

#[test]
fn invalid_geometry_cannot_enter_the_mapping_contract() {
    for (origin, size) in [
        ([f64::NAN, 0.0], [100.0, 100.0]),
        ([0.0, 0.0], [0.0, 100.0]),
        ([0.0, 0.0], [-1.0, 100.0]),
        ([0.0, 0.0], [100.0, f64::INFINITY]),
        ([f64::MAX, 0.0], [f64::MAX, 100.0]),
    ] {
        assert!(Rectangle::new(origin, size).is_none());
    }
}

#[test]
fn clipped_application_geometry_does_not_receive_input_outside_a_hole() {
    let rectangle = Rectangle::new([0.0; 2], [200.0; 2]).unwrap();
    let item = ItemGeometry {
        id: ItemId::Application(SurfaceId(1)),
        output: OutputId(1),
        mapping: ContentMapping {
            corners: None,
            canvas: rectangle,
            surface: rectangle,
        },
        floating: false,
        clip: Some(vec![vec![
            [50.0, 50.0],
            [150.0, 50.0],
            [150.0, 150.0],
            [50.0, 150.0],
        ]]),
    };
    assert_eq!(item.surface_point([100.0; 2]), Some([100.0; 2]));
    assert_eq!(item.surface_point([25.0; 2]), None);
    assert_eq!(item.surface_point([f64::NAN, 100.0]), None);
}
