use anyhow::{Context, Result};
pub(super) fn value(args: &[String], key: &str) -> Option<String> {
    args.windows(2).find(|x| x[0] == key).map(|x| x[1].clone())
}
pub(super) fn flag(args: &[String], key: &str) -> bool {
    args.iter().any(|x| x == key)
}
pub(super) fn num(args: &[String], key: &str, default: usize) -> Result<usize> {
    value(args, key)
        .map(|v| {
            v.parse::<usize>()
                .with_context(|| format!("잘못된 숫자: {key}"))
        })
        .unwrap_or(Ok(default))
}
pub(super) fn require(args: &[String], key: &str) -> Result<String> {
    value(args, key).with_context(|| format!("필수 옵션 {key} 누락"))
}
