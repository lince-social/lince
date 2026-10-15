use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};
pub use lince_interface::wake::WakeSignal;

pub fn from_proxy(proxy: &EventLoopProxyWrapper) -> WakeSignal {
    let proxy = (**proxy).clone();
    WakeSignal::new(move || {
        let _ = proxy.send_event(WinitUserEvent::WakeUp);
    })
}
