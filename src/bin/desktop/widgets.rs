use eframe::egui;
pub(super) fn row(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.label(label);
    ui.add(egui::TextEdit::singleline(value).desired_width(f32::INFINITY));
}
pub(super) fn row_option(ui: &mut egui::Ui, label: &str, value: &mut Option<String>) {
    let text = value.get_or_insert_with(String::new);
    row(ui, label, text);
    if text.is_empty() {
        *value = None;
    }
}
