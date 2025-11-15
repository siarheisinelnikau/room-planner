use crate::model::*;
use std::collections::HashSet;

pub fn signed_area(p: &[Point]) -> f64 {
    (0..p.len())
        .map(|i| p[i].cross(p[(i + 1) % p.len()]))
        .sum::<f64>()
        / 2.0
}
pub fn distance_segment(p: Point, a: Point, b: Point) -> (f64, f64) {
    let v = b - a;
    let t = ((p - a).dot(v) / v.dot(v).max(EPS)).clamp(0.0, 1.0);
    (p.distance(a + v * t), t)
}
pub fn segments_intersect(a: Point, b: Point, c: Point, d: Point) -> bool {
    let orient = |a: Point, b: Point, c: Point| (b - a).cross(c - a);
    let (ab_c, ab_d, cd_a, cd_b) = (
        orient(a, b, c),
        orient(a, b, d),
        orient(c, d, a),
        orient(c, d, b),
    );
    if ab_c * ab_d < -EPS && cd_a * cd_b < -EPS {
        return true;
    }
    [
        (ab_c, c, a, b),
        (ab_d, d, a, b),
        (cd_a, a, c, d),
        (cd_b, b, c, d),
    ]
    .into_iter()
    .any(|(v, p, a, b)| v.abs() < EPS && distance_segment(p, a, b).0 < EPS)
}
pub fn contains(polygon: &[Point], p: Point) -> bool {
    let mut inside = false;
    for i in 0..polygon.len() {
        let (a, b) = (polygon[i], polygon[(i + 1) % polygon.len()]);
        if distance_segment(p, a, b).0 < EPS {
            return true;
        }
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
    }
    inside
}
pub fn validate_polygon(p: &[Point]) -> bool {
    if !(3..=128).contains(&p.len())
        || p.iter()
            .any(|v| !v.finite() || v.x.abs() > 1000.0 || v.y.abs() > 1000.0)
        || signed_area(p) < 0.1
    {
        return false;
    }
    for i in 0..p.len() {
        let (a, b) = (p[i], p[(i + 1) % p.len()]);
        if a.distance(b) < 0.1 {
            return false;
        }
        let prev = p[(i + p.len() - 1) % p.len()];
        let incoming = (a - prev) / (a - prev).length();
        let outgoing = (b - a) / (b - a).length();
        // Exclude reversals and near-parallel corners that create unbounded wall miters.
        if incoming.dot(outgoing) < -0.98 {
            return false;
        }
        for j in i + 1..p.len() {
            if j == i + 1 || (i == 0 && j == p.len() - 1) {
                continue;
            }
            if segments_intersect(a, b, p[j], p[(j + 1) % p.len()]) {
                return false;
            }
        }
    }
    true
}
fn bounded(v: f64, min: f64, max: f64) -> bool {
    v.is_finite() && (min..=max).contains(&v)
}
pub fn validate(d: &Document) -> Result<(), ValidationError> {
    use ValidationError::*;
    if d.format_version != FORMAT_VERSION {
        return Err(Format);
    }
    if d.title.len() > 256
        || d.title.chars().any(char::is_control)
        || d.furniture.len() > 500
        || d.openings.len() > 256
        || d.next_id == u64::MAX
        || d.next_id == 0
    {
        return Err(Limit);
    }
    if !validate_polygon(&d.room.vertices) {
        return Err(Room);
    }
    if !bounded(d.room.thickness, 0.05, 1.0) || !bounded(d.room.height, 1.0, 10.0) {
        return Err(Wall);
    }
    let outer = outer_corners(&d.room);
    for i in 0..outer.len() {
        let (a, b) = d.room.edge(i);
        if !outer[i].finite() || (outer[(i + 1) % outer.len()] - outer[i]).dot(b - a) <= EPS {
            return Err(Wall);
        }
        for j in i + 1..outer.len() {
            if j != i + 1
                && !(i == 0 && j == outer.len() - 1)
                && segments_intersect(
                    outer[i],
                    outer[(i + 1) % outer.len()],
                    outer[j],
                    outer[(j + 1) % outer.len()],
                )
            {
                return Err(Wall);
            }
        }
        for j in 0..outer.len() {
            let (p, q) = d.room.edge(j);
            if segments_intersect(outer[i], outer[(i + 1) % outer.len()], p, q) {
                return Err(Wall);
            }
        }
    }
    let mut ids = HashSet::new();
    for o in &d.openings {
        if !ids.insert(o.id) || o.id == 0 || o.id >= d.next_id {
            return Err(DuplicateId);
        }
        if o.wall >= d.room.vertices.len() {
            return Err(Opening);
        }
        let (a, b) = d.room.edge(o.wall);
        let len = a.distance(b);
        let u = (b - a) / len;
        let start_clearance = (outer[o.wall] - a).dot(u).max(0.0) + 0.05;
        let end_clearance = (b - outer[(o.wall + 1) % outer.len()]).dot(u).max(0.0) + 0.05;
        if !bounded(o.offset, 0.05, len)
            || !bounded(o.width, 0.1, len)
            || o.offset + o.width > len - 0.05 + EPS
            || o.offset < start_clearance - EPS
            || o.offset + o.width > len - end_clearance + EPS
            || !bounded(o.bottom, 0.0, d.room.height)
            || !bounded(o.height, 0.1, d.room.height)
            || o.bottom + o.height > d.room.height + EPS
        {
            return Err(Opening);
        }
        if o.kind != OpeningKind::Window && o.bottom != 0.0 {
            return Err(Opening);
        }
        for other in d
            .openings
            .iter()
            .filter(|v| v.id != o.id && v.wall == o.wall)
        {
            if o.offset < other.offset + other.width + 0.05 - EPS
                && other.offset < o.offset + o.width + 0.05 - EPS
            {
                return Err(Opening);
            }
        }
    }
    for o in &d.furniture {
        if !ids.insert(o.id) || o.id == 0 || o.id >= d.next_id {
            return Err(DuplicateId);
        }
        if !o.center.finite()
            || o.center.x.abs() > 1000.0
            || o.center.y.abs() > 1000.0
            || !bounded(o.width, 0.1, 20.0)
            || !bounded(o.depth, 0.1, 20.0)
            || !bounded(o.height, 0.1, 10.0)
            || !o.angle.is_finite()
            || o.angle.abs() > std::f64::consts::TAU * 100.0
        {
            return Err(Furniture);
        }
    }
    Ok(())
}

pub fn outer_corners(room: &Room) -> Vec<Point> {
    (0..room.vertices.len())
        .map(|i| {
            let a = room.vertices[(i + room.vertices.len() - 1) % room.vertices.len()];
            let p = room.vertices[i];
            let b = room.vertices[(i + 1) % room.vertices.len()];
            let u = (p - a) / (p - a).length();
            let v = (b - p) / (b - p).length();
            let n1 = Point::new(u.y, -u.x);
            let n2 = Point::new(v.y, -v.x);
            p + (n1 + n2) * (room.thickness / (1.0 + n1.dot(n2)))
        })
        .collect()
}
pub fn wall_quad(room: &Room, wall: usize, start: f64, end: f64) -> [Point; 4] {
    let (a, b) = room.edge(wall);
    let u = (b - a) / a.distance(b);
    let n = Point::new(u.y, -u.x) * room.thickness;
    let corners = outer_corners(room);
    let len = a.distance(b);
    let p = a + u * start;
    let q = a + u * end;
    [
        p,
        q,
        if (end - len).abs() < EPS {
            corners[(wall + 1) % corners.len()]
        } else {
            q + n
        },
        if start.abs() < EPS {
            corners[wall]
        } else {
            p + n
        },
    ]
}

/// Ear clipping supports both convex and concave simple counterclockwise floors.
pub fn triangulate(p: &[Point]) -> Vec<[usize; 3]> {
    let mut indices: Vec<_> = (0..p.len()).collect();
    let mut triangles = vec![];
    let inside = |q: Point, a: Point, b: Point, c: Point| {
        (b - a).cross(q - a) >= -EPS && (c - b).cross(q - b) >= -EPS && (a - c).cross(q - c) >= -EPS
    };
    while indices.len() > 3 {
        let mut found = false;
        for i in 0..indices.len() {
            let a = indices[(i + indices.len() - 1) % indices.len()];
            let b = indices[i];
            let c = indices[(i + 1) % indices.len()];
            if (p[b] - p[a]).cross(p[c] - p[b]) <= EPS {
                continue;
            }
            if indices
                .iter()
                .any(|&j| j != a && j != b && j != c && inside(p[j], p[a], p[b], p[c]))
            {
                continue;
            }
            triangles.push([a, b, c]);
            indices.remove(i);
            found = true;
            break;
        }
        if !found {
            return vec![];
        }
    }
    if indices.len() == 3 {
        triangles.push([indices[0], indices[1], indices[2]]);
    }
    triangles
}

pub fn convex_overlap(a: &[Point], b: &[Point]) -> bool {
    for polygon in [a, b] {
        for i in 0..polygon.len() {
            let edge = polygon[(i + 1) % polygon.len()] - polygon[i];
            let axis = Point::new(-edge.y, edge.x);
            let extent = |p: &[Point]| {
                p.iter()
                    .map(|q| q.dot(axis))
                    .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), x| {
                        (lo.min(x), hi.max(x))
                    })
            };
            let (alo, ahi) = extent(a);
            let (blo, bhi) = extent(b);
            if ahi <= blo + EPS || bhi <= alo + EPS {
                return false;
            }
        }
    }
    true
}
pub fn door_swing(room: &Room, o: &Opening) -> Vec<Point> {
    let (a, b) = room.edge(o.wall);
    let u = (b - a) / a.distance(b);
    let inward = Point::new(-u.y, u.x);
    let hinge = a + u * (o.offset + if o.hinge_at_end { o.width } else { 0.0 });
    let along = u * if o.hinge_at_end { -1.0 } else { 1.0 };
    let mut p = vec![hinge];
    for i in 0..=24 {
        let t = i as f64 / 24.0 * std::f64::consts::FRAC_PI_2;
        p.push(hinge + (along * t.cos() + inward * t.sin()) * o.width);
    }
    p
}

/// Exact intersection with the swept quarter disk; display tessellation is not used for checks.
pub fn blocks_door(room: &Room, o: &Opening, f: &Furniture) -> bool {
    let (a, b) = room.edge(o.wall);
    let u = (b - a) / a.distance(b);
    let inward = Point::new(-u.y, u.x);
    let hinge = a + u * (o.offset + if o.hinge_at_end { o.width } else { 0.0 });
    let along = u * if o.hinge_at_end { -1.0 } else { 1.0 };
    let points = f
        .corners()
        .map(|p| Point::new((p - hinge).dot(along), (p - hinge).dot(inward)));
    let r = o.width;
    let in_sector = |p: Point| p.x >= -EPS && p.y >= -EPS && p.dot(p) <= r * r + EPS;
    if points.iter().copied().any(in_sector) || f.contains(hinge) {
        return true;
    }
    for i in 0..4 {
        let p = points[i];
        let q = points[(i + 1) % 4];
        let v = q - p;
        if segments_intersect(p, q, Point::default(), Point::new(r, 0.0))
            || segments_intersect(p, q, Point::default(), Point::new(0.0, r))
        {
            return true;
        }
        let aa = v.dot(v);
        let bb = 2.0 * p.dot(v);
        let cc = p.dot(p) - r * r;
        let discriminant = bb * bb - 4.0 * aa * cc;
        if discriminant >= 0.0 {
            for t in [
                (-bb - discriminant.sqrt()) / (2.0 * aa),
                (-bb + discriminant.sqrt()) / (2.0 * aa),
            ] {
                if (0.0..=1.0).contains(&t) && in_sector(p + v * t) {
                    return true;
                }
            }
        }
    }
    false
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IssueKind {
    Overlap,
    Outside,
    DoorSwing,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Issue {
    pub kind: IssueKind,
    pub object: Id,
    pub other: Option<Id>,
}
pub fn checks(d: &Document) -> Vec<Issue> {
    let mut issues = vec![];
    for (i, f) in d.furniture.iter().enumerate() {
        let corners = f.corners();
        // Split at boundary intersections so a rectangle cannot bridge a concave notch unnoticed.
        let mut outside = corners.iter().any(|&p| !contains(&d.room.vertices, p));
        for k in 0..4 {
            let (a, b) = (corners[k], corners[(k + 1) % 4]);
            let mut ts = vec![0.0, 1.0];
            for j in 0..d.room.vertices.len() {
                let (c, e) = d.room.edge(j);
                let v = b - a;
                let w = e - c;
                let denom = v.cross(w);
                if denom.abs() > EPS {
                    let t = (c - a).cross(w) / denom;
                    let u = (c - a).cross(v) / denom;
                    if (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u) {
                        ts.push(t);
                    }
                }
            }
            ts.sort_by(f64::total_cmp);
            outside |= ts
                .windows(2)
                .any(|t| !contains(&d.room.vertices, a + (b - a) * ((t[0] + t[1]) / 2.0)));
        }
        if outside {
            issues.push(Issue {
                kind: IssueKind::Outside,
                object: f.id,
                other: None,
            });
        }
        for other in &d.furniture[i + 1..] {
            if convex_overlap(&corners, &other.corners()) {
                issues.push(Issue {
                    kind: IssueKind::Overlap,
                    object: f.id,
                    other: Some(other.id),
                });
            }
        }
        for door in d.openings.iter().filter(|o| o.kind == OpeningKind::Door) {
            if blocks_door(&d.room, door, f) {
                issues.push(Issue {
                    kind: IssueKind::DoorSwing,
                    object: f.id,
                    other: Some(door.id),
                });
            }
        }
    }
    issues
}
pub fn clearance(d: &Document, f: &Furniture) -> f64 {
    let corners = f.corners();
    let mut nearest = f64::INFINITY;
    for i in 0..d.room.vertices.len() {
        let (a, b) = d.room.edge(i);
        for j in 0..4 {
            let (c, e) = (corners[j], corners[(j + 1) % 4]);
            if segments_intersect(a, b, c, e) {
                return 0.0;
            }
            nearest = nearest
                .min(distance_segment(c, a, b).0)
                .min(distance_segment(a, c, e).0);
        }
    }
    if corners.iter().any(|&p| !contains(&d.room.vertices, p)) {
        0.0
    } else {
        nearest
    }
}

/// Snap in world space; tolerance is obtained from logical pixels / logical pixels per metre.
pub fn snap(d: &Document, p: Point, grid: f64, tolerance: f64, exclude: Option<Id>) -> Point {
    let mut out = Point::new((p.x / grid).round() * grid, (p.y / grid).round() * grid);
    let mut dx = tolerance;
    let mut dy = tolerance;
    for q in d.room.vertices.iter().copied().chain(
        d.furniture
            .iter()
            .filter(|f| Some(f.id) != exclude)
            .flat_map(|f| f.corners().into_iter().chain([f.center])),
    ) {
        if (p.x - q.x).abs() < dx {
            out.x = q.x;
            dx = (p.x - q.x).abs();
        }
        if (p.y - q.y).abs() < dy {
            out.y = q.y;
            dy = (p.y - q.y).abs();
        }
    }
    out
}

/// Align furniture edges with inner walls and other furniture while retaining a metric grid.
pub fn snap_placement(
    d: &Document,
    f: &Furniture,
    center: Point,
    grid: f64,
    tolerance: f64,
) -> Point {
    let mut candidate = f.clone();
    candidate.center = Point::new(
        (center.x / grid).round() * grid,
        (center.y / grid).round() * grid,
    );
    let mut targets: Vec<_> = (0..d.room.vertices.len()).map(|i| d.room.edge(i)).collect();
    for other in d.furniture.iter().filter(|other| other.id != f.id) {
        let c = other.corners();
        for i in 0..4 {
            targets.push((c[i], c[(i + 1) % 4]));
        }
    }
    let mut dx: Option<f64> = None;
    let mut dy: Option<f64> = None;
    let mut diagonal: Option<Point> = None;
    for (a, b) in targets {
        let length = a.distance(b);
        let direction = (b - a) / length;
        let normal = Point::new(-direction.y, direction.x);
        for point in candidate.corners() {
            let projection = (point - a).dot(direction);
            let distance = (point - a).dot(normal);
            if projection < -EPS || projection > length + EPS || distance.abs() > tolerance {
                continue;
            }
            let shift = normal * (-distance);
            if normal.x.abs() > 0.999999 {
                if dx.is_none_or(|old| shift.x.abs() < old.abs()) {
                    dx = Some(shift.x);
                }
            } else if normal.y.abs() > 0.999999 {
                if dy.is_none_or(|old| shift.y.abs() < old.abs()) {
                    dy = Some(shift.y);
                }
            } else if diagonal.is_none_or(|old| shift.length() < old.length()) {
                diagonal = Some(shift);
            }
        }
    }
    if dx.is_some() || dy.is_some() {
        candidate.center + Point::new(dx.unwrap_or(0.0), dy.unwrap_or(0.0))
    } else {
        candidate.center + diagonal.unwrap_or_default()
    }
}
