use super::font::load_font;
use super::{Desktop, Tab};
use eframe::egui;
use std::{ffi::OsString, path::PathBuf, time::Duration};
use syousetu_translate::{
    config::Config,
    jobs::{sibling_cli, Queue, Status},
    learning::Memory,
};

impl Desktop {
    pub(super) fn new(context: &egui::Context) -> Self {
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

    pub(super) fn enqueue(
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

    pub(super) fn draw(&mut self, context: &egui::Context) {
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
