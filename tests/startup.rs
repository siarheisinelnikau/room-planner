use room_planner::{
    i18n::Language,
    settings::{Settings, Theme},
    software::SoftwarePainter,
    ui::Workspace,
    updates::Transport,
};
use std::sync::Arc;

struct NoNetwork;
impl Transport for NoNetwork {
    fn bytes(
        &self,
        _url: &str,
        _limit: u64,
        _progress: &mut dyn FnMut(u64),
    ) -> Result<Vec<u8>, String> {
        panic!("Startup layout must not use the network");
    }
}

#[test]
fn first_frame_reaches_the_software_rasterizer() {
    let settings = Settings {
        startup_updates: false,
        ..Default::default()
    };
    let mut workspace = Workspace::with_settings(settings, Arc::new(NoNetwork), Arc::new(|| {}));
    let context = egui::Context::default();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1060.0, 680.0),
        )),
        ..Default::default()
    };
    let output = context.run(input, |context| workspace.show(context));
    let primitives = context.tessellate(output.shapes, output.pixels_per_point);
    let mut painter = SoftwarePainter::default();
    painter.update(&output.textures_delta).unwrap();
    let mut pixels = vec![0; 1060 * 680];
    painter.paint(
        &mut pixels,
        1060,
        680,
        output.pixels_per_point,
        &primitives,
        egui::Color32::BLACK,
    );
    assert!(
        pixels
            .iter()
            .copied()
            .collect::<std::collections::HashSet<_>>()
            .len()
            > 20
    );
}

#[test]
fn first_workspace_frame_initializes_without_a_native_window() {
    for language in Language::ALL {
        for theme in [Theme::Light, Theme::Dark] {
            let settings = Settings {
                language,
                theme,
                startup_updates: false,
                ..Default::default()
            };
            let mut workspace =
                Workspace::with_settings(settings, Arc::new(NoNetwork), Arc::new(|| {}));
            let context = egui::Context::default();
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280.0, 820.0),
                )),
                ..Default::default()
            };
            let output = context.run(input, |context| workspace.show(context));
            assert!(!output.shapes.is_empty());
        }
    }
}

#[test]
fn unchanged_workspace_stops_requesting_frames_after_layout_settles() {
    let settings = Settings {
        startup_updates: false,
        reduced_motion: true,
        ..Default::default()
    };
    let mut workspace = Workspace::with_settings(settings, Arc::new(NoNetwork), Arc::new(|| {}));
    let context = egui::Context::default();
    let mut delay = std::time::Duration::ZERO;
    for frame in 0..12 {
        let input = egui::RawInput {
            time: Some(frame as f64 * 0.2),
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 820.0),
            )),
            ..Default::default()
        };
        let output = context.run(input, |context| workspace.show(context));
        delay = output.viewport_output[&egui::ViewportId::ROOT].repaint_delay;
    }
    assert_eq!(delay, std::time::Duration::MAX);
}
