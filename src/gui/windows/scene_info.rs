use crate::gui::views;

pub struct SceneInfoWindow {
    pub open: bool,
    pub adapter_name: String,
    pub meshes_count: String,
    pub bvh_nodes_count: String,
}

impl SceneInfoWindow {
    pub fn new() -> Self {
        SceneInfoWindow {
            open: false,
            adapter_name: String::from(""),
            meshes_count: String::from("0"),
            bvh_nodes_count: String::from("0"),
        }
    }

    pub fn set_meshes_count(&mut self, count: usize) {
        self.meshes_count = count.to_string();
    }

    pub fn set_bvh_nodes_count(&mut self, count: usize) {
        self.bvh_nodes_count = count.to_string();
    }

    pub fn render(&mut self, context: &egui::Context) {
        let adapter = &self.adapter_name;
        let meshes_count = &self.meshes_count;
        let bvh_nodes_count = &self.bvh_nodes_count;
        let mut window = egui::Window::new("Scene Info")
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
                        ui.heading("Scene");
                    });
                    views::render_label_and_text(ui, "Meshes Count:", meshes_count);
                    views::render_label_and_text(ui, "BVH Nodes Count:", bvh_nodes_count);
                });
            });
    }
}
