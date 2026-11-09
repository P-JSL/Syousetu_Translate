use super::super::{
    args::{require, value},
    factory::translation_session,
    options,
    progress::emit_progress,
};
use anyhow::Result;
use std::path::PathBuf;
pub(crate) fn file_translate(args: &[String]) -> Result<()> {
    let config = options::settings(args)?;
    let input = PathBuf::from(require(args, "--input")?);
    let output = PathBuf::from(require(args, "--out")?);
    let mut session = translation_session(args, &config)?;
    crate::translation::file::translate_file(
        &input,
        &output,
        &config,
        &mut session,
        value(args, "--input-encoding").as_deref(),
        |done, total| emit_progress(args, done, total, "lines"),
    )?;
    println!("Saved {}", output.display());
    Ok(())
}
