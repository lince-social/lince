use super::*;
#[cfg(feature = "nested")]
use smithay::backend::allocator::{Buffer, dmabuf::Dmabuf};
use wayland_protocols::wp::linux_dmabuf::zv1::client::{zwp_linux_buffer_params_v1 as params, zwp_linux_dmabuf_v1 as manager};

#[derive(Default)]
pub(super) struct Capture {
    pub(super) manager: Option<manager::ZwpLinuxDmabufV1>,
    pub(super) modifiers: std::collections::BTreeSet<(u32, u64)>,
}

pub(super) fn bind(client: &mut Client, registry: &wl_registry::WlRegistry, name: u32, version: u32, qh: &QueueHandle<Client>) {
    client.dmabuf.manager = Some(registry.bind(name, version.min(3), qh, ()));
}

impl Dispatch<manager::ZwpLinuxDmabufV1, ()> for Client {
    fn event(client: &mut Self, _: &manager::ZwpLinuxDmabufV1, event: manager::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let manager::Event::Modifier { format, modifier_hi, modifier_lo } = event {
            client.dmabuf.modifiers.insert((format, (u64::from(modifier_hi) << 32) | u64::from(modifier_lo)));
        }
    }
}

#[cfg(feature = "nested")]
pub(super) fn buffer(client: &Client, queue: &QueueHandle<Client>, image: &Dmabuf) -> wl_buffer::WlBuffer {
    let format = image.format();
    let modifier = u64::from(format.modifier);
    assert!(client.dmabuf.modifiers.contains(&(format.code as u32, modifier)), "The client cannot use an unadvertised DMA-BUF format/modifier");
    let parameters = client.dmabuf.manager.as_ref().unwrap().create_params(queue, ());
    assert_eq!(image.num_planes(), 1);
    parameters.add(image.handles().next().unwrap(), 0, image.offsets().next().unwrap(), image.strides().next().unwrap(),
        (modifier >> 32) as u32, modifier as u32);
    let buffer = parameters.create_immed(image.size().w, image.size().h, format.code as u32, params::Flags::empty(), queue, ());
    parameters.destroy();
    buffer
}

delegate_noop!(Client: ignore params::ZwpLinuxBufferParamsV1);
