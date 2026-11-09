use eframe::egui;
use std::path::PathBuf;
pub(super) fn load_font(context: &egui::Context) {
    let mut candidates = Vec::new();
    if let Some(path) = std::env::var_os("SYOUSETU_FONT") {
        candidates.push(PathBuf::from(path));
    }
    if let Some(windows) = std::env::var_os("WINDIR") {
        candidates.push(PathBuf::from(windows).join("Fonts/malgun.ttf"));
    }
    candidates.extend([
        PathBuf::from("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc"),
        PathBuf::from("/usr/share/fonts/opentype/noto/NotoSansCJKkr-Regular.otf"),
        PathBuf::from("/System/Library/Fonts/AppleSDGothicNeo.ttc"),
    ]);
    for path in candidates {
        if let Ok(bytes) = std::fs::read(path) {
            let mut fonts = egui::FontDefinitions::default();
            fonts
                .font_data
                .insert("cjk".into(), egui::FontData::from_owned(bytes).into());
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, "cjk".into());
            fonts
                .families
                .entry(egui::FontFamily::Monospace)
                .or_default()
                .push("cjk".into());
            context.set_fonts(fonts);
            break;
        }
    }
}
