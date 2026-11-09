pub(super) fn japanese(text: &str) -> bool {
    text.chars()
        .any(|c| matches!(c,'\u{3040}'..='\u{30ff}'|'\u{3400}'..='\u{9fff}'))
}
pub(super) fn split_boundary(text: &str) -> Option<usize> {
    let positions = text.char_indices().collect::<Vec<_>>();
    let mid = positions.len() / 2;
    if positions.len() < 2 {
        return None;
    }
    positions
        .iter()
        .enumerate()
        .skip(1)
        .filter(|(i, _)| *i >= mid / 2 && *i <= mid + mid / 2)
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
        .or_else(|| positions.get(mid.max(1)).map(|(byte, _)| *byte))
}
