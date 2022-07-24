use crate::errors::Error;

mod render_settings;

pub fn render_toolbar_gui(
    ui: &mut egui::Ui,
    app_context: &mut crate::ApplicationContext,
) -> Result<(), Error> {
    ui.menu_button("Rendering", |ui| {
        render_settings::render_settings_toolbar_gui(ui, app_context);
    });
    Ok({})
}
