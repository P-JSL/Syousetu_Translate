pub(super) const REVIEW_MARKER: &str = "[검수 필요]";

pub(super) fn japanese(text: &str) -> bool {
    text.chars()
        .any(|c| matches!(c,'\u{3040}'..='\u{30ff}'|'\u{3400}'..='\u{9fff}'))
}
pub(super) fn split_boundary(text: &str) -> Option<usize> {
    let length = text.chars().count();
    if length < 2 {
        return None;
    }
    let mid = length / 2;
    text.char_indices()
        .enumerate()
        .take(mid + mid / 2 + 1)
        .skip((mid / 2).max(1))
        .min_by_key(|(i, (_, c))| {
            (
                if "。！？.!?\n".contains(*c) {
                    0
                } else if c.is_whitespace() || "、,;；".contains(*c) {
                    1
                } else {
                    2
                },
                i.abs_diff(mid),
            )
        })
        .map(|(_, (byte, c))| byte + c.len_utf8())
        .filter(|&byte| byte < text.len())
        .or_else(|| text.char_indices().nth(mid).map(|(byte, _)| byte))
}
