use serde::{Deserialize, Serialize};
use std::ops::{Add, Div, Mul, Sub};

pub const FORMAT_VERSION: u32 = 1;
pub const EPS: f64 = 1e-7;
pub type Id = u64;

/// All document lengths and plan coordinates are metres, independent of display units.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}
impl Point {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
    pub fn dot(self, rhs: Self) -> f64 {
        self.x * rhs.x + self.y * rhs.y
    }
    pub fn cross(self, rhs: Self) -> f64 {
        self.x * rhs.y - self.y * rhs.x
    }
    pub fn length(self) -> f64 {
        self.dot(self).sqrt()
    }
    pub fn distance(self, rhs: Self) -> f64 {
        (self - rhs).length()
    }
    pub fn rotate(self, angle: f64) -> Self {
        let (s, c) = angle.sin_cos();
        Self::new(c * self.x - s * self.y, s * self.x + c * self.y)
    }
    pub fn finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
}
impl Add for Point {
    type Output = Self;
    fn add(self, p: Self) -> Self {
        Self::new(self.x + p.x, self.y + p.y)
    }
}
impl Sub for Point {
    type Output = Self;
    fn sub(self, p: Self) -> Self {
        Self::new(self.x - p.x, self.y - p.y)
    }
}
impl Mul<f64> for Point {
    type Output = Self;
    fn mul(self, s: f64) -> Self {
        Self::new(self.x * s, self.y * s)
    }
}
impl Div<f64> for Point {
    type Output = Self;
    fn div(self, s: f64) -> Self {
        Self::new(self.x / s, self.y / s)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Units {
    #[default]
    Metres,
    Centimetres,
    Feet,
}
impl Units {
    pub fn factor(self) -> f64 {
        match self {
            Self::Metres => 1.0,
            Self::Centimetres => 100.0,
            Self::Feet => 1.0 / 0.3048,
        }
    }
    pub fn symbol(self) -> &'static str {
        match self {
            Self::Metres => "m",
            Self::Centimetres => "cm",
            Self::Feet => "ft",
        }
    }
    pub fn from_display(self, v: f64) -> f64 {
        v / self.factor()
    }
    pub fn display(self, v: f64) -> f64 {
        v * self.factor()
    }
    pub fn format(self, v: f64) -> String {
        format!("{:.2} {}", self.display(v), self.symbol())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Room {
    /// Counterclockwise inner wall boundary. One simple polygon, without holes.
    pub vertices: Vec<Point>,
    pub thickness: f64,
    pub height: f64,
}
impl Room {
    pub fn rectangle(width: f64, depth: f64) -> Self {
        Self {
            vertices: vec![
                Point::new(0.0, 0.0),
                Point::new(width, 0.0),
                Point::new(width, depth),
                Point::new(0.0, depth),
            ],
            thickness: 0.15,
            height: 2.7,
        }
    }
    pub fn l_shape() -> Self {
        Self {
            vertices: vec![
                Point::new(0.0, 0.0),
                Point::new(6.0, 0.0),
                Point::new(6.0, 3.0),
                Point::new(3.8, 3.0),
                Point::new(3.8, 5.0),
                Point::new(0.0, 5.0),
            ],
            ..Self::rectangle(6.0, 5.0)
        }
    }
    pub fn edge(&self, i: usize) -> (Point, Point) {
        (
            self.vertices[i],
            self.vertices[(i + 1) % self.vertices.len()],
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OpeningKind {
    Door,
    Window,
    Passage,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Opening {
    pub id: Id,
    pub wall: usize,
    pub kind: OpeningKind,
    /// Distance from the inner wall's first vertex, in metres.
    pub offset: f64,
    pub width: f64,
    pub bottom: f64,
    pub height: f64,
    pub hinge_at_end: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FurnitureKind {
    Desk,
    Bed,
    Sofa,
    Cabinet,
}
impl FurnitureKind {
    pub const ALL: [Self; 4] = [Self::Desk, Self::Bed, Self::Sofa, Self::Cabinet];
    pub fn dimensions(self) -> [f64; 3] {
        match self {
            Self::Desk => [1.4, 0.7, 0.75],
            Self::Bed => [1.6, 2.0, 0.55],
            Self::Sofa => [2.0, 0.9, 0.85],
            Self::Cabinet => [1.0, 0.45, 1.8],
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Furniture {
    pub id: Id,
    pub kind: FurnitureKind,
    pub center: Point,
    pub width: f64,
    pub depth: f64,
    pub height: f64,
    pub angle: f64,
}
impl Furniture {
    pub fn new(id: Id, kind: FurnitureKind, center: Point) -> Self {
        let [width, depth, height] = kind.dimensions();
        Self {
            id,
            kind,
            center,
            width,
            depth,
            height,
            angle: 0.0,
        }
    }
    pub fn corners(&self) -> [Point; 4] {
        [
            Point::new(-self.width / 2.0, -self.depth / 2.0),
            Point::new(self.width / 2.0, -self.depth / 2.0),
            Point::new(self.width / 2.0, self.depth / 2.0),
            Point::new(-self.width / 2.0, self.depth / 2.0),
        ]
        .map(|p| self.center + p.rotate(self.angle))
    }
    pub fn local(&self, p: Point) -> Point {
        (p - self.center).rotate(-self.angle)
    }
    pub fn contains(&self, p: Point) -> bool {
        let q = self.local(p);
        q.x.abs() <= self.width / 2.0 && q.y.abs() <= self.depth / 2.0
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub format_version: u32,
    pub title: String,
    pub units: Units,
    pub room: Room,
    pub openings: Vec<Opening>,
    pub furniture: Vec<Furniture>,
    pub next_id: Id,
}
impl Default for Document {
    fn default() -> Self {
        Self {
            format_version: FORMAT_VERSION,
            title: "Room".into(),
            units: Units::Metres,
            room: Room::rectangle(5.0, 4.0),
            openings: vec![],
            furniture: vec![],
            next_id: 1,
        }
    }
}
impl Document {
    pub fn allocate_id(&mut self) -> Id {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
    pub fn sample() -> Self {
        let mut d = Self::default();
        d.furniture.push(Furniture::new(
            1,
            FurnitureKind::Desk,
            Point::new(1.2, 3.45),
        ));
        d.furniture
            .push(Furniture::new(2, FurnitureKind::Sofa, Point::new(3.6, 2.7)));
        d.openings.push(Opening {
            id: 3,
            wall: 0,
            kind: OpeningKind::Door,
            offset: 0.5,
            width: 0.9,
            bottom: 0.0,
            height: 2.1,
            hinge_at_end: false,
        });
        d.openings.push(Opening {
            id: 4,
            wall: 2,
            kind: OpeningKind::Window,
            offset: 1.2,
            width: 1.5,
            bottom: 0.9,
            height: 1.2,
            hinge_at_end: false,
        });
        d.next_id = 5;
        d
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ValidationError {
    Format,
    Room,
    Wall,
    Opening,
    Furniture,
    DuplicateId,
    Limit,
}
impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for ValidationError {}
