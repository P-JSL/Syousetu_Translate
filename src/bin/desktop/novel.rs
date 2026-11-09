use super::widgets::row;
use super::Desktop;
use eframe::egui;
use std::ffi::OsString;

impl Desktop {
    pub(super) fn novel(&mut self, ui: &mut egui::Ui) {
        ui.heading("웹소설 다운로드 및 번역");
        ui.label("Kakuyomu · Alphapolis · Syosetu · Syosetu18 · Hameln · Novelup");
        self.model_tools(ui);
        row(ui, "작품 URL", &mut self.url);
        if ui.button("목차 확인").clicked() {
            if self.url.trim().is_empty() {
                self.message = "작품 URL을 입력해 주세요.".into();
            } else {
                self.info_job = self.enqueue(
                    "목차 확인".into(),
                    vec!["inspect".into(), "--url".into(), self.url.trim().into()],
                    false,
                );
            }
        }
        if !self.metadata.is_empty() {
            ui.label(&self.metadata);
        }
        ui.horizontal(|ui| {
            ui.label("시작 화");
            ui.add(egui::DragValue::new(&mut self.start).range(1..=1_000_000));
            ui.label("마지막 화 (0: 끝까지)");
            ui.add(egui::DragValue::new(&mut self.end).range(0..=1_000_000));
        });
        row(ui, "출력 파일", &mut self.output);
        ui.horizontal(|ui| {
            if ui.button("저장 위치 선택").clicked() {
                if let Some(path) = tinyfiledialogs::save_file_dialog(
                    "Save translation",
                    &self
                        .config
                        .download_directory()
                        .join("novel.txt")
                        .to_string_lossy(),
                ) {
                    self.output = path;
                }
            }
            ui.checkbox(
                &mut self.config.save_without_file_chooser,
                "파일명 자동 생성 (출력 파일을 비운 경우)",
            );
        });
        if ui.button("번역 실행").clicked() {
            if self.url.trim().is_empty() {
                self.message = "작품 URL을 입력해 주세요.".into();
                return;
            }
            if self.end != 0 && self.end < self.start {
                self.message = "마지막 화는 시작 화보다 작을 수 없습니다.".into();
                return;
            }
            let mut args: Vec<OsString> = vec![
                "download".into(),
                "--url".into(),
                self.url.trim().into(),
                "--start".into(),
                self.start.to_string().into(),
            ];
            if self.end != 0 {
                args.extend(["--end".into(), self.end.to_string().into()]);
            }
            if !self.output.trim().is_empty() {
                args.extend(["--out".into(), self.output.clone().into()]);
            } else if self.config.save_without_file_chooser {
                args.push("--auto-output".into());
            } else {
                let Some(path) = tinyfiledialogs::save_file_dialog(
                    "Save translated novel",
                    &self
                        .config
                        .download_directory()
                        .join("novel.txt")
                        .to_string_lossy(),
                ) else {
                    return;
                };
                self.output = path;
                args.extend(["--out".into(), self.output.clone().into()]);
            }
            self.enqueue(self.url.clone(), args, true);
        }
    }

    pub(super) fn model_tools(&mut self, ui: &mut egui::Ui) {
        let status = syousetu_translate::models::status(&self.config.model_directory());
        ui.label(if status.ready {
            "SMaLL-100 INT8 준비됨 · 번역은 오프라인".to_owned()
        } else {
            format!(
                "SMaLL-100 INT8 직접 다운로드 · 최대 약 {}MB",
                status.download_bytes / 1_000_000
            )
        });
        if ui.button("모델 준비 / 복구").clicked() {
            self.enqueue(
                "내부 번역 모델 준비".into(),
                vec![
                    "models".into(),
                    "--prepare".into(),
                    "--progress-json".into(),
                ],
                false,
            );
        }
    }
}
