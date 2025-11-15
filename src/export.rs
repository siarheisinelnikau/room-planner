use crate::{geometry, model::*, scene, storage};
use std::{fmt::Write, path::Path};

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
pub fn svg(d: &Document) -> Result<String, String> {
    geometry::validate(d).map_err(|e| e.to_string())?;
    let (min, max) = scene::bounds(&geometry::outer_corners(&d.room));
    let margin = 0.85;
    let scale = 120.0;
    let width = (max.x - min.x + margin * 2.0) * scale;
    let height = (max.y - min.y + margin * 2.0 + 0.45) * scale;
    let map = |p: Point| {
        Point::new(
            (p.x - min.x + margin) * scale,
            (max.y - p.y + margin + 0.35) * scale,
        )
    };
    let points = |p: &[Point]| {
        p.iter()
            .map(|&p| {
                let p = map(p);
                format!("{:.3},{:.3}", p.x, p.y)
            })
            .collect::<Vec<_>>()
            .join(" ")
    };
    let mut out = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width:.0}\" height=\"{height:.0}\" viewBox=\"0 0 {width:.3} {height:.3}\"><rect width=\"100%\" height=\"100%\" fill=\"#fff\"/><g font-family=\"Segoe UI,Arial,sans-serif\" fill=\"#233d3b\"><text x=\"28\" y=\"32\" font-size=\"20\">{}</text><text x=\"28\" y=\"53\" font-size=\"12\">RoomPlanner · {}² · 1 {} = {:.4} m</text>",
        escape(&d.title),
        format_args!(
            "{:.2} {}",
            geometry::signed_area(&d.room.vertices) * d.units.factor().powi(2),
            d.units.symbol()
        ),
        d.units.symbol(),
        1.0 / d.units.factor()
    );
    write!(
        out,
        "<polygon points=\"{}\" fill=\"#f5f3eb\"/>",
        points(&d.room.vertices)
    )
    .unwrap();
    for wall in 0..d.room.vertices.len() {
        let (a, b) = d.room.edge(wall);
        let len = a.distance(b);
        let u = (b - a) / len;
        let n = Point::new(u.y, -u.x);
        write!(
            out,
            "<polygon points=\"{}\" fill=\"#425e58\" stroke=\"#233d3b\" stroke-width=\"1\"/>",
            points(&geometry::wall_quad(&d.room, wall, 0.0, len))
        )
        .unwrap();
        let p = map(a + n * (d.room.thickness + 0.25));
        let q = map(b + n * (d.room.thickness + 0.25));
        let c = (p + q) / 2.0;
        write!(out,"<path d=\"M {:.3} {:.3} L {:.3} {:.3}\" fill=\"none\" stroke=\"#84958e\"/><circle cx=\"{:.3}\" cy=\"{:.3}\" r=\"2\"/><circle cx=\"{:.3}\" cy=\"{:.3}\" r=\"2\"/><text x=\"{:.3}\" y=\"{:.3}\" text-anchor=\"middle\" font-size=\"13\" stroke=\"white\" stroke-width=\"4\" paint-order=\"stroke\">{}</text>",p.x,p.y,q.x,q.y,p.x,p.y,q.x,q.y,c.x,c.y-5.0,d.units.format(len)).unwrap();
    }
    for o in &d.openings {
        let quad = geometry::wall_quad(&d.room, o.wall, o.offset, o.offset + o.width);
        write!(
            out,
            "<polygon points=\"{}\" fill=\"{}\"/>",
            points(&quad),
            if o.kind == OpeningKind::Window {
                "#b5d7db"
            } else {
                "#fff"
            }
        )
        .unwrap();
        if o.kind == OpeningKind::Door {
            let mut p = geometry::door_swing(&d.room, o);
            p.push(p[0]);
            write!(
                out,
                "<polyline points=\"{}\" fill=\"none\" stroke=\"#9b754d\" stroke-width=\"1.5\"/>",
                points(&p)
            )
            .unwrap();
        }
        let (a, b) = d.room.edge(o.wall);
        let c = map(a + (b - a) / a.distance(b) * (o.offset + o.width / 2.0));
        write!(out,"<text x=\"{:.2}\" y=\"{:.2}\" text-anchor=\"middle\" font-size=\"10\" stroke=\"#fff\" stroke-width=\"3\" paint-order=\"stroke\">{}</text>",c.x,c.y+4.0,d.units.format(o.width)).unwrap();
    }
    for f in &d.furniture {
        write!(
            out,
            "<polygon points=\"{}\" fill=\"#bdd2c8\" stroke=\"#377568\" stroke-width=\"1.5\"/>",
            points(&f.corners())
        )
        .unwrap();
        let p = map(f.center);
        write!(out,"<text x=\"{:.3}\" y=\"{:.3}\" text-anchor=\"middle\" font-size=\"11\">#{} · {} × {}</text>",p.x,p.y,f.id,d.units.format(f.width),d.units.format(f.depth)).unwrap();
    }
    out.push_str("</g></svg>");
    Ok(out)
}
pub fn save_svg(path: &Path, d: &Document) -> Result<(), String> {
    storage::atomic_write(path, svg(d)?.as_bytes())
}
