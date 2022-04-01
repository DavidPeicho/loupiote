pub struct ErrorWindow {
    pub open: bool,
    pub message: String,
}

impl ErrorWindow {
    pub fn new(message: String) -> Self {
        ErrorWindow {
            message,
            open: true,
        }
    }

    pub fn render(&mut self, context: &egui::Context) {
        let message = &self.message;
        egui::Window::new("Performance Info")
            .resizable(true)
            .open(&mut self.open)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(context, |ui| {
                ui.label(egui::RichText::new(message).color(egui::Color32::RED));
            });
    }
}
