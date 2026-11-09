#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
//! Rust desktop replacement for translator7's web-novel workflow.
use eframe::egui;
use std::{ffi::OsString, path::PathBuf, time::Duration};
use syousetu_translate::{
    config::Config,
    jobs::{sibling_cli, Queue, Status},
    learning::Memory,
    storage::atomic_write,
};

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Novel,
    Settings,
    Learning,
}

struct Desktop {
    tab: Tab,
    config: Config,
    config_path: String,
    queue: Queue,
    selected: Option<u64>,
    info_job: Option<u64>,
    url: String,
    output: String,
    dictionary: String,
    start: usize,
    end: usize,
    metadata: String,
    message: String,
    memory: Memory,
    source: String,
    previous: String,
    corrected: String,
    term_mode: bool,
}

impl Desktop {
    fn new(context: &egui::Context) -> Self {
        load_font(context);
        let directory = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let path = directory.join("config/config.properties");
        let defaults = || {
            let mut config = Config::parse("").unwrap_or_default();
            config.translator_name = "internal".into();
            config
        };
        let (mut config, mut message) = if path.is_file() {
            match Config::load(&path) {
                Ok(config) => (config, String::new()),
                Err(e) => (
                    defaults(),
                    format!("설정 읽기 실패 (기존 파일은 보존됨): {e:#}"),
                ),
            }
        } else {
            (defaults(), String::new())
        };
        if matches!(config.translator_name.as_str(), "eztrans" | "native") {
            config.translator_name = "internal".into();
            message = "내부 번역 엔진으로 전환했습니다. 최초 1회 모델을 준비해 주세요.".into();
        }
        let memory = match Memory::load(&config.memory_file()) {
            Ok(memory) => memory,
            Err(error) => {
                message = format!("{error:#}");
                Memory::default()
            }
        };
        let executable = std::env::current_exe().unwrap_or_else(|_| directory.join("syousetu-gui"));
        Self {
            tab: Tab::Novel,
            config,
            config_path: path.to_string_lossy().into_owned(),
            queue: Queue::new(sibling_cli(&executable), directory),
            selected: None,
            info_job: None,
            url: String::new(),
            output: String::new(),
            dictionary: String::new(),
            start: 1,
            end: 0,
            metadata: String::new(),
            message,
            memory,
            source: String::new(),
            previous: String::new(),
            corrected: String::new(),
            term_mode: false,
        }
    }

    fn enqueue(
        &mut self,
        title: String,
        mut args: Vec<OsString>,
        translation: bool,
    ) -> Option<u64> {
        if translation {
            if !matches!(
                self.config.translator_name.as_str(),
                "internal" | "passthrough"
            ) {
                self.message =
                    "설정에서 internal(내부 번역) 또는 passthrough(원문 저장)를 선택해 주세요."
                        .into();
                return None;
            }
            if self.config.translator_name == "internal"
                && !syousetu_translate::models::status(&self.config.model_directory()).ready
            {
                self.message =
                    "먼저 '모델 준비 / 복구'를 실행해 주세요. 준비 후에는 오프라인으로 번역됩니다."
                        .into();
                return None;
            }
            args.push("--progress-json".into());
            if !self.dictionary.trim().is_empty() {
                args.extend(["--dictionary".into(), self.dictionary.clone().into()]);
            }
        }
        match self.queue.enqueue(title, args, &self.config) {
            Ok(id) => {
                self.selected = Some(id);
                self.message.clear();
                Some(id)
            }
            Err(e) => {
                self.message = format!("{e:#}");
                None
            }
        }
    }

    fn novel(&mut self, ui: &mut egui::Ui) {
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
                if let Some(path) =
                    tinyfiledialogs::save_file_dialog("Save translation", "novel.txt")
                {
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
                let Some(path) =
                    tinyfiledialogs::save_file_dialog("Save translated novel", "novel.txt")
                else {
                    return;
                };
                self.output = path;
                args.extend(["--out".into(), self.output.clone().into()]);
            }
            self.enqueue(self.url.clone(), args, true);
        }
    }

    fn settings(&mut self, ui: &mut egui::Ui) {
        ui.heading("설정");
        row(ui, "설정 파일", &mut self.config_path);
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
            "모델 폴더 (비우면 사용자 데이터 폴더)",
            &mut self.config.model_dir,
        );
        self.model_tools(ui);
        row_option(
            ui,
            "교정 기억 파일 (비우면 사용자 데이터 폴더)",
            &mut self.config.learning_path,
        );
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
                ui.label("번역 worker 수 (-1: 자동)");
                ui.add(egui::DragValue::new(&mut self.config.max_cpu_core_count).range(-1..=16));
                ui.end_row();
                ui.label("파일 제목 최대 길이");
                ui.add(egui::DragValue::new(&mut self.config.max_title_length).range(1..=255));
                ui.end_row();
            });
        row(ui, "User Agent", &mut self.config.user_agent);
    }

    fn model_tools(&mut self, ui: &mut egui::Ui) {
        let status = syousetu_translate::models::status(&self.config.model_directory());
        ui.label(if status.ready {
            "내부 모델 준비됨 · 번역은 오프라인".to_owned()
        } else {
            format!(
                "내부 모델 준비 필요 · 최초 다운로드 약 {}MB",
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

    fn learning(&mut self, ui: &mut egui::Ui) {
        ui.heading("교정 기억");
        ui.label("직접 고친 문장과 용어를 기억합니다. 모델 가중치를 재학습하지 않으며 자동 생성 결과는 학습하지 않습니다.");
        ui.label("저장한 교정은 이후 시작하는 번역 작업에 적용됩니다.");
        ui.checkbox(&mut self.config.learning_enabled, "교정 기억 적용");
        row_option(ui, "교정 기억 파일", &mut self.config.learning_path);
        ui.label(self.config.memory_file().display().to_string());
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
        for (source, corrected) in &self.memory.sentences {
            ui.horizontal(|ui| {
                if ui.small_button("수정").clicked() {
                    self.term_mode = false;
                    self.source = source.clone();
                    self.corrected = corrected.clone();
                }
                if ui.small_button("삭제").clicked() {
                    delete = Some((source.clone(), None));
                }
                ui.label(format!("{source} → {corrected}"));
            });
        }
        for term in &self.memory.terms {
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

    fn tasks(&mut self, ui: &mut egui::Ui) {
        ui.heading("작업 큐");
        egui::ScrollArea::vertical()
            .id_salt("task-list")
            .max_height(150.0)
            .show(ui, |ui| {
                for job in &self.queue.jobs {
                    let (label, color) = match job.status {
                        Status::Waiting => ("대기", egui::Color32::GRAY),
                        Status::Running => ("진행", egui::Color32::from_rgb(220, 150, 30)),
                        Status::Done => ("완료", egui::Color32::from_rgb(60, 170, 100)),
                        Status::Error => ("실패", egui::Color32::from_rgb(220, 70, 70)),
                    };
                    let text =
                        egui::RichText::new(format!("{} · {} · {}", job.id, label, job.title))
                            .color(color);
                    if ui
                        .selectable_label(self.selected == Some(job.id), text)
                        .clicked()
                    {
                        self.selected = Some(job.id);
                    }
                }
            });
        if let Some(id) = self.selected {
            ui.horizontal(|ui| {
                if ui.button("대기 작업 위로").clicked() {
                    self.queue.move_waiting(id, -1);
                }
                if ui.button("대기 작업 아래로").clicked() {
                    self.queue.move_waiting(id, 1);
                }
                if ui.button("목록에서 삭제 (진행 작업 제외)").clicked() {
                    self.queue.remove(id);
                }
            });
            if let Some(job) = self.queue.get(id) {
                if let Some((done, total, unit)) = &job.progress {
                    let ratio = if *total == 0 {
                        0.0
                    } else {
                        *done as f32 / *total as f32
                    };
                    ui.add(egui::ProgressBar::new(ratio).text(format!("{done}/{total} {unit}")));
                }
                if job.status == Status::Running {
                    ui.spinner();
                }
                egui::ScrollArea::vertical()
                    .id_salt("task-log")
                    .max_height(160.0)
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        ui.monospace(&job.log);
                    });
            }
        }
    }

    fn draw(&mut self, context: &egui::Context) {
        self.queue.poll();
        if let Some(id) = self.info_job {
            if let Some(job) = self.queue.get(id) {
                if job.status == Status::Done {
                    match serde_json::from_str::<serde_json::Value>(&job.output) {
                        Ok(work) => {
                            let count = work["episodes"].as_array().map_or(0, Vec::len);
                            self.metadata =
                                format!("{} · {}화", work["title"].as_str().unwrap_or(""), count);
                            self.end = count;
                        }
                        Err(e) => self.message = format!("목차 결과 해석 실패: {e}"),
                    }
                    self.info_job = None;
                } else if job.status == Status::Error {
                    self.message = "목차 확인에 실패했습니다. 작업 로그를 확인해 주세요.".into();
                    self.info_job = None;
                }
            }
        }
        egui::TopBottomPanel::top("navigation").show(context, |ui| {
            ui.horizontal(|ui| {
                for (tab, label) in [
                    (Tab::Novel, "웹소설"),
                    (Tab::Settings, "설정"),
                    (Tab::Learning, "교정 기억"),
                ] {
                    ui.selectable_value(&mut self.tab, tab, label);
                }
            });
        });
        egui::CentralPanel::default().show(context, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("main-content")
                .show(ui, |ui| {
                    match self.tab {
                        Tab::Novel => self.novel(ui),
                        Tab::Settings => self.settings(ui),
                        Tab::Learning => self.learning(ui),
                    }
                    if !self.message.is_empty() {
                        ui.separator();
                        ui.label(
                            egui::RichText::new(&self.message)
                                .color(egui::Color32::from_rgb(230, 150, 50)),
                        );
                    }
                    ui.separator();
                    self.tasks(ui);
                });
        });
        if self.queue.busy() {
            context.request_repaint_after(Duration::from_millis(50));
        }
    }
}

impl eframe::App for Desktop {
    fn update(&mut self, context: &egui::Context, _frame: &mut eframe::Frame) {
        self.draw(context);
    }
}

fn row(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.label(label);
    ui.add(egui::TextEdit::singleline(value).desired_width(f32::INFINITY));
}
fn row_option(ui: &mut egui::Ui, label: &str, value: &mut Option<String>) {
    let mut text = value.clone().unwrap_or_default();
    row(ui, label, &mut text);
    *value = if text.is_empty() { None } else { Some(text) };
}
fn load_font(context: &egui::Context) {
    let mut candidates = Vec::new();
    if let Some(path) = std::env::var_os("SYOUSETU_FONT") {
        candidates.push(PathBuf::from(path));
    }
    if let Some(windows) = std::env::var_os("WINDIR") {
        candidates.push(PathBuf::from(windows).join("Fonts/malgun.ttf"));
    }
    candidates.extend([
        PathBuf::from("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc"),
        PathBuf::from("/usr/share/fonts/opentype/noto/NotoSansCJKkr-Regular.otf"),
        PathBuf::from("/System/Library/Fonts/AppleSDGothicNeo.ttc"),
    ]);
    for path in candidates {
        if let Ok(bytes) = std::fs::read(path) {
            let mut fonts = egui::FontDefinitions::default();
            fonts
                .font_data
                .insert("cjk".into(), egui::FontData::from_owned(bytes).into());
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, "cjk".into());
            fonts
                .families
                .entry(egui::FontFamily::Monospace)
                .or_default()
                .push("cjk".into());
            context.set_fonts(fonts);
            break;
        }
    }
}

fn main() -> eframe::Result {
    eframe::run_native(
        "Syousetu Translate",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([860.0, 850.0])
                .with_min_inner_size([600.0, 500.0]),
            ..Default::default()
        },
        Box::new(|creation| Ok(Box::new(Desktop::new(&creation.egui_ctx)))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn desktop_tabs_render_without_a_display_server() {
        let context = egui::Context::default();
        let mut app = Desktop::new(&context);
        for tab in [Tab::Novel, Tab::Settings, Tab::Learning] {
            app.tab = tab;
            let output = context.run(egui::RawInput::default(), |context| app.draw(context));
            assert!(!output.shapes.is_empty());
        }
        assert!(app.queue.jobs.is_empty());
    }
}
