use super::*;

struct Scripted(Vec<usize>);
impl Translator for Scripted {
    fn translate(&mut self, text: &str) -> Result<String> {
        Ok(text.to_owned())
    }
    fn translate_lines_into(
        &mut self,
        _: &[String],
        emit: &mut dyn FnMut(usize, &str) -> Result<()>,
    ) -> Result<()> {
        for &index in &self.0 {
            emit(index, "번역")?;
        }
        Ok(())
    }
}

#[test]
fn malformed_streams_expose_only_the_valid_prefix_and_fail() {
    let input = ["一".to_owned(), "二".to_owned()];
    for (indices, expected) in [
        (vec![1, 0], vec![]),
        (vec![0, 0], vec![0]),
        (vec![0, 2], vec![0]),
        (vec![0], vec![0]),
        (vec![0, 1, 2], vec![0, 1]),
    ] {
        let mut session = Session::direct(Box::new(Scripted(indices)));
        let mut visible = Vec::new();
        assert!(session
            .translate_lines_into(&input, &mut |index, _| {
                visible.push(index);
                Ok(())
            })
            .is_err());
        assert_eq!(visible, expected);
    }
}

#[test]
fn output_failure_stops_emission_and_reaches_the_caller() {
    let mut session = Session::direct(Box::new(Scripted(vec![0, 1])));
    let mut calls = 0;
    let error = session
        .translate_lines_into(&["一".into(), "二".into()], &mut |_, _| {
            calls += 1;
            anyhow::bail!("disk full")
        })
        .unwrap_err();
    assert_eq!(calls, 1);
    assert_eq!(error.to_string(), "disk full");
}
