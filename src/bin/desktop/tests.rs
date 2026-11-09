use super::*;
#[test]
fn desktop_tabs_render_without_a_display_server() {
    let context = egui::Context::default();
    let mut app = Desktop::new(&context);
    for tab in [Tab::Novel, Tab::Settings, Tab::Learning] {
        app.tab = tab;
        let output = context.run(egui::RawInput::default(), |context| app.draw(context));
        assert!(!output.shapes.is_empty());
    }
    assert!(app.queue.jobs.is_empty());
}
