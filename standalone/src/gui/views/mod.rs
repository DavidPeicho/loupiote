pub fn render_label_and_text<L: AsRef<str>, T: AsRef<str>>(ui: &mut egui::Ui, label: L, text: T) {
    ui.horizontal(|ui| {
        ui.label(label.as_ref());
        ui.label(text.as_ref());
    });
}
