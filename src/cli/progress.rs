use super::args::flag;
pub(super) fn emit_progress(args: &[String], done: usize, total: usize, unit: &str) {
    if flag(args, "--progress-json") {
        eprintln!(
            "{}",
            serde_json::json!({"event":"progress","done":done,"total":total,"unit":unit})
        );
    }
}
