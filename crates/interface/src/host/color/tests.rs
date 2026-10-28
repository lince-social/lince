use super::*;

#[test]
fn untagged_srgb_keeps_identity_primaries_and_the_existing_transfer_characteristic() {
    let description = Description::default();
    let black = description.linear_srgb([0.0; 3]).unwrap();
    let white = description.linear_srgb([1.0; 3]).unwrap();
    assert_eq!(black, [0.0; 3]);
    assert_eq!(white, [1.0; 3]);
    let middle = description.linear_srgb([0.5; 3]).unwrap();
    assert!(middle.iter().all(|value| (*value - 0.21404114).abs() < 0.000001));
    let uniforms = description.uniforms();
    assert_eq!(uniforms[0], [1.0, 0.0, 0.0, 0.0]);
    assert_eq!(uniforms[1], [0.0, 1.0, 0.0, 0.0]);
    assert_eq!(uniforms[2], [0.0, 0.0, 1.0, 0.0]);
    assert_eq!(uniforms[3], [0.0, 1.0, 0.0, 0.0]);
}

#[test]
fn p3_uses_d65_chromaticities_preserves_neutral_white_and_does_not_silently_treat_its_red_as_srgb() {
    let description = Description { primaries: Primaries::DisplayP3, ..Default::default() };
    let white = description.linear_srgb([1.0; 3]).unwrap();
    assert!(white.iter().all(|value| (*value - 1.0).abs() < 0.000001));
    let red = description.linear_srgb([1.0, 0.0, 0.0]).unwrap();
    assert!((1.22..1.23).contains(&red[0]));
    assert!((-0.045..-0.04).contains(&red[1]));
    assert!((-0.025..-0.015).contains(&red[2]));
    assert_eq!(description.primaries.chromaticities()[3], Primaries::Srgb.chromaticities()[3]);
}

#[test]
fn source_reference_white_is_anchored_and_invalid_or_unimplemented_volumes_are_refused() {
    let gamma = Description { transfer: Transfer::Gamma22, minimum_luminance: 0, maximum_luminance: 160, reference_luminance: 80, ..Default::default() };
    let encoded_reference = 0.5_f64.powf(1.0 / 2.2);
    assert!(gamma.linear_srgb([encoded_reference; 3]).unwrap().iter().all(|value| (*value - 1.0).abs() < 0.000001));
    for value in [Description { reference_luminance: 0, ..Default::default() },
        Description { minimum_luminance: 800000, ..Default::default() },
        Description { maximum_luminance: 0, ..Default::default() },
        Description { maximum_luminance: 20000, ..Default::default() },
        Description { reference_luminance: 90, ..Default::default() }] {
        assert!(!value.supported_sdr());
        assert!(value.linear_srgb([0.5; 3]).is_none());
    }
    assert!(Description::default().linear_srgb([f64::NAN, 0.0, 0.0]).is_none());
    assert!(Description::default().linear_srgb([0.0, f64::INFINITY, 0.0]).is_none());
    assert!(Description::default().linear_srgb([-0.1, 0.0, 0.0]).is_none());
    assert!(Description::default().linear_srgb([1.01, 0.0, 0.0]).is_none());
}

#[test]
fn pq_uses_absolute_luminance_and_the_extended_working_space_keeps_highlights() {
    for (nits, encoded) in [(0.0, 0.0), (100.0, 0.508078421517399), (203.0, 0.5806888810416109),
        (1000.0, 0.751827096247041), (10000.0, 1.0)] {
        assert!((nits_to_pq(nits).unwrap() - encoded).abs() < 0.000001);
        assert!((pq_to_nits(encoded).unwrap() - nits).abs() <= 0.000001 * nits.max(1.0));
    }
    let description = Description::hdr10();
    assert!(description.supported());
    assert!(!description.supported_sdr());
    let value = description.linear_srgb([0.751827096247041; 3]).unwrap();
    assert!(value.iter().all(|value| (*value - 12.5).abs() < 0.000001));
    let red = description.linear_srgb([1.0, 0.0, 0.0]).unwrap();
    assert!((207.55..207.57).contains(&red[0]));
    assert!((-15.58..-15.56).contains(&red[1]));
    assert!((-2.28..-2.26).contains(&red[2]));
    assert_eq!(Description::linear_working().linear_srgb([2.0, -0.125, 0.5]), Some([2.0, -0.125, 0.5]));
    for value in [f64::NAN, f64::INFINITY, -0.0001, 1.0001] { assert!(pq_to_nits(value).is_none()); }
    for value in [f64::NAN, f64::INFINITY, -0.0001, 10000.0001] { assert!(nits_to_pq(value).is_none()); }
    assert!(Transfer::Linear.decode(65505.0).is_none());
    assert!(Transfer::Linear.decode(f64::NAN).is_none());
}

#[test]
fn mastering_volume_and_content_light_are_bounded_without_clipping_the_encoding_volume() {
    let primaries = Primaries::Bt2020.chromaticities();
    let contained = primaries.map(|xy| std::array::from_fn(|axis| (xy[axis] + primaries[3][axis]) / 2));
    let metadata = Mastering { primaries: Some(contained),
        luminances: Some([50, 1000]), max_cll: Some(1000), max_fall: Some(400) };
    let description = Description { mastering: metadata, ..Description::hdr10() };
    assert!(description.supported());
    assert_eq!(description.linear_srgb([1.0; 3]), Description::hdr10().linear_srgb([1.0; 3]));
    for metadata in [Mastering { max_fall: Some(1001), ..metadata },
        Mastering { max_cll: Some(1001), ..metadata },
        Mastering { max_cll: Some(0), ..metadata },
        Mastering { luminances: Some([10000000, 1000]), ..metadata },
        Mastering { luminances: Some([50, 10001]), ..metadata },
        Mastering { primaries: Some(Primaries::DisplayP3.chromaticities()), ..metadata },
        Mastering { primaries: Some([[0, 0]; 4]), ..metadata },
        Mastering { primaries: Some([[i32::MAX, i32::MIN]; 4]), ..metadata }] {
        assert!(!(Description { mastering: metadata, ..description }).supported());
    }
    assert!(!(Description { primaries: Primaries::Srgb, mastering: Mastering {
        primaries: Some(Primaries::Bt2020.chromaticities()), ..Default::default() }, ..Default::default() }).supported());
    let mut different = description;
    different.mastering.max_fall = Some(300);
    assert_ne!(description, different);
    assert_eq!(description.uniforms(), different.uniforms());
}
