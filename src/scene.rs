use crate::{geometry, model::*};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}
impl Vec3 {
    pub fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }
    fn sub(self, b: Self) -> Self {
        Self::new(self.x - b.x, self.y - b.y, self.z - b.z)
    }
    fn cross(self, b: Self) -> Self {
        Self::new(
            self.y * b.z - self.z * b.y,
            self.z * b.x - self.x * b.z,
            self.x * b.y - self.y * b.x,
        )
    }
    fn dot(self, b: Self) -> f64 {
        self.x * b.x + self.y * b.y + self.z * b.z
    }
}
#[derive(Clone, Debug)]
pub struct Triangle {
    pub vertices: [Vec3; 3],
    pub color: [u8; 3],
    pub object: Option<Id>,
    pub wall: Option<usize>,
}
#[derive(Clone, Debug, Default)]
pub struct Scene {
    pub triangles: Vec<Triangle>,
}
impl Scene {
    fn triangle(
        &mut self,
        vertices: [Vec3; 3],
        color: [u8; 3],
        object: Option<Id>,
        wall: Option<usize>,
    ) {
        self.triangles.push(Triangle {
            vertices,
            color,
            object,
            wall,
        });
    }
    fn prism(
        &mut self,
        p: &[Point; 4],
        bottom: f64,
        top: f64,
        color: [u8; 3],
        object: Option<Id>,
        wall: Option<usize>,
    ) {
        let lower = p.map(|q| Vec3::new(q.x, bottom, q.y));
        let upper = p.map(|q| Vec3::new(q.x, top, q.y));
        for i in 0..4 {
            let j = (i + 1) % 4;
            self.triangle([lower[i], lower[j], upper[j]], color, object, wall);
            self.triangle([lower[i], upper[j], upper[i]], color, object, wall);
        }
        self.triangle([upper[0], upper[1], upper[2]], color, object, wall);
        self.triangle([upper[0], upper[2], upper[3]], color, object, wall);
        self.triangle([lower[2], lower[1], lower[0]], color, object, wall);
        self.triangle([lower[3], lower[2], lower[0]], color, object, wall);
    }
    // Keep the plan dimensions and vertical bounds explicit at each parametric part.
    #[allow(clippy::too_many_arguments)]
    fn part(
        &mut self,
        f: &Furniture,
        offset: Point,
        w: f64,
        d: f64,
        bottom: f64,
        top: f64,
        color: [u8; 3],
    ) {
        let center = f.center + offset.rotate(f.angle);
        let p = [
            Point::new(-w / 2.0, -d / 2.0),
            Point::new(w / 2.0, -d / 2.0),
            Point::new(w / 2.0, d / 2.0),
            Point::new(-w / 2.0, d / 2.0),
        ]
        .map(|q| center + q.rotate(f.angle));
        self.prism(&p, bottom, top, color, Some(f.id), None);
    }
    pub fn from_document(d: &Document) -> Self {
        let mut scene = Self::default();
        for t in geometry::triangulate(&d.room.vertices) {
            scene.triangle(
                t.map(|i| Vec3::new(d.room.vertices[i].x, 0.0, d.room.vertices[i].y)),
                [204, 187, 161],
                None,
                None,
            );
        }
        for wall in 0..d.room.vertices.len() {
            let (a, b) = d.room.edge(wall);
            let len = a.distance(b);
            let mut openings: Vec<_> = d.openings.iter().filter(|o| o.wall == wall).collect();
            openings.sort_by(|a, b| a.offset.total_cmp(&b.offset));
            let mut start = 0.0;
            for o in openings {
                if o.offset > start {
                    scene.prism(
                        &geometry::wall_quad(&d.room, wall, start, o.offset),
                        0.0,
                        d.room.height,
                        [228, 226, 214],
                        None,
                        Some(wall),
                    );
                }
                let p = geometry::wall_quad(&d.room, wall, o.offset, o.offset + o.width);
                if o.bottom > 0.0 {
                    scene.prism(&p, 0.0, o.bottom, [228, 226, 214], None, Some(wall));
                }
                if o.bottom + o.height < d.room.height {
                    scene.prism(
                        &p,
                        o.bottom + o.height,
                        d.room.height,
                        [228, 226, 214],
                        None,
                        Some(wall),
                    );
                }
                if o.kind == OpeningKind::Window {
                    let u = (b - a) / len;
                    let n = Point::new(u.y, -u.x) * d.room.thickness / 2.0;
                    let q = a + u * o.offset + n;
                    let r = q + u * o.width;
                    let inset = Point::new(u.y, -u.x) * 0.012;
                    scene.prism(
                        &[q, r, r + inset, q + inset],
                        o.bottom,
                        o.bottom + o.height,
                        [134, 186, 198],
                        Some(o.id),
                        Some(wall),
                    );
                }
                if o.kind == OpeningKind::Door {
                    let u = (b - a) / len;
                    let inward = Point::new(-u.y, u.x);
                    let hinge = a + u * (o.offset + if o.hinge_at_end { o.width } else { 0.0 });
                    let tip = hinge + inward * o.width;
                    let across = u * 0.035;
                    scene.prism(
                        &[hinge, tip, tip + across, hinge + across],
                        0.0,
                        o.height,
                        [169, 121, 80],
                        Some(o.id),
                        Some(wall),
                    );
                }
                start = o.offset + o.width;
            }
            if start < len {
                scene.prism(
                    &geometry::wall_quad(&d.room, wall, start, len),
                    0.0,
                    d.room.height,
                    [228, 226, 214],
                    None,
                    Some(wall),
                );
            }
        }
        for f in &d.furniture {
            match f.kind {
                FurnitureKind::Desk => {
                    scene.part(
                        f,
                        Point::default(),
                        f.width,
                        f.depth,
                        f.height * 0.91,
                        f.height,
                        [175, 126, 84],
                    );
                    for x in [-0.42, 0.42] {
                        for y in [-0.36, 0.36] {
                            scene.part(
                                f,
                                Point::new(x * f.width, y * f.depth),
                                f.width * 0.06,
                                f.depth * 0.08,
                                0.0,
                                f.height * 0.91,
                                [59, 79, 78],
                            );
                        }
                    }
                }
                FurnitureKind::Bed => {
                    scene.part(
                        f,
                        Point::default(),
                        f.width,
                        f.depth,
                        0.05,
                        f.height * 0.4,
                        [140, 101, 70],
                    );
                    scene.part(
                        f,
                        Point::default(),
                        f.width * 0.96,
                        f.depth * 0.98,
                        f.height * 0.4,
                        f.height * 0.87,
                        [201, 211, 201],
                    );
                    scene.part(
                        f,
                        Point::new(0.0, -f.depth * 0.31),
                        f.width * 0.8,
                        f.depth * 0.22,
                        f.height * 0.87,
                        f.height,
                        [242, 236, 218],
                    );
                }
                FurnitureKind::Sofa => {
                    scene.part(
                        f,
                        Point::default(),
                        f.width,
                        f.depth,
                        0.08,
                        f.height * 0.55,
                        [80, 132, 122],
                    );
                    scene.part(
                        f,
                        Point::new(0.0, f.depth * 0.4),
                        f.width,
                        f.depth * 0.2,
                        f.height * 0.55,
                        f.height,
                        [64, 114, 104],
                    );
                    for x in [-0.44, 0.44] {
                        scene.part(
                            f,
                            Point::new(x * f.width, 0.0),
                            f.width * 0.12,
                            f.depth,
                            0.08,
                            f.height * 0.82,
                            [64, 114, 104],
                        );
                    }
                }
                FurnitureKind::Cabinet => {
                    scene.part(
                        f,
                        Point::default(),
                        f.width,
                        f.depth,
                        0.0,
                        f.height,
                        [173, 139, 103],
                    );
                    scene.part(
                        f,
                        Point::new(0.0, -f.depth * 0.49),
                        0.015,
                        f.depth * 0.015,
                        f.height * 0.05,
                        f.height * 0.95,
                        [104, 86, 66],
                    );
                }
            }
        }
        scene
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub yaw: f64,
    pub elevation: f64,
    pub scale: f64,
    pub target: Point,
    pub cutaway: bool,
}
impl Camera {
    pub fn fit(d: &Document) -> Self {
        let (min, max) = bounds(&d.room.vertices);
        Self {
            yaw: -0.8,
            elevation: 0.65,
            scale: 1.0,
            target: (min + max) / 2.0,
            cutaway: true,
        }
    }
    pub fn project(&self, p: Vec3, width: usize, height: usize, pixels_per_metre: f64) -> [f64; 3] {
        let p = Vec3::new(p.x - self.target.x, p.y - 0.7, p.z - self.target.y);
        let (s, c) = self.yaw.sin_cos();
        let (se, ce) = self.elevation.sin_cos();
        let right = p.x * c - p.z * s;
        let forward = p.x * s + p.z * c;
        [
            width as f64 / 2.0 + right * pixels_per_metre * self.scale,
            height as f64 / 2.0 - (p.y * ce - forward * se) * pixels_per_metre * self.scale,
            p.y * se + forward * ce,
        ]
    }
}
pub fn bounds(p: &[Point]) -> (Point, Point) {
    p.iter().fold(
        (
            Point::new(f64::INFINITY, f64::INFINITY),
            Point::new(f64::NEG_INFINITY, f64::NEG_INFINITY),
        ),
        |(min, max), p| {
            (
                Point::new(min.x.min(p.x), min.y.min(p.y)),
                Point::new(max.x.max(p.x), max.y.max(p.y)),
            )
        },
    )
}

/// Depth-buffered orthographic software view. It uses the same wall cutouts and object transforms as the plan.
pub fn render(
    scene: &Scene,
    d: &Document,
    camera: Camera,
    width: usize,
    height: usize,
    dark: bool,
    selected: Option<Id>,
) -> egui::ColorImage {
    let background = if dark {
        egui::Color32::from_rgb(28, 40, 43)
    } else {
        egui::Color32::from_rgb(235, 240, 235)
    };
    let mut image = egui::ColorImage::filled([width, height], background);
    let mut depths = vec![f64::NEG_INFINITY; width * height];
    let (min, max) = bounds(&d.room.vertices);
    let ppm = (width as f64 / (max - min).length().max(1.0))
        .min(height as f64 / ((max - min).length() * 0.7 + d.room.height))
        * 0.86;
    for triangle in &scene.triangles {
        if camera.cutaway
            && let Some(wall) = triangle.wall
        {
            let (a, b) = d.room.edge(wall);
            let edge = b - a;
            let outward = Point::new(edge.y, -edge.x);
            let toward_camera = Point::new(camera.yaw.sin(), camera.yaw.cos());
            if outward.dot(toward_camera) > 0.0 {
                continue;
            }
        }
        let projected = triangle
            .vertices
            .map(|v| camera.project(v, width, height, ppm));
        let edge = |a: [f64; 3], b: [f64; 3], x: f64, y: f64| {
            (b[0] - a[0]) * (y - a[1]) - (b[1] - a[1]) * (x - a[0])
        };
        let area = edge(projected[0], projected[1], projected[2][0], projected[2][1]);
        if area.abs() < 1e-8 {
            continue;
        }
        let minx = projected
            .iter()
            .map(|p| p[0])
            .fold(f64::INFINITY, f64::min)
            .floor()
            .max(0.0) as usize;
        let maxx = projected
            .iter()
            .map(|p| p[0])
            .fold(f64::NEG_INFINITY, f64::max)
            .ceil()
            .clamp(0.0, width as f64) as usize;
        let miny = projected
            .iter()
            .map(|p| p[1])
            .fold(f64::INFINITY, f64::min)
            .floor()
            .max(0.0) as usize;
        let maxy = projected
            .iter()
            .map(|p| p[1])
            .fold(f64::NEG_INFINITY, f64::max)
            .ceil()
            .clamp(0.0, height as f64) as usize;
        let normal = triangle.vertices[1]
            .sub(triangle.vertices[0])
            .cross(triangle.vertices[2].sub(triangle.vertices[0]));
        let light =
            (normal.dot(Vec3::new(-0.4, 0.85, -0.3)) / normal.dot(normal).sqrt().max(EPS)).abs();
        let color = if triangle.object.is_some() && triangle.object == selected {
            [202, 158, 72]
        } else {
            triangle.color
        };
        let shade = 0.62 + 0.38 * light;
        let color = egui::Color32::from_rgb(
            (color[0] as f64 * shade) as u8,
            (color[1] as f64 * shade) as u8,
            (color[2] as f64 * shade) as u8,
        );
        for y in miny..maxy {
            for x in minx..maxx {
                let xf = x as f64 + 0.5;
                let yf = y as f64 + 0.5;
                let a = edge(projected[1], projected[2], xf, yf) / area;
                let b = edge(projected[2], projected[0], xf, yf) / area;
                let c = 1.0 - a - b;
                if a >= -EPS && b >= -EPS && c >= -EPS {
                    let z = a * projected[0][2] + b * projected[1][2] + c * projected[2][2];
                    let i = y * width + x;
                    if z > depths[i] {
                        depths[i] = z;
                        image.pixels[i] = color;
                    }
                }
            }
        }
    }
    image
}
