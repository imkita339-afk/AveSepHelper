use eframe::egui::{self, Color32, Stroke, Vec2};
use std::time::{Duration, Instant};

pub struct Effects {
    pub enabled: bool,
    page: Instant,
    success: Option<Instant>,
}
impl Default for Effects {
    fn default() -> Self {
        Self {
            enabled: true,
            page: Instant::now(),
            success: None,
        }
    }
}
impl Effects {
    pub fn transition(&mut self) {
        self.page = Instant::now();
    }
    pub fn celebrate(&mut self) {
        self.success = Some(Instant::now());
    }
    pub fn opacity(&self, ctx: &egui::Context) -> f32 {
        if !self.enabled {
            return 1.0;
        }
        let t = (self.page.elapsed().as_secs_f32() / 0.23).min(1.0);
        if t < 1.0 {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
        1.0 - (1.0 - t).powi(3)
    }
    pub fn busy_line(&self, ui: &mut egui::Ui) {
        let (r, _) = ui.allocate_exact_size(
            Vec2::new(ui.available_width().min(180.0), 4.0),
            egui::Sense::hover(),
        );
        ui.painter()
            .rect_filled(r, 2, Color32::from_rgb(222, 239, 232));
        let t = if self.enabled {
            ui.input(|i| i.time) as f32 * 0.8 % 1.0
        } else {
            0.5
        };
        let left = r.left() + (r.width() * 1.3) * t - r.width() * 0.3;
        let band = egui::Rect::from_min_size(
            egui::pos2(left, r.top()),
            Vec2::new(r.width() * 0.3, r.height()),
        )
        .intersect(r);
        ui.painter()
            .rect_filled(band, 2, Color32::from_rgb(38, 164, 134));
        if self.enabled {
            ui.ctx().request_repaint_after(Duration::from_millis(25));
        }
    }
    pub fn success(&self, ui: &mut egui::Ui) {
        let (r, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 58.0), egui::Sense::hover());
        let c = r.min + Vec2::new(27.0, 28.0);
        let age = self.success.map_or(3.0, |v| v.elapsed().as_secs_f32());
        let t = if self.enabled {
            (age / 0.4).min(1.0)
        } else {
            1.0
        };
        let ease = 1.0 - (1.0 - t).powi(3);
        ui.painter().circle_filled(
            c,
            24.0 * (0.8 + ease * 0.2),
            Color32::from_rgb(227, 245, 236),
        );
        let points = [
            c + Vec2::new(-9.0, 0.0),
            c + Vec2::new(-2.0, 7.0),
            c + Vec2::new(11.0, -8.0),
        ];
        ui.painter().line_segment(
            [points[0], points[0].lerp(points[1], (ease * 2.0).min(1.0))],
            Stroke::new(3.0_f32, Color32::from_rgb(9, 123, 106)),
        );
        if ease > 0.5 {
            ui.painter().line_segment(
                [points[1], points[1].lerp(points[2], (ease - 0.5) * 2.0)],
                Stroke::new(3.0_f32, Color32::from_rgb(9, 123, 106)),
            );
        }
        if self.enabled && age < 1.25 {
            let t = age / 1.25;
            for i in 0..24 {
                let seed = i as f32;
                let x = r.left() + 65.0 + ((seed * 73.7) % (r.width() - 75.0).max(1.0));
                let y = r.top() + 8.0 + (seed * 17.1 % 20.0)
                    - (t * std::f32::consts::PI).sin() * 13.0
                    + t * 20.0;
                let col = if i % 3 == 0 {
                    Color32::from_rgb(220, 171, 91)
                } else {
                    Color32::from_rgb(46, 170, 139)
                };
                ui.painter().circle_filled(
                    egui::pos2(x, y),
                    1.5 + seed % 2.0,
                    col.gamma_multiply((1.0 - t) * 0.8),
                );
            }
            ui.ctx().request_repaint_after(Duration::from_millis(16));
        }
    }
}

pub fn button_glow(response: &egui::Response) {
    let enabled = response
        .ctx
        .data(|d| d.get_temp::<bool>(egui::Id::new("motion-enabled")))
        .unwrap_or(true);
    let hover = response.ctx.animate_bool_with_time(
        response.id.with("soft-glow"),
        response.hovered() && response.enabled(),
        if enabled { 0.15 } else { 0.0 },
    );
    if hover > 0.001 {
        response.ctx.layer_painter(response.layer_id).rect_stroke(
            response.rect.expand(2.0),
            11,
            Stroke::new(
                2.0_f32,
                Color32::from_rgba_unmultiplied(35, 165, 130, (45.0 * hover) as u8),
            ),
            egui::StrokeKind::Outside,
        );
    }
}
