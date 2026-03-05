use ratatui::Frame;

pub fn render(frame: &mut Frame) {
    frame.render_widget("Hellour world", frame.area());
}
