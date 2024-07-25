use albedo_lib::BlitMode;

pub fn render_settings_toolbar_gui(ui: &mut egui::Ui, settings: &mut crate::Settings) {
    ui.checkbox(&mut settings.accumulate, "Accumulate");
    ui.checkbox(&mut settings.use_blue_noise, "Use Blue Noise");
    egui::ComboBox::from_label("Blit Mode")
        .selected_text(format!("{:?}", settings.blit_mode))
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut settings.blit_mode, BlitMode::Pahtrace, "Pathtrace");
            ui.selectable_value(&mut settings.blit_mode,  BlitMode::GBuffer, "GBuffer");
            ui.selectable_value(&mut settings.blit_mode,  BlitMode::MotionVector, "Motion Vectors");
        });
}
