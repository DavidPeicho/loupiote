use crate::gui::{views, GUIContext};

#[derive(Default)]
pub struct PerformanceInfoWindow {
    pub open: bool,
    pub delta: String,
    pub fps: String,
}

impl PerformanceInfoWindow {
    pub fn set_global_performance(&mut self, delta: f32) {
        self.delta = format!("{:.3}ms", delta);
        self.fps = format!("{} FPS", (1.0 / delta) as u16);
    }

    pub fn render(&mut self, context: &GUIContext, egui_ctx: &egui::Context) {
        let delta = &self.delta;
        let fps = &self.fps;
        let timestamp_values = context.renderer.queries.values();
        let timestamp_labels = context.renderer.queries.labels();
        egui::Window::new("Performance Info")
            .resizable(true)
            .open(&mut self.open)
            .show(egui_ctx, |ui| {
                ui.vertical(|ui: &mut egui::Ui| {
                    views::render_label_and_text(ui, "Delta:", delta);
                    views::render_label_and_text(ui, "FPS:", fps);
                    Self::render_timestamps(ui, timestamp_values, timestamp_labels);
                });
            });
    }

    fn render_timestamps(ui: &mut egui::Ui, values: &[f64], labels: &[String]) {
        ui.separator();
        ui.heading("Timestamps (GPU)");
        for i in 0..values.len() {
            let value = format!("{:.4} ms", values[i]);
            views::render_label_and_text(ui, &labels[i], &value);
        }
    }
}
