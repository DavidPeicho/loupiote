use crate::errors::Error;

pub fn render_settings_toolbar_gui(
    ui: &mut egui::Ui,
    app_context: &mut crate::ApplicationContext,
) -> Result<(), Error> {
    ui.checkbox(&mut app_context.settings.accumulate, "Accumulate");
    Ok({})
}
