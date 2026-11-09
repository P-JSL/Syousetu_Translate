//! Bounded translation chunks with an all-or-nothing output transaction.
use super::session::Session;
use crate::{
    config::Config,
    storage::{decode_text, AtomicFile},
};
use anyhow::{bail, Result};
use std::{fs, io::Write, path::Path};
pub fn translate_file(
    input: &Path,
    output: &Path,
    config: &Config,
    session: &mut Session,
    encoding: Option<&str>,
    progress: impl Fn(usize, usize),
) -> Result<()> {
    if input == output
        || output
            .canonicalize()
            .ok()
            .is_some_and(|path| input.canonicalize().ok() == Some(path))
    {
        bail!("입력 파일에 직접 덮어쓸 수 없습니다. 다른 출력 경로를 지정하세요");
    }
    let contents = decode_text(fs::read(input)?, encoding)?;
    let contents = contents.trim_start_matches('\u{feff}');
    let total = contents.split_inclusive('\n').count();
    let mut lines = contents.split_inclusive('\n');
    let mut output = AtomicFile::new(output)?;
    let mut done = 0;
    loop {
        let chunk = lines.by_ref().take(128).collect::<Vec<_>>();
        if chunk.is_empty() {
            break;
        }
        let source = chunk
            .iter()
            .map(|line| {
                line.trim_end_matches('\n')
                    .trim_end_matches('\r')
                    .to_owned()
            })
            .collect::<Vec<_>>();
        let translated = session.translate_lines(&source)?;
        anyhow::ensure!(
            chunk.len() == translated.len(),
            "파일 원문·번역 줄 수가 다릅니다. 불완전한 결과는 저장하지 않습니다"
        );
        for (line, text) in chunk.iter().zip(translated) {
            if config.print_japanese {
                writeln!(
                    output,
                    "{}",
                    line.trim_end_matches('\n').trim_end_matches('\r')
                )?;
            }
            output.write_all(text.as_bytes())?;
            if line.ends_with('\n') {
                output.write_all(b"\n")?;
            }
        }
        done += chunk.len();
        progress(done, total);
    }
    output.commit()
}
