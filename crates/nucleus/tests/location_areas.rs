use nucleus::{
    place::{Place, approximate, distance},
    visibility::{Bound, Condition, Data, Policy},
};

#[test]
fn generalized_centres_cover_points_at_poles_dateline_and_all_supported_sizes() {
    for radius in [100, 1000, 3000, 100_000] {
        for latitude in -90..=90 {
            for longitude in (-180..=180).step_by(7) {
                let point = Place {
                    lat: f64::from(latitude),
                    lon: f64::from(longitude),
                };
                let center = approximate(point, radius).unwrap();
                assert!(distance(point, center) <= f64::from(radius));
                assert_eq!(approximate(center, radius), Some(center));
                let nearby = Place {
                    lat: (center.lat + 0.00001).min(90.0),
                    lon: center.lon,
                };
                assert_eq!(approximate(nearby, radius), Some(center));
            }
        }
        assert_eq!(
            approximate(
                Place {
                    lat: 30.0,
                    lon: -180.0
                },
                radius
            ),
            approximate(
                Place {
                    lat: 30.0,
                    lon: 180.0
                },
                radius
            ),
        );
    }
    for point in [
        Place {
            lat: f64::NAN,
            lon: 0.0,
        },
        Place {
            lat: 91.0,
            lon: 0.0,
        },
        Place {
            lat: 0.0,
            lon: 181.0,
        },
    ] {
        assert!(approximate(point, 1000).is_none());
    }
    for radius in [0, 99, 100_001, u32::MAX] {
        assert!(approximate(Place { lat: 0.0, lon: 0.0 }, radius).is_none());
    }
}

#[test]
fn matching_location_permissions_choose_precision_and_exclusion_still_wins() {
    let passenger = nucleus::new_uid("r");
    let family = nucleus::new_uid("r");
    let policy = Policy {
        include: vec![
            Condition {
                upper: Some(Bound {
                    value: 8,
                    inclusive: false,
                }),
                radius_metres: Some(3000),
                ..Default::default()
            },
            Condition {
                organs: vec![family.clone()],
                radius_metres: Some(1000),
                ..Default::default()
            },
            Condition {
                organs: vec![passenger.clone()],
                ..Default::default()
            },
        ],
        exclude: vec![Condition {
            lower: Some(Bound {
                value: 6,
                inclusive: true,
            }),
            ..Default::default()
        }],
    };
    assert!(policy.validate_for(Data::LiveLocation).is_ok());
    assert!(policy.validate_for(Data::Place).is_ok());
    assert!(policy.validate_for(Data::Record).is_err());
    assert_eq!(policy.decide(&passenger, Some(2)).radius_metres, None);
    assert_eq!(policy.decide(&family, Some(5)).radius_metres, Some(1000));
    assert!(!policy.decide(&family, Some(6)).allowed);
    let exclusion_size = Policy {
        include: vec![],
        exclude: vec![Condition {
            radius_metres: Some(1000),
            ..Default::default()
        }],
    };
    assert!(exclusion_size.validate().is_err());
}
