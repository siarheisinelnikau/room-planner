use egui::{
    Color32, TextureId,
    epaint::{ClippedPrimitive, Primitive, Vertex, textures::TexturesDelta},
};
use std::collections::HashMap;

struct Texture {
    size: [usize; 2],
    pixels: Vec<Color32>,
}
#[derive(Default)]
pub struct SoftwarePainter {
    textures: HashMap<TextureId, Texture>,
}
impl SoftwarePainter {
    pub fn update(&mut self, delta: &TexturesDelta) -> Result<(), String> {
        for (id, change) in &delta.set {
            let egui::ImageData::Color(image) = &change.image;
            if image.size.contains(&0) || image.size[0] > 8192 || image.size[1] > 8192 {
                return Err("Texture exceeds software renderer limit".into());
            }
            if let Some([x, y]) = change.pos {
                let tex = self
                    .textures
                    .get_mut(id)
                    .ok_or("Texture patch has no base")?;
                if x + image.size[0] > tex.size[0] || y + image.size[1] > tex.size[1] {
                    return Err("Invalid texture patch".into());
                }
                for row in 0..image.size[1] {
                    let start = (y + row) * tex.size[0] + x;
                    tex.pixels[start..start + image.size[0]].copy_from_slice(
                        &image.pixels[row * image.size[0]..(row + 1) * image.size[0]],
                    );
                }
            } else {
                self.textures.insert(
                    *id,
                    Texture {
                        size: image.size,
                        pixels: image.pixels.clone(),
                    },
                );
            }
        }
        Ok(())
    }
    pub fn free(&mut self, delta: &TexturesDelta) {
        for id in &delta.free {
            self.textures.remove(id);
        }
    }
    pub fn paint(
        &self,
        target: &mut [u32],
        width: usize,
        height: usize,
        ppp: f32,
        primitives: &[ClippedPrimitive],
        background: Color32,
    ) {
        target.fill(pack(background.r(), background.g(), background.b()));
        for primitive in primitives {
            let Primitive::Mesh(mesh) = &primitive.primitive else {
                continue;
            };
            let Some(texture) = self.textures.get(&mesh.texture_id) else {
                continue;
            };
            let clip = [
                (primitive.clip_rect.min.x * ppp).floor().max(0.0) as usize,
                (primitive.clip_rect.min.y * ppp).floor().max(0.0) as usize,
                (primitive.clip_rect.max.x * ppp)
                    .ceil()
                    .clamp(0.0, width as f32) as usize,
                (primitive.clip_rect.max.y * ppp)
                    .ceil()
                    .clamp(0.0, height as f32) as usize,
            ];
            for triangle in mesh.indices.chunks_exact(3) {
                let mut v = [
                    mesh.vertices[triangle[0] as usize],
                    mesh.vertices[triangle[1] as usize],
                    mesh.vertices[triangle[2] as usize],
                ];
                for p in &mut v {
                    p.pos.x *= ppp;
                    p.pos.y *= ppp;
                }
                self.triangle(target, width, clip, v, texture);
            }
        }
    }
    fn triangle(
        &self,
        target: &mut [u32],
        width: usize,
        clip: [usize; 4],
        mut v: [Vertex; 3],
        texture: &Texture,
    ) {
        let edge = |a: egui::Pos2, b: egui::Pos2, p: egui::Pos2| {
            (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x)
        };
        let mut area = edge(v[0].pos, v[1].pos, v[2].pos);
        if !area.is_finite() || area.abs() < 1e-6 {
            return;
        }
        if area < 0.0 {
            v.swap(1, 2);
            area = -area;
        }
        let minx = (v
            .iter()
            .map(|p| p.pos.x)
            .fold(f32::INFINITY, f32::min)
            .floor()
            .max(0.0) as usize)
            .max(clip[0]);
        let miny = (v
            .iter()
            .map(|p| p.pos.y)
            .fold(f32::INFINITY, f32::min)
            .floor()
            .max(0.0) as usize)
            .max(clip[1]);
        let maxx = (v
            .iter()
            .map(|p| p.pos.x)
            .fold(f32::NEG_INFINITY, f32::max)
            .ceil()
            .max(0.0) as usize)
            .min(clip[2]);
        let maxy = (v
            .iter()
            .map(|p| p.pos.y)
            .fold(f32::NEG_INFINITY, f32::max)
            .ceil()
            .max(0.0) as usize)
            .min(clip[3]);
        let inclusive = |a: egui::Pos2, b: egui::Pos2| b.y < a.y || (b.y == a.y && b.x > a.x);
        let top = [
            inclusive(v[1].pos, v[2].pos),
            inclusive(v[2].pos, v[0].pos),
            inclusive(v[0].pos, v[1].pos),
        ];
        let solid = v.iter().all(|p| p.uv == egui::epaint::WHITE_UV);
        for y in miny..maxy {
            for x in minx..maxx {
                let p = egui::pos2(x as f32 + 0.5, y as f32 + 0.5);
                let weights = [
                    edge(v[1].pos, v[2].pos, p),
                    edge(v[2].pos, v[0].pos, p),
                    edge(v[0].pos, v[1].pos, p),
                ];
                if weights
                    .iter()
                    .enumerate()
                    .any(|(i, w)| *w < 0.0 || (*w == 0.0 && !top[i]))
                {
                    continue;
                }
                let w = weights.map(|a| a / area);
                let mut rgba = [0.0; 4];
                for i in 0..3 {
                    for (j, c) in rgba.iter_mut().enumerate() {
                        *c += v[i].color[j] as f32 * w[i];
                    }
                }
                if !solid {
                    let uv = v[0].uv.to_vec2() * w[0]
                        + v[1].uv.to_vec2() * w[1]
                        + v[2].uv.to_vec2() * w[2];
                    let tx = (uv.x * texture.size[0] as f32 - 0.5)
                        .clamp(0.0, texture.size[0].saturating_sub(1) as f32);
                    let ty = (uv.y * texture.size[1] as f32 - 0.5)
                        .clamp(0.0, texture.size[1].saturating_sub(1) as f32);
                    let x0 = tx as usize;
                    let y0 = ty as usize;
                    let x1 = (x0 + 1).min(texture.size[0] - 1);
                    let y1 = (y0 + 1).min(texture.size[1] - 1);
                    let fx = tx - x0 as f32;
                    let fy = ty - y0 as f32;
                    for (j, c) in rgba.iter_mut().enumerate() {
                        let a = texture.pixels[y0 * texture.size[0] + x0][j] as f32 * (1.0 - fx)
                            + texture.pixels[y0 * texture.size[0] + x1][j] as f32 * fx;
                        let b = texture.pixels[y1 * texture.size[0] + x0][j] as f32 * (1.0 - fx)
                            + texture.pixels[y1 * texture.size[0] + x1][j] as f32 * fx;
                        *c *= (a * (1.0 - fy) + b * fy) / 255.0;
                    }
                }
                let i = y * width + x;
                let dst = target[i];
                let inverse = 1.0 - rgba[3] / 255.0;
                target[i] = pack(
                    (rgba[0] + ((dst >> 16) & 255) as f32 * inverse).clamp(0.0, 255.0) as u8,
                    (rgba[1] + ((dst >> 8) & 255) as f32 * inverse).clamp(0.0, 255.0) as u8,
                    (rgba[2] + (dst & 255) as f32 * inverse).clamp(0.0, 255.0) as u8,
                );
            }
        }
    }
}
fn pack(r: u8, g: u8, b: u8) -> u32 {
    (r as u32) << 16 | (g as u32) << 8 | b as u32
}
