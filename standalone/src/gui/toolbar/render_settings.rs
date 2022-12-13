use crate::{errors::Error, settings};

pub fn render_settings_toolbar_gui(
    ui: &mut egui::Ui,
    settings: &mut crate::Settings,
) -> Result<(), Error> {
    ui.checkbox(&mut settings.accumulate, "Accumulate");
    Ok({})
}
