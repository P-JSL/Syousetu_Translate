use super::Desktop;
use eframe::egui;
use syousetu_translate::jobs::Status;

impl Desktop {
    pub(super) fn tasks(&mut self, ui: &mut egui::Ui) {
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
}
