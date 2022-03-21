use crate::gui::views;

pub struct InfoWindowGUI {
  pub open: bool,
  pub adapter_name: String,
  pub meshes_count: String,
  pub bvh_nodes_count: String,
  pub delta: String,
  pub fps: String,
}

impl InfoWindowGUI {
  pub fn new() -> Self {
    InfoWindowGUI {
      open: true,
      adapter_name: String::from(""),
      meshes_count: String::from("0"),
      bvh_nodes_count: String::from("0"),
      delta: String::from("0"),
      fps: String::from("0"),
    }
  }

  pub fn set_meshes_count(&mut self, count: usize) {
    self.meshes_count = count.to_string();
  }

  pub fn set_bvh_nodes_count(&mut self, count: usize) {
    self.bvh_nodes_count = count.to_string();
  }

  pub fn set_global_performance(&mut self, delta: f64) {
    self.delta = format!("{:.3}ms", delta);
    self.fps = format!("{} FPS", (1000.0 / delta) as u16);
  }

  pub fn render(&mut self, context: &egui::Context) {
    let adapter = &self.adapter_name;
    let meshes_count = &self.meshes_count;
    let bvh_nodes_count = &self.bvh_nodes_count;
    let delta = &self.delta;
    let fps = &self.fps;
    let mut window = egui::Window::new("Info")
      .resizable(true)
      .open(&mut self.open)
      .show(context, |ui| {
        ui.vertical(|ui| {
          ui.vertical_centered(|ui| {
            ui.heading("General");
          });
          views::render_label_and_text(ui, "Adapter:", adapter);
          ui.separator();
          ui.vertical_centered(|ui| {
            ui.heading("Performance");
          });
          views::render_label_and_text(ui, "Delta:", delta);
          views::render_label_and_text(ui, "FPS:", fps);
          ui.separator();
          ui.vertical_centered(|ui| {
            ui.heading("Scene");
          });
          views::render_label_and_text(ui, "Meshes Count:", meshes_count);
          views::render_label_and_text(ui, "BVH Nodes Count:", bvh_nodes_count);
        });
      });
  }
}
