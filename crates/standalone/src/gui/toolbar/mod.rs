mod render_settings;

pub fn render_toolbar_gui(ui: &mut egui::Ui, settings: &mut crate::Settings) {
    ui.menu_button("Rendering", |ui| {
        render_settings::render_settings_toolbar_gui(ui, settings);
    });
}
