use super::widgets::{row, row_option};
use super::Desktop;
use eframe::egui;
use syousetu_translate::learning::Memory;

impl Desktop {
    pub(super) fn learning(&mut self, ui: &mut egui::Ui) {
        ui.heading("교정 기억");
        ui.label("직접 고친 문장과 용어를 기억합니다. 모델 가중치를 재학습하지 않으며 자동 생성 결과는 학습하지 않습니다.");
        ui.label("저장한 교정은 이후 시작하는 번역 작업에 적용됩니다.");
        ui.checkbox(&mut self.config.learning_enabled, "교정 기억 적용");
        row_option(ui, "교정 기억 파일", &mut self.config.learning_path);
        ui.label(self.config.memory_file().display().to_string());
        if ui.button("EHND 사전에서 교정 가져오기").clicked() {
            if let Some(directory) =
                tinyfiledialogs::select_folder_dialog("Select extracted EHND dictionary folder", "")
            {
                let mut report = None;
                let result = Memory::edit(&self.config.memory_file(), |memory| {
                    report = Some(syousetu_translate::learning::import::import_ehnd(
                        std::path::Path::new(&directory),
                        memory,
                    )?);
                    Ok(())
                });
                match result {
                    Ok(memory) => {
                        self.memory = memory;
                        if let Some(report) = report {
                            self.message = format!(
                                "교정 {}개를 가져왔습니다. 기존 교정 {}개를 유지했습니다.",
                                report.inserted, report.retained_existing
                            );
                        }
                    }
                    Err(error) => self.message = format!("{error:#}"),
                }
            }
        }
        if ui.button("목록 새로고침").clicked() {
            match Memory::load(&self.config.memory_file()) {
                Ok(memory) => {
                    self.memory = memory;
                    self.message.clear();
                }
                Err(error) => self.message = format!("{error:#}"),
            }
        }
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.term_mode, false, "문장 교정");
            ui.selectable_value(&mut self.term_mode, true, "용어 교정");
        });
        ui.label(if self.term_mode {
            "일본어 용어 (이 원문이 포함된 경우에만 치환)"
        } else {
            "일본어 원문 (같은 문장에 적용)"
        });
        ui.text_edit_multiline(&mut self.source);
        if self.term_mode {
            row(ui, "기존 한국어 표현", &mut self.previous);
        }
        ui.label("교정한 한국어");
        ui.text_edit_multiline(&mut self.corrected);
        if ui.button("교정 기억 저장").clicked() {
            let result = Memory::edit(&self.config.memory_file(), |memory| {
                if self.term_mode {
                    memory.learn_term(&self.source, &self.previous, &self.corrected)
                } else {
                    memory.learn(&self.source, &self.corrected)
                }
            });
            match result {
                Ok(memory) => {
                    self.memory = memory;
                    self.message = "교정을 저장했습니다.".into();
                }
                Err(error) => self.message = format!("{error:#}"),
            }
        }
        ui.separator();
        ui.label(format!(
            "문장 {}개 · 용어 {}개",
            self.memory.sentences.len(),
            self.memory.terms.len()
        ));
        let mut delete = None;
        let height = ui.spacing().interact_size.y;
        egui::ScrollArea::vertical()
            .id_salt("sentence-corrections")
            .max_height(260.0)
            .show_rows(ui, height, self.memory.sentences.len(), |ui, range| {
                for (source, corrected) in self
                    .memory
                    .sentences
                    .iter()
                    .skip(range.start)
                    .take(range.len())
                {
                    ui.horizontal(|ui| {
                        if ui.small_button("수정").clicked() {
                            self.term_mode = false;
                            self.source = source.clone();
                            self.corrected = corrected.clone();
                        }
                        if ui.small_button("삭제").clicked() {
                            delete = Some((source.clone(), None));
                        }
                        ui.add(
                            egui::Label::new(format!(
                                "{} → {}",
                                source.lines().next().unwrap_or(""),
                                corrected.lines().next().unwrap_or("")
                            ))
                            .truncate(),
                        );
                    });
                }
            });
        egui::ScrollArea::vertical()
            .id_salt("term-corrections")
            .max_height(160.0)
            .show_rows(ui, height, self.memory.terms.len(), |ui, range| {
                for term in &self.memory.terms[range] {
                    ui.horizontal(|ui| {
                        if ui.small_button("수정").clicked() {
                            self.term_mode = true;
                            self.source = term.source.clone();
                            self.previous = term.previous.clone();
                            self.corrected = term.corrected.clone();
                        }
                        if ui.small_button("삭제").clicked() {
                            delete = Some((term.source.clone(), Some(term.previous.clone())));
                        }
                        ui.label(format!(
                            "{}: {} → {}",
                            term.source, term.previous, term.corrected
                        ));
                    });
                }
            });
        if let Some((source, previous)) = delete {
            match Memory::edit(&self.config.memory_file(), |m| {
                if let Some(previous) = previous {
                    m.terms
                        .retain(|t| t.source != source || t.previous != previous);
                } else {
                    m.sentences.remove(&source);
                }
                Ok(())
            }) {
                Ok(memory) => {
                    self.memory = memory;
                    self.message = "교정을 삭제했습니다.".into();
                }
                Err(error) => self.message = format!("{error:#}"),
            }
        }
    }
}
