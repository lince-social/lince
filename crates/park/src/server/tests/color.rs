use super::*;
use lince_interface::host::color::{Description, Primaries, Transfer};
use wayland_protocols::wp::color_management::v1::client::{
    wp_color_manager_v1::{self as manager, WpColorManagerV1 as Manager},
    wp_color_management_surface_v1::WpColorManagementSurfaceV1 as Surface,
    wp_color_management_output_v1::{self as color_output, WpColorManagementOutputV1 as Output},
    wp_color_management_surface_feedback_v1::{self as feedback, WpColorManagementSurfaceFeedbackV1 as Feedback},
    wp_image_description_creator_params_v1::WpImageDescriptionCreatorParamsV1 as Params,
    wp_image_description_creator_icc_v1::WpImageDescriptionCreatorIccV1 as Icc,
    wp_image_description_v1::{self as image, WpImageDescriptionV1 as Image},
    wp_image_description_info_v1::{self as info, WpImageDescriptionInfoV1 as Info},
};

#[derive(Default)]
pub(super) struct Capture {
    pub(super) manager: Option<Manager>,
    primaries: Vec<manager::Primaries>,
    pub(super) transfers: Vec<manager::TransferFunction>,
    features: Vec<manager::Feature>,
    pub(super) ready: HashMap<u32, u32>,
    failed: Vec<u32>,
    information: Vec<(u32, u32, u32)>,
    information_done: usize,
    output_changes: usize,
    preferred: Vec<u32>,
}

pub(super) fn bind(state: &mut Client, registry: &wl_registry::WlRegistry, name: u32, version: u32, qh: &QueueHandle<Client>) {
    state.color.manager = Some(registry.bind(name, version.min(1), qh, ()));
}

impl Dispatch<Manager, ()> for Client {
    fn event(state: &mut Self, _: &Manager, event: manager::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match event {
            manager::Event::SupportedFeature { feature: wayland_client::WEnum::Value(feature) } => state.color.features.push(feature),
            manager::Event::SupportedPrimariesNamed { primaries: wayland_client::WEnum::Value(primaries) } => state.color.primaries.push(primaries),
            manager::Event::SupportedTfNamed { tf: wayland_client::WEnum::Value(tf) } => state.color.transfers.push(tf),
            _ => {}
        }
    }
}

impl Dispatch<Image, u32> for Client {
    fn event(state: &mut Self, _: &Image, event: image::Event, key: &u32, _: &Connection, _: &QueueHandle<Self>) {
        match event {
            image::Event::Ready { identity } => { state.color.ready.insert(*key, identity); }
            image::Event::Failed { .. } => state.color.failed.push(*key),
            _ => {}
        }
    }
}

impl Dispatch<Info, ()> for Client {
    fn event(state: &mut Self, _: &Info, event: info::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match event {
            info::Event::Luminances { min_lum, max_lum, reference_lum } => state.color.information.push((min_lum, max_lum, reference_lum)),
            info::Event::Done => state.color.information_done += 1,
            _ => {}
        }
    }
}

impl Dispatch<Feedback, ()> for Client {
    fn event(state: &mut Self, _: &Feedback, event: feedback::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let feedback::Event::PreferredChanged { identity } = event { state.color.preferred.push(identity); }
    }
}

impl Dispatch<Output, ()> for Client {
    fn event(state: &mut Self, _: &Output, event: color_output::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if matches!(event, color_output::Event::ImageDescriptionChanged) { state.color.output_changes += 1; }
    }
}

delegate_noop!(Client: ignore Surface);
delegate_noop!(Client: ignore Params);
delegate_noop!(Client: ignore Icc);

fn harness() -> Harness {
    let mut server = Server::new([640, 480]).unwrap();
    server.enable_sdr_color_management();
    Harness::connect(server)
}

fn description(harness: &mut Harness, key: u32, primaries: manager::Primaries, transfer: manager::TransferFunction) -> Image {
    let params = harness.client.color.manager.as_ref().unwrap().create_parametric_creator(&harness.queue.handle(), ());
    params.set_primaries_named(primaries); params.set_tf_named(transfer);
    let image = params.create(&harness.queue.handle(), key);
    harness.sync();
    assert!(harness.client.color.ready.contains_key(&key));
    image
}

#[test]
fn color_protocol_is_enabled_only_when_its_renderer_path_is_ready_and_hdr_is_not_advertised_by_sdr() {
    assert!(Harness::new().client.color.manager.is_none());
    let harness = harness();
    assert!(harness.client.color.manager.is_some());
    assert_eq!(harness.client.color.primaries, vec![manager::Primaries::Srgb, manager::Primaries::DisplayP3]);
    assert_eq!(harness.client.color.transfers, vec![manager::TransferFunction::Srgb, manager::TransferFunction::Gamma22]);
    assert_eq!(harness.client.color.features, vec![manager::Feature::Parametric, manager::Feature::SetLuminances]);
}

#[test]
fn color_changes_are_committed_with_the_surface_and_description_destruction_keeps_copy_semantics() {
    let mut harness = harness();
    let window = harness.window();
    let _buffer = harness.attach(&window, [16, 16]);
    let before = harness.server.windows().remove(0);
    let version = before.nodes[0].content.version;
    let object = harness.client.color.manager.as_ref().unwrap().get_surface(&window.surface, &harness.queue.handle(), ());
    let image = description(&mut harness, 1, manager::Primaries::DisplayP3, manager::TransferFunction::Gamma22);
    object.set_image_description(&image, manager::RenderIntent::Perceptual); image.destroy(); harness.sync();
    assert_eq!(harness.server.windows()[0].nodes[0].color, Description::default());
    window.surface.commit(); harness.sync();
    let current = harness.server.windows().remove(0);
    assert_eq!(current.nodes[0].content.version, version);
    assert_eq!(current.nodes[0].color.primaries, Primaries::DisplayP3);
    assert_eq!(current.nodes[0].color.transfer, Transfer::Gamma22);
    assert_eq!(before.nodes[0].color, Description::default());
    object.destroy(); harness.sync();
    assert_eq!(harness.server.windows()[0].nodes[0].color.primaries, Primaries::DisplayP3);
    window.surface.commit(); harness.sync();
    assert_eq!(harness.server.windows()[0].nodes[0].color, Description::default());
    let replacement = harness.client.color.manager.as_ref().unwrap().get_surface(&window.surface, &harness.queue.handle(), ());
    harness.sync(); replacement.destroy(); harness.sync();
}

#[test]
fn synchronized_child_metadata_waits_for_parent_commit_and_does_not_change_the_parent_description() {
    let mut harness = harness();
    let window = harness.window();
    let _parent = harness.attach(&window, [32, 32]);
    let child = harness.client.compositor.as_ref().unwrap().create_surface(&harness.queue.handle(), ());
    let subsurface = harness.client.subcompositor.as_ref().unwrap().get_subsurface(&child, &window.surface, &harness.queue.handle(), ());
    let _child_buffer = harness.buffer([8, 8]);
    child.attach(Some(&_child_buffer), 0, 0); child.commit(); window.surface.commit(); harness.sync();
    let object = harness.client.color.manager.as_ref().unwrap().get_surface(&child, &harness.queue.handle(), ());
    let image = description(&mut harness, 2, manager::Primaries::DisplayP3, manager::TransferFunction::Srgb);
    object.set_image_description(&image, manager::RenderIntent::Perceptual); child.commit(); harness.sync();
    assert!(harness.server.windows()[0].nodes.iter().all(|node| node.color == Description::default()));
    window.surface.commit(); harness.sync();
    let snapshot = harness.server.windows().remove(0);
    let node = snapshot.nodes.iter().find(|node| node.surface.id().protocol_id() == child.id().protocol_id()).unwrap();
    assert_eq!(node.color.primaries, Primaries::DisplayP3);
    assert_eq!(snapshot.nodes.iter().find(|node| node.surface.id().protocol_id() == window.surface.id().protocol_id()).unwrap().color, Description::default());
    object.destroy(); image.destroy(); subsurface.destroy(); child.destroy(); harness.sync();
}

#[test]
fn output_descriptions_are_readable_immutable_and_client_descriptions_share_live_identity() {
    let mut harness = harness();
    let manager = harness.client.color.manager.as_ref().unwrap().clone();
    let output = manager.get_output(harness.client.output.as_ref().unwrap(), &harness.queue.handle(), ());
    let original = output.get_image_description(&harness.queue.handle(), 1);
    harness.sync();
    original.get_information(&harness.queue.handle(), ()); harness.sync();
    assert_eq!(harness.client.color.information, vec![(2000, 80, 80)]);
    assert_eq!(harness.client.color.information_done, 1);
    let first = description(&mut harness, 2, manager::Primaries::DisplayP3, manager::TransferFunction::Srgb);
    let second = description(&mut harness, 3, manager::Primaries::DisplayP3, manager::TransferFunction::Srgb);
    assert_eq!(harness.client.color.ready[&2], harness.client.color.ready[&3]);
    assert_ne!(harness.client.color.ready[&1], harness.client.color.ready[&2]);
    first.destroy(); second.destroy(); original.destroy(); output.destroy(); harness.sync();
}

#[test]
fn valid_but_unimplemented_luminance_volume_fails_without_producing_a_ready_description() {
    let mut harness = harness();
    let params = harness.client.color.manager.as_ref().unwrap().create_parametric_creator(&harness.queue.handle(), ());
    params.set_tf_named(manager::TransferFunction::Srgb); params.set_primaries_named(manager::Primaries::Srgb);
    params.set_luminances(0, 20000, 80);
    let object = params.create(&harness.queue.handle(), 8); harness.sync();
    assert_eq!(harness.client.color.failed, vec![8]);
    assert!(!harness.client.color.ready.contains_key(&8));
    object.destroy(); harness.sync();
}

#[test]
fn invalid_and_duplicate_color_requests_fail_the_connection_without_poisoning_the_server() {
    for invalid in 0..7 {
        let mut harness = harness();
        let params = harness.client.color.manager.as_ref().unwrap().create_parametric_creator(&harness.queue.handle(), ());
        match invalid {
            0 => { params.set_tf_named(manager::TransferFunction::Srgb); params.set_tf_named(manager::TransferFunction::Srgb); }
            1 => params.set_tf_named(manager::TransferFunction::St2084Pq),
            2 => params.set_primaries_named(manager::Primaries::Bt2020),
            3 => params.set_luminances(800000, 80, 80),
            4 => { let _image = params.create(&harness.queue.handle(), 4); }
            5 => { let _image = harness.client.color.manager.as_ref().unwrap().create_windows_scrgb(&harness.queue.handle(), 5); }
            _ => { let _creator = harness.client.color.manager.as_ref().unwrap().create_icc_creator(&harness.queue.handle(), ()); }
        }
        let failure = synchronize(&mut harness.server, &harness.connection, &mut harness.queue, &mut harness.client).unwrap_err();
        assert!(!failure.contains("timed out"), "The fixture must observe a protocol rejection, not a timeout: {failure}");
        let mut recovered = Harness::connect(harness.server);
        let window = recovered.window();
        let _buffer = recovered.attach(&window, [16, 16]);
        assert_eq!(recovered.server.windows().iter().filter(|window| window.mapped).count(), 1);
    }
}

#[test]
fn per_client_color_object_pressure_is_bounded_and_fresh_clients_retain_normal_input_surfaces() {
    let mut harness = harness();
    let manager = harness.client.color.manager.as_ref().unwrap().clone();
    let _objects: Vec<_> = (0..300).map(|_| manager.create_parametric_creator(&harness.queue.handle(), ())).collect();
    let failure = synchronize(&mut harness.server, &harness.connection, &mut harness.queue, &mut harness.client).unwrap_err();
    assert!(!failure.contains("timed out"), "The limit must terminate the offending connection: {failure}");
    let mut recovered = Harness::connect(harness.server);
    let window = recovered.window();
    let _buffer = recovered.attach(&window, [16, 16]);
    assert_eq!(recovered.server.windows().iter().filter(|window| window.mapped).count(), 1);
    let description = description(&mut recovered, 99, manager::Primaries::Srgb, manager::TransferFunction::Srgb);
    description.destroy(); recovered.sync();
}

fn hdr_harness() -> Harness {
    let mut server = Server::new([640, 480]).unwrap();
    server.enable_hdr_color_management().unwrap();
    Harness::connect(server)
}

#[test]
fn output_description_changes_only_after_effective_color_observation_and_preserves_old_objects() {
    use lince_interface::host::{color::{ColorMode, OutputColor}, display::ColorStatus};
    let mut harness = hdr_harness();
    harness.server.registered_output_catalog().unwrap();
    harness.sync();
    let manager = harness.client.color.manager.as_ref().unwrap().clone();
    let output = manager.get_output(harness.client.output.as_ref().unwrap(), &harness.queue.handle(), ());
    let old = output.get_image_description(&harness.queue.handle(), 501);
    harness.sync();
    let before = harness.server.display_snapshot().revision;
    let target = OutputColor { mode: ColorMode::Hdr10, peak_nits: 1000 };
    let status = ColorStatus { hdr10: true, prepared: Some(target), effective: Some(Default::default()),
        revision: 7, pending: true, ..Default::default() };
    harness.server.output_color(OutputId(1), status.clone());
    harness.sync();
    assert_eq!(harness.client.color.output_changes, 0);
    assert_eq!(harness.server.display_snapshot().revision, before);
    harness.server.output_color(OutputId(1), ColorStatus { effective: Some(target), pending: false, ..status });
    harness.sync();
    assert_eq!(harness.client.color.output_changes, 1);
    assert_eq!(harness.server.display_snapshot().revision, before);
    let current = output.get_image_description(&harness.queue.handle(), 502);
    harness.sync();
    old.get_information(&harness.queue.handle(), ());
    current.get_information(&harness.queue.handle(), ());
    harness.sync();
    let values = &harness.client.color.information;
    assert_eq!(values[values.len() - 2..], [(2000, 80, 80), (50, 10000, 203)]);
    assert_ne!(harness.client.color.ready[&501], harness.client.color.ready[&502]);
    old.destroy(); current.destroy(); output.destroy(); harness.sync();
}

#[test]
fn prepared_hdr_capabilities_are_fixed_at_global_creation_and_sdr_cannot_upgrade_late() {
    let hdr = hdr_harness();
    assert!(hdr.client.color.features.contains(&manager::Feature::SetMasteringDisplayPrimaries));
    assert!(hdr.client.color.primaries.contains(&manager::Primaries::Bt2020));
    assert!(hdr.client.color.transfers.contains(&manager::TransferFunction::St2084Pq));
    assert!(hdr.client.color.transfers.contains(&manager::TransferFunction::ExtLinear));
    assert!(!hdr.client.color.features.contains(&manager::Feature::WindowsScrgb));
    let mut sdr = harness();
    assert!(sdr.server.enable_hdr_color_management().is_err());
    sdr.sync();
    assert!(!sdr.client.color.transfers.contains(&manager::TransferFunction::St2084Pq));
}

#[test]
fn hdr_metadata_commits_as_one_immutable_description_and_reused_pixels_keep_their_version() {
    let mut harness = hdr_harness();
    let window = harness.window();
    let _buffer = harness.attach(&window, [16, 16]);
    let previous = harness.server.windows().remove(0);
    let manager = harness.client.color.manager.as_ref().unwrap().clone();
    let surface = manager.get_surface(&window.surface, &harness.queue.handle(), ());
    for (key, fall) in [(11, 400), (12, 300)] {
        let params = manager.create_parametric_creator(&harness.queue.handle(), ());
        params.set_mastering_display_primaries(680000, 320000, 265000, 690000, 150000, 60000, 312700, 329000);
        params.set_mastering_luminance(50, 1000);
        params.set_max_cll(1000); params.set_max_fall(fall);
        params.set_luminances(50, 7, 203);
        params.set_tf_named(manager::TransferFunction::St2084Pq);
        params.set_primaries_named(manager::Primaries::Bt2020);
        let image = params.create(&harness.queue.handle(), key);
        harness.sync();
        assert!(harness.client.color.ready.contains_key(&key));
        surface.set_image_description(&image, manager::RenderIntent::Perceptual);
        image.destroy(); window.surface.commit(); harness.sync();
        let current = harness.server.windows().remove(0);
        assert_eq!(current.nodes[0].content.version, previous.nodes[0].content.version);
        assert_eq!(current.nodes[0].color.transfer, Transfer::Pq);
        assert_eq!(current.nodes[0].color.maximum_luminance, 10000);
        assert_eq!(current.nodes[0].color.mastering.max_cll, Some(1000));
        assert_eq!(current.nodes[0].color.mastering.max_fall, Some(fall));
        assert_eq!(current.nodes[0].color.mastering.primaries, Some(Primaries::DisplayP3.chromaticities()));
    }
    assert_ne!(harness.client.color.ready[&11], harness.client.color.ready[&12]);
    assert_eq!(previous.nodes[0].color, Description::default());
    surface.destroy(); window.surface.commit(); harness.sync();
    assert_eq!(harness.server.windows()[0].nodes[0].color, Description::default());
}

#[test]
fn hdr_invalid_or_repeated_light_metadata_is_rejected_without_retaining_a_ready_object() {
    for invalid in 0..5 {
        let mut harness = hdr_harness();
        let params = harness.client.color.manager.as_ref().unwrap().create_parametric_creator(&harness.queue.handle(), ());
        params.set_tf_named(manager::TransferFunction::St2084Pq);
        params.set_primaries_named(manager::Primaries::Bt2020);
        match invalid {
            0 => { params.set_max_cll(1000); params.set_max_fall(1001); }
            1 => { params.set_max_cll(1000); params.set_max_cll(1000); }
            2 => params.set_mastering_luminance(10000000, 1000),
            3 => { params.set_mastering_luminance(50, 500); params.set_max_cll(1000); }
            _ => { params.set_mastering_display_primaries(708000, 292000, 170000, 797000, 131000, 46000, 312700, 329000);
                params.set_mastering_display_primaries(708000, 292000, 170000, 797000, 131000, 46000, 312700, 329000); }
        }
        let _image = params.create(&harness.queue.handle(), 80);
        let failure = synchronize(&mut harness.server, &harness.connection, &mut harness.queue, &mut harness.client).unwrap_err();
        assert!(!failure.contains("timed out"));
        assert!(!harness.client.color.ready.contains_key(&80));
        let mut recovered = Harness::connect(harness.server);
        let _image = description(&mut recovered, 90, manager::Primaries::Bt2020, manager::TransferFunction::St2084Pq);
    }
}
