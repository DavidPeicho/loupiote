pub fn render_settings_toolbar_gui(ui: &mut egui::Ui, settings: &mut crate::Settings) {
    ui.checkbox(&mut settings.accumulate, "Accumulate");
}
