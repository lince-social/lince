#![forbid(unsafe_code)]

pub mod configuration;
pub mod profile;
pub mod project;
pub mod serial;

pub const SERIAL_LICENSE: &str = include_str!("../LICENSE-serialport.txt");
pub const SDK_LICENSE: &str = include_str!("../firmware/esp-idf/licenses/esp-idf-Apache-2.0.txt");
pub const CAMERA_LICENSE: &str =
    include_str!("../firmware/esp-idf/licenses/esp32-camera-Apache-2.0.txt");
pub const WEBSOCKET_LICENSE: &str =
    include_str!("../firmware/esp-idf/licenses/esp-websocket-client-Apache-2.0.txt");
pub const JPEG_LICENSE: &str = include_str!("../firmware/esp-idf/licenses/esp-jpeg-Apache-2.0.txt");
pub const TJPGDEC_LICENSE: &str = include_str!("../firmware/esp-idf/licenses/TJpgDec.txt");
pub const IROH_LICENSE: &str = include_str!("../firmware/iroh/licenses/iroh-MIT.txt");
pub const IROH_PROVIDER_LICENSE: &str =
    include_str!("../firmware/iroh/licenses/rusty-esp-iroh-MIT.txt");
pub const RUSTLS_LICENSE: &str = include_str!("../firmware/iroh/licenses/rustls-MIT.txt");
pub const RUSTCRYPTO_LICENSE: &str =
    include_str!("../firmware/iroh/licenses/rustls-rustcrypto-MIT.txt");

pub type Result<T> = std::result::Result<T, String>;
