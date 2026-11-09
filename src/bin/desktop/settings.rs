use super::widgets::{row, row_option};
use super::Desktop;
use eframe::egui;
use syousetu_translate::{config::Config, storage::atomic_write};

impl Desktop {
    pub(super) fn settings(&mut self, ui: &mut egui::Ui) {
        ui.heading("설정");
        row(ui, "설정 파일", &mut self.config_path);
        row_option(ui, "다운로드 기본 폴더", &mut self.config.download_dir);
        ui.label(format!(
            "기본 저장 위치: {}",
            self.config.download_directory().display()
        ));
        ui.horizontal(|ui| {
            if ui.button("불러오기").clicked() {
                match Config::load(std::path::Path::new(&self.config_path)) {
                    Ok(mut config) => {
                        if matches!(config.translator_name.as_str(), "native" | "eztrans") {
                            config.translator_name = "internal".into();
                        }
                        self.config = config;
                        self.message = "설정을 불러왔습니다.".into();
                    }
                    Err(e) => self.message = format!("{e:#}"),
                }
            }
            if ui.button("저장").clicked() {
                let result = self
                    .config
                    .to_properties()
                    .and_then(|text| atomic_write(std::path::Path::new(&self.config_path), &text));
                self.message = match result {
                    Ok(()) => "설정을 저장했습니다. 이미 추가된 작업의 설정은 유지됩니다.".into(),
                    Err(e) => format!("{e:#}"),
                };
            }
        });
        egui::ComboBox::from_label("번역기")
            .selected_text(&self.config.translator_name)
            .show_ui(ui, |ui| {
                for name in ["internal", "passthrough"] {
                    ui.selectable_value(&mut self.config.translator_name, name.into(), name);
                }
            });
        ui.label("internal: Rust 내부 오프라인 번역 · passthrough: 원문 저장");
        row_option(
            ui,
            "모델 폴더 (비우면 내장 모델 자동 선택)",
            &mut self.config.model_dir,
        );
        self.model_tools(ui);
        row_option(
            ui,
            "교정 기억 파일 (비우면 사용자 데이터 폴더)",
            &mut self.config.learning_path,
        );
        row_option(
            ui,
            "추가 CA 인증서 (필요한 경우 PEM 파일)",
            &mut self.config.ca_bundle,
        );
        ui.label("Windows의 신뢰된 인증서를 기본 사용합니다. 사내망·보안 프로그램의 추가 CA만 지정해 주세요.");
        ui.checkbox(&mut self.config.learning_enabled, "교정 기억 적용");
        egui::ComboBox::from_label("본문 처리 방식")
            .selected_text(&self.config.parallel_mode)
            .show_ui(ui, |ui| {
                for (value, label) in [
                    ("auto", "자동 성능 확인"),
                    ("serial", "단일"),
                    ("parallel", "병렬"),
                ] {
                    ui.selectable_value(&mut self.config.parallel_mode, value.into(), label);
                }
            });
        ui.label("자동: 첫 본문 묶음의 속도와 출력 일치를 확인하고 빠른 방식을 선택합니다.");
        ui.checkbox(
            &mut self.config.fast_vocabulary,
            "한국어 중심 고속 어휘 (실험 · 번역 표현이 달라질 수 있습니다)",
        );
        ui.checkbox(&mut self.config.print_japanese, "일본어 동시 출력");
        ui.checkbox(&mut self.config.include_furigana, "후리가나 포함");
        ui.checkbox(&mut self.config.include_comment, "작가 코멘트 포함");
        ui.checkbox(
            &mut self.config.filter_title_bracket,
            "파일명 앞의 【분류】 제거",
        );
        ui.checkbox(
            &mut self.config.save_without_file_chooser,
            "파일명 자동 생성",
        );
        ui.label("화 제목 형식: [currentPage] [lastPage] [koreanTitle] [japaneseTitle]");
        ui.text_edit_multiline(&mut self.config.title_format);
        row(ui, "화 번호 형식", &mut self.config.page_number_format);
        ui.label("작가 코멘트 구분자");
        ui.text_edit_multiline(&mut self.config.comment_separator);
        egui::Grid::new("settings-numbers")
            .num_columns(2)
            .show(ui, |ui| {
                ui.label("다운로드 간격 (ms)");
                ui.add(
                    egui::DragValue::new(&mut self.config.page_download_delay).range(0..=u32::MAX),
                );
                ui.end_row();
                ui.label("Hameln 간격 (ms)");
                ui.add(
                    egui::DragValue::new(&mut self.config.hameln_page_download_delay)
                        .range(0..=u32::MAX),
                );
                ui.end_row();
                ui.label("재시도 횟수");
                ui.add(egui::DragValue::new(&mut self.config.max_retry_count).range(0..=10));
                ui.end_row();
                ui.label("번역 작업자 수 (-1: 기본 32개, 최대 64개)");
                ui.add(egui::DragValue::new(&mut self.config.max_cpu_core_count).range(-1..=64));
                ui.end_row();
                ui.label("파일 제목 최대 길이");
                ui.add(egui::DragValue::new(&mut self.config.max_title_length).range(1..=255));
                ui.end_row();
            });
        row(ui, "User Agent", &mut self.config.user_agent);
    }
}
