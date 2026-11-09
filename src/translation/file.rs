//! Bounded translation chunks written directly to a visible output file.
use super::session::Session;
use crate::{
    config::Config,
    storage::{decode_text, StreamingFile},
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
    let mut output = StreamingFile::create(output)?;
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
        session.translate_lines_into(&source, &mut |index, text| {
            let line = chunk[index];
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
            if !text.trim().is_empty() {
                output.flush()?;
            }
            progress(done + index + 1, total);
            Ok(())
        })?;
        done += chunk.len();
        output.flush()?;
    }
    output.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::translation::engine::Translator;
    use std::sync::mpsc;
    #[test]
    fn final_txt_is_readable_while_the_next_translation_is_still_running() {
        struct Paused {
            ready: mpsc::Sender<()>,
            resume: mpsc::Receiver<()>,
        }
        impl Translator for Paused {
            fn translate(&mut self, text: &str) -> Result<String> {
                Ok(text.to_owned())
            }
            fn translate_lines_into(
                &mut self,
                lines: &[String],
                emit: &mut dyn FnMut(usize, &str) -> Result<()>,
            ) -> Result<()> {
                for index in 0..lines.len() {
                    if index == 1 {
                        self.ready.send(())?;
                        self.resume
                            .recv_timeout(std::time::Duration::from_secs(5))?;
                    }
                    emit(index, &format!("번역{index}"))?;
                }
                Ok(())
            }
        }
        let dir = std::env::temp_dir().join(format!("syousetu-live-file-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let input = dir.join("input.txt");
        let output = dir.join("output.txt");
        fs::write(&input, "一\n二\n").unwrap();
        let (ready_tx, ready_rx) = mpsc::channel();
        let (resume_tx, resume_rx) = mpsc::channel();
        let worker_input = input.clone();
        let worker_output = output.clone();
        let worker = std::thread::spawn(move || {
            let mut session = Session::direct(Box::new(Paused {
                ready: ready_tx,
                resume: resume_rx,
            }));
            translate_file(
                &worker_input,
                &worker_output,
                &Config {
                    print_japanese: true,
                    ..Config::default()
                },
                &mut session,
                None,
                |_, _| {},
            )
        });
        ready_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        assert_eq!(fs::read_to_string(&output).unwrap(), "一\n번역0\n");
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 2);
        resume_tx.send(()).unwrap();
        worker.join().unwrap().unwrap();
        assert_eq!(
            fs::read_to_string(&output).unwrap(),
            "一\n번역0\n二\n번역1\n"
        );
        assert_eq!(fs::read_to_string(input).unwrap(), "一\n二\n");
        fs::remove_dir_all(dir).unwrap();
    }
}
