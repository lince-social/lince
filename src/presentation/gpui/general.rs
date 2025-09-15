use gpui::Application;
use std::io::Error;

pub fn general_gpui() -> Result<(), Error> {
    Application::new().run().map_err(Error::other);
}
