use crate::gui::views;

pub struct PerformanceInfoWindow {
    pub open: bool,
    pub delta: String,
    pub fps: String,
}

impl PerformanceInfoWindow {
    pub fn new() -> Self {
        PerformanceInfoWindow {
            open: false,
            delta: String::from("0"),
            fps: String::from("0"),
        }
    }

    pub fn set_global_performance(&mut self, delta: f32) {
        self.delta = format!("{:.3}ms", delta);
        self.fps = format!("{} FPS", (1.0 / delta) as u16);
    }

    pub fn render(&mut self, context: &egui::Context) {
        let delta = &self.delta;
        let fps = &self.fps;
        egui::Window::new("Performance Info")
            .resizable(true)
            .open(&mut self.open)
            .show(context, |ui| {
                ui.vertical(|ui| {
                    views::render_label_and_text(ui, "Delta:", delta);
                    views::render_label_and_text(ui, "FPS:", fps);
                });
            });
    }
}
