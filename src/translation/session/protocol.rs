use super::{Session, MAX_MESSAGE};
use anyhow::{bail, Result};
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
pub fn serve(session: Result<Session>) -> Result<()> {
    let mut output = BufWriter::new(std::io::stdout().lock());
    let mut session = match session {
        Ok(session) => session,
        Err(error) => {
            writeln!(
                output,
                "{}",
                serde_json::json!({"ready":false,"error":format!("{error:#}")})
            )?;
            output.flush()?;
            return Err(error);
        }
    };
    writeln!(output, "{{\"ready\":true}}")?;
    output.flush()?;
    let mut input = BufReader::new(std::io::stdin().lock());
    loop {
        let mut message = String::new();
        if input
            .by_ref()
            .take(MAX_MESSAGE as u64 + 1)
            .read_line(&mut message)?
            == 0
        {
            break;
        }
        if message.len() > MAX_MESSAGE || !message.ends_with('\n') {
            bail!("worker 입력 용량 초과 또는 불완전한 입력");
        }
        let translated = match serde_json::from_str::<String>(&message) {
            Ok(text) => match session.translate(&text) {
                Ok(text) => serde_json::json!({"output":text}),
                Err(error) => serde_json::json!({"error":format!("{error:#}")}),
            },
            Err(error) => serde_json::json!({"error":format!("worker JSON 입력 오류: {error}")}),
        };
        let message = serde_json::to_string(&translated)?;
        if message.len() >= MAX_MESSAGE {
            bail!("worker 결과 용량 초과");
        }
        writeln!(output, "{message}")?;
        output.flush()?;
    }
    Ok(())
}
