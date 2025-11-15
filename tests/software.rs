use egui::{
    Color32, Pos2, Rect, TextureId,
    epaint::{ClippedPrimitive, Mesh, Primitive, Vertex, textures::TexturesDelta},
};
use room_planner::software::SoftwarePainter;
fn painter() -> SoftwarePainter {
    let mut painter = SoftwarePainter::default();
    let mut delta = TexturesDelta::default();
    delta.set.push((
        TextureId::Managed(0),
        egui::epaint::ImageDelta::full(
            egui::ColorImage::filled([1, 1], Color32::WHITE),
            egui::TextureOptions::LINEAR,
        ),
    ));
    painter.update(&delta).unwrap();
    painter
}
fn square(color: Color32) -> ClippedPrimitive {
    let mut mesh = Mesh::default();
    for p in [
        Pos2::new(0.0, 0.0),
        Pos2::new(4.0, 0.0),
        Pos2::new(4.0, 4.0),
        Pos2::new(0.0, 4.0),
    ] {
        mesh.vertices.push(Vertex {
            pos: p,
            uv: egui::epaint::WHITE_UV,
            color,
        });
    }
    mesh.indices = vec![0, 1, 2, 0, 2, 3];
    ClippedPrimitive {
        clip_rect: Rect::from_min_max(Pos2::ZERO, Pos2::new(4.0, 4.0)),
        primitive: Primitive::Mesh(mesh),
    }
}
#[test]
fn shared_triangle_edge_blends_exactly_once() {
    let painter = painter();
    let mut pixels = vec![0; 16];
    painter.paint(
        &mut pixels,
        4,
        4,
        1.0,
        &[square(Color32::from_rgba_premultiplied(128, 0, 0, 128))],
        Color32::BLACK,
    );
    assert!(pixels.iter().all(|&p| p == 0x00800000));
}
#[test]
fn clipping_and_dpi_are_applied_in_physical_pixels() {
    let painter = painter();
    let mut mesh = square(Color32::GREEN);
    mesh.clip_rect = Rect::from_min_max(Pos2::new(1.0, 1.0), Pos2::new(3.0, 3.0));
    let mut pixels = vec![0; 64];
    painter.paint(&mut pixels, 8, 8, 2.0, &[mesh], Color32::BLACK);
    for y in 0..8 {
        for x in 0..8 {
            assert_eq!(
                pixels[y * 8 + x] != 0,
                (2..6).contains(&x) && (2..6).contains(&y)
            );
        }
    }
}
#[test]
fn texture_patch_and_sampling_use_actual_pixels() {
    let mut painter = painter();
    let mut delta = TexturesDelta::default();
    delta.set.push((
        TextureId::Managed(1),
        egui::epaint::ImageDelta::full(
            egui::ColorImage::filled([2, 2], Color32::BLUE),
            egui::TextureOptions::LINEAR,
        ),
    ));
    painter.update(&delta).unwrap();
    delta.set.clear();
    delta.set.push((
        TextureId::Managed(1),
        egui::epaint::ImageDelta::partial(
            [0, 0],
            egui::ColorImage::filled([1, 1], Color32::RED),
            egui::TextureOptions::LINEAR,
        ),
    ));
    painter.update(&delta).unwrap();
    let mut primitive = square(Color32::WHITE);
    if let Primitive::Mesh(mesh) = &mut primitive.primitive {
        mesh.texture_id = TextureId::Managed(1);
        for v in &mut mesh.vertices {
            v.uv = Pos2::new(0.25, 0.25);
        }
    }
    let mut pixels = vec![0; 16];
    painter.paint(&mut pixels, 4, 4, 1.0, &[primitive], Color32::BLACK);
    assert!(pixels.iter().all(|&p| p == 0x00ff0000));
}
