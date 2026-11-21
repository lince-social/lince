// non-lean-yet
// Scope: fn from_proxy.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 5.

use bevy::winit::{EventLoopProxyWrapper, WinitUserEvent};
pub use lince_interface::wake::WakeSignal;

pub fn from_proxy(proxy: &EventLoopProxyWrapper) -> WakeSignal {
    let proxy = (**proxy).clone();
    WakeSignal::new(move || {
        let _ = proxy.send_event(WinitUserEvent::WakeUp);
    })
}
