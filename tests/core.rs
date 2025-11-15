use room_planner::{
    editor::{Editor, Selection},
    export,
    geometry::{self, IssueKind},
    model::*,
    scene::{Scene, Vec3},
    storage,
    ui::parse_coordinates,
};
use std::f64::consts::{FRAC_PI_2, PI};

fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-8, "{a} != {b}");
}
fn with_furniture() -> Document {
    let mut d = Document::default();
    d.furniture
        .push(Furniture::new(1, FurnitureKind::Desk, Point::new(2.0, 2.0)));
    d.next_id = 2;
    d
}
#[test]
fn rectangle_and_concave_room_are_valid() {
    let d = Document::sample();
    geometry::validate(&d).unwrap();
    let d = Document {
        room: Room::l_shape(),
        openings: vec![],
        ..d
    };
    geometry::validate(&d).unwrap();
}
#[test]
fn rejects_crossing_duplicate_and_clockwise_outlines() {
    for vertices in [
        vec![
            Point::new(0.0, 0.0),
            Point::new(4.0, 4.0),
            Point::new(0.0, 4.0),
            Point::new(4.0, 0.0),
        ],
        vec![
            Point::new(0.0, 0.0),
            Point::new(4.0, 0.0),
            Point::new(4.0, 0.0),
            Point::new(0.0, 4.0),
        ],
        vec![
            Point::new(0.0, 0.0),
            Point::new(0.0, 4.0),
            Point::new(4.0, 4.0),
            Point::new(4.0, 0.0),
        ],
    ] {
        assert!(!geometry::validate_polygon(&vertices));
    }
}
#[test]
fn rejects_nan_extreme_and_acute_reversals() {
    let mut d = Document::default();
    d.room.vertices[1].x = f64::NAN;
    assert!(geometry::validate(&d).is_err());
    d = Document::default();
    d.room.height = f64::INFINITY;
    assert!(geometry::validate(&d).is_err());
    assert!(!geometry::validate_polygon(&[
        Point::new(0.0, 0.0),
        Point::new(5.0, 0.0),
        Point::new(0.01, 0.01),
        Point::new(0.0, 4.0)
    ]));
}
#[test]
fn units_round_trip_and_feet_are_not_rounded_inches() {
    for u in [Units::Metres, Units::Centimetres, Units::Feet] {
        near(u.from_display(u.display(3.127)), 3.127);
    }
    near(Units::Feet.from_display(10.0), 3.048);
}
#[test]
fn snapping_is_independent_of_display_units() {
    let mut d = Document::default();
    let a = geometry::snap(&d, Point::new(0.036, 2.345), 0.1, 0.05, None);
    d.units = Units::Feet;
    let b = geometry::snap(&d, Point::new(0.036, 2.345), 0.1, 0.05, None);
    assert_eq!(a, b);
    near(a.x, 0.0);
    near(a.y, 2.3);
}
#[test]
fn snapping_can_exclude_the_dragged_object() {
    let d = with_furniture();
    let p = Point::new(2.023, 2.023);
    let snapped = geometry::snap(&d, p, 0.1, 0.04, None);
    near(snapped.x, 2.0);
    let without = geometry::snap(&d, p, 0.07, 0.04, Some(1));
    assert_ne!(snapped, without);
}
#[test]
fn concave_triangulation_preserves_area() {
    let room = Room::l_shape();
    let tris = geometry::triangulate(&room.vertices);
    assert_eq!(tris.len(), room.vertices.len() - 2);
    let area: f64 = tris
        .iter()
        .map(|t| geometry::signed_area(&t.map(|i| room.vertices[i])))
        .sum();
    near(area, geometry::signed_area(&room.vertices));
}
#[test]
fn wall_miters_preserve_exact_thickness() {
    let room = Room::rectangle(5.0, 4.0);
    let outer = geometry::outer_corners(&room);
    near(outer[0].x, -0.15);
    near(outer[2].y, 4.15);
    let quad = geometry::wall_quad(&room, 0, 0.5, 1.5);
    near(quad[0].y, 0.0);
    near(quad[2].y, -0.15);
    near(quad[0].x, quad[3].x);
}
#[test]
fn openings_cannot_cross_corners_or_overlap() {
    let mut d = Document::sample();
    d.openings[0].offset = 4.5;
    assert_eq!(geometry::validate(&d), Err(ValidationError::Opening));
    d = Document::sample();
    let mut o = d.openings[0].clone();
    o.id = 5;
    o.offset = 1.0;
    d.next_id = 6;
    d.openings.push(o);
    assert_eq!(geometry::validate(&d), Err(ValidationError::Opening));
}
#[test]
fn opening_sill_and_header_must_fit_height() {
    let mut d = Document::sample();
    d.openings[1].bottom = 2.0;
    assert!(geometry::validate(&d).is_err());
    d = Document::sample();
    d.openings[0].bottom = 0.2;
    assert!(geometry::validate(&d).is_err());
}
#[test]
fn ids_must_be_unique_and_monotonic() {
    let mut d = Document::sample();
    d.furniture[0].id = d.openings[0].id;
    assert_eq!(geometry::validate(&d), Err(ValidationError::DuplicateId));
    d = Document::sample();
    d.next_id = 3;
    assert!(geometry::validate(&d).is_err());
}
#[test]
fn rotated_furniture_has_correct_dimensions() {
    let mut f = Furniture::new(1, FurnitureKind::Desk, Point::new(2.0, 3.0));
    f.angle = FRAC_PI_2;
    let c = f.corners();
    near(c[0].distance(c[1]), 1.4);
    near(c[1].distance(c[2]), 0.7);
    assert!(f.contains(Point::new(2.0, 3.6)));
    assert!(!f.contains(Point::new(2.6, 3.0)));
}
#[test]
fn touching_is_allowed_but_real_overlap_is_reported() {
    let mut d = with_furniture();
    let mut f = d.furniture[0].clone();
    f.id = 2;
    f.center.x += f.width;
    d.furniture.push(f);
    d.next_id = 3;
    assert!(
        !geometry::checks(&d)
            .iter()
            .any(|i| i.kind == IssueKind::Overlap)
    );
    d.furniture[1].center.x -= 0.01;
    assert!(
        geometry::checks(&d)
            .iter()
            .any(|i| i.kind == IssueKind::Overlap)
    );
}
#[test]
fn rotated_collision_uses_oriented_rectangles() {
    let mut d = with_furniture();
    let mut f = d.furniture[0].clone();
    f.id = 2;
    f.angle = PI / 4.0;
    f.center.x += 1.0;
    d.furniture.push(f);
    d.next_id = 3;
    assert!(
        geometry::checks(&d)
            .iter()
            .any(|i| i.kind == IssueKind::Overlap)
    );
}
#[test]
fn furniture_outside_concave_room_is_reported() {
    let mut d = with_furniture();
    d.room = Room::l_shape();
    d.furniture[0].center = Point::new(4.0, 3.0);
    assert!(
        geometry::checks(&d)
            .iter()
            .any(|i| i.kind == IssueKind::Outside)
    );
}
#[test]
fn furniture_bridge_across_concave_notch_is_reported() {
    let mut d = with_furniture();
    d.room.vertices = vec![
        Point::new(0.0, 0.0),
        Point::new(5.0, 0.0),
        Point::new(5.0, 5.0),
        Point::new(3.0, 5.0),
        Point::new(3.0, 2.0),
        Point::new(2.0, 2.0),
        Point::new(2.0, 5.0),
        Point::new(0.0, 5.0),
    ];
    let f = &mut d.furniture[0];
    f.center = Point::new(2.5, 3.0);
    f.width = 4.0;
    f.depth = 0.5;
    assert!(
        f.corners()
            .iter()
            .all(|&p| geometry::contains(&d.room.vertices, p))
    );
    assert!(
        geometry::checks(&d)
            .iter()
            .any(|i| i.kind == IssueKind::Outside)
    );
}
#[test]
fn door_swing_obstruction_and_hinge_change_are_checked() {
    let mut d = Document::sample();
    d.furniture.clear();
    let mut f = Furniture::new(5, FurnitureKind::Cabinet, Point::new(0.55, 0.65));
    f.width = 0.1;
    f.depth = 0.1;
    d.furniture.push(f);
    d.next_id = 6;
    assert!(
        geometry::checks(&d)
            .iter()
            .any(|i| i.kind == IssueKind::DoorSwing)
    );
    d.openings[0].hinge_at_end = true;
    assert!(
        !geometry::checks(&d)
            .iter()
            .any(|i| i.kind == IssueKind::DoorSwing)
    );
}
#[test]
fn clearance_uses_rotated_edges_and_zero_for_outside() {
    let mut d = with_furniture();
    near(geometry::clearance(&d, &d.furniture[0]), 1.3);
    d.furniture[0].center.x = 0.0;
    near(geometry::clearance(&d, &d.furniture[0]), 0.0);
}
#[test]
fn undo_redo_restore_ids_geometry_and_dirty_state() {
    let original = Document::default();
    let mut e = Editor::new(original.clone());
    let id = e
        .add_furniture(FurnitureKind::Bed, Point::new(2.0, 2.0))
        .unwrap();
    assert_eq!(id, 1);
    assert!(e.dirty());
    e.undo();
    assert_eq!(e.document, original);
    assert!(!e.dirty());
    e.redo();
    assert_eq!(e.document.furniture[0].id, id);
    e.mark_saved();
    assert!(!e.dirty());
    e.selection = Some(Selection::Furniture(id));
    e.delete_selection().unwrap();
    assert!(e.dirty());
    e.undo();
    assert!(!e.dirty());
}
#[test]
fn invalid_commands_are_atomic_and_do_not_enter_history() {
    let mut e = Editor::new(Document::default());
    let original = e.document.clone();
    assert!(
        e.apply(|d| d.room.vertices[0] = d.room.vertices[1])
            .is_err()
    );
    assert_eq!(e.document, original);
    assert!(!e.can_undo());
}
#[test]
fn drag_coalesces_to_one_undo_and_cancel_is_reversible() {
    let mut e = Editor::new(with_furniture());
    let original = e.document.clone();
    e.begin_gesture();
    for i in 1..20 {
        e.apply(|d| d.furniture[0].center.x = 2.0 + i as f64 * 0.01)
            .unwrap();
    }
    e.finish_gesture();
    e.undo();
    assert_eq!(e.document, original);
    assert!(!e.can_undo());
    e.begin_gesture();
    e.apply(|d| d.furniture[0].angle = 1.0).unwrap();
    e.cancel_gesture();
    assert_eq!(e.document, original);
}
#[test]
fn editing_after_undo_discards_redo_branch() {
    let mut e = Editor::new(Document::default());
    e.add_furniture(FurnitureKind::Desk, Point::new(2.0, 2.0))
        .unwrap();
    e.undo();
    e.add_furniture(FurnitureKind::Sofa, Point::new(3.0, 3.0))
        .unwrap();
    assert!(!e.can_redo());
}
#[test]
fn corner_deletion_protects_attached_openings() {
    let mut e = Editor::new(Document::sample());
    e.selection = Some(Selection::Vertex(0));
    assert!(e.delete_selection().is_err());
    assert_eq!(e.document.room.vertices.len(), 4);
}
#[test]
fn splitting_wall_and_undo_remap_openings() {
    let mut e = Editor::new(Document::sample());
    e.insert_vertex(1, Point::new(5.0, 2.0)).unwrap();
    assert_eq!(e.document.openings[1].wall, 3);
    e.undo();
    assert_eq!(e.document.openings[1].wall, 2);
}
#[test]
fn coordinate_import_normalizes_winding_but_rejects_bad_data() {
    let points = parse_coordinates("0 0\n0 4\n5 4\n5 0").unwrap();
    near(geometry::signed_area(&points), 20.0);
    for input in ["0 0\n5 4\n0 4\n5 0", "NaN 0\n0 5\n5 5", "1 2 3", "0 0\n1 0"] {
        assert!(parse_coordinates(input).is_err());
    }
}
#[test]
fn atomic_save_and_reopen_round_trip_real_model() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("synthetic.roomplan");
    let original = Document::sample();
    storage::save_document(&path, &original).unwrap();
    assert_eq!(storage::load_document(&path).unwrap().document, original);
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 1);
}
#[test]
fn corrupted_primary_recovers_previous_valid_save() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("synthetic.roomplan");
    let original = Document::sample();
    storage::save_document(&path, &original).unwrap();
    let mut newer = original.clone();
    newer.title = "Changed".into();
    storage::save_document(&path, &newer).unwrap();
    std::fs::write(&path, b"{broken").unwrap();
    let recovered = storage::load_document(&path).unwrap();
    assert!(recovered.recovered);
    assert_eq!(recovered.document, original);
    storage::save_document(&path, &newer).unwrap();
    assert_eq!(
        storage::read_document(&storage::backup_path(&path)).unwrap(),
        original
    );
}
#[test]
fn unsupported_format_never_opens_as_current_document() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("future.roomplan");
    let d = Document {
        format_version: 99,
        ..Default::default()
    };
    storage::save_json(&path, &d).unwrap();
    assert!(storage::load_document(&path).is_err());
}
#[test]
fn svg_exports_scaled_dimensions_and_escapes_project_name() {
    let mut d = Document::sample();
    d.title = "<room & \"test\">".into();
    let svg = export::svg(&d).unwrap();
    assert!(svg.contains("&lt;room &amp; &quot;test&quot;&gt;"));
    assert!(svg.contains("5.00 m"));
    assert!(svg.contains("polyline"));
    d.units = Units::Feet;
    assert!(export::svg(&d).unwrap().contains("16.40 ft"));
}
fn ray_hit(origin: Vec3, direction: Vec3, tri: [Vec3; 3]) -> bool {
    let sub = |a: Vec3, b: Vec3| Vec3::new(a.x - b.x, a.y - b.y, a.z - b.z);
    let cross = |a: Vec3, b: Vec3| {
        Vec3::new(
            a.y * b.z - a.z * b.y,
            a.z * b.x - a.x * b.z,
            a.x * b.y - a.y * b.x,
        )
    };
    let dot = |a: Vec3, b: Vec3| a.x * b.x + a.y * b.y + a.z * b.z;
    let e1 = sub(tri[1], tri[0]);
    let e2 = sub(tri[2], tri[0]);
    let h = cross(direction, e2);
    let a = dot(e1, h);
    if a.abs() < 1e-8 {
        return false;
    }
    let f = 1.0 / a;
    let s = sub(origin, tri[0]);
    let u = f * dot(s, h);
    if !(0.0..=1.0).contains(&u) {
        return false;
    }
    let q = cross(s, e1);
    let v = f * dot(direction, q);
    v >= 0.0 && u + v <= 1.0 && f * dot(e2, q) > 0.0
}
#[test]
fn derived_scene_has_real_door_void_sill_and_header() {
    let mut d = Document::sample();
    d.furniture.clear();
    let s = Scene::from_document(&d);
    let wall_hits = |x: f64, y: f64| {
        s.triangles
            .iter()
            .filter(|t| t.wall == Some(0) && t.object.is_none())
            .any(|t| ray_hit(Vec3::new(x, y, -1.0), Vec3::new(0.0, 0.0, 1.0), t.vertices))
    };
    assert!(!wall_hits(0.95, 1.0));
    assert!(wall_hits(0.95, 2.4));
    assert!(wall_hits(2.5, 1.0));
}
#[test]
fn derived_scene_keeps_rotated_furniture_in_same_footprint() {
    let mut d = with_furniture();
    let f = &mut d.furniture[0];
    f.angle = 0.7;
    f.width = 2.2;
    f.depth = 0.8;
    let s = Scene::from_document(&d);
    let f = &d.furniture[0];
    for t in s.triangles.iter().filter(|t| t.object == Some(f.id)) {
        for v in t.vertices {
            let p = f.local(Point::new(v.x, v.z));
            assert!(p.x.abs() <= f.width / 2.0 + EPS);
            assert!(p.y.abs() <= f.depth / 2.0 + EPS);
            assert!((0.0..=f.height + EPS).contains(&v.y));
        }
    }
}
#[test]
fn scene_scales_with_edited_room_and_wall_thickness() {
    let mut d = Document {
        room: Room::rectangle(8.0, 6.0),
        ..Default::default()
    };
    d.room.thickness = 0.3;
    let s = Scene::from_document(&d);
    let min = s
        .triangles
        .iter()
        .flat_map(|t| t.vertices)
        .map(|p| p.x)
        .fold(f64::INFINITY, f64::min);
    let max = s
        .triangles
        .iter()
        .flat_map(|t| t.vertices)
        .map(|p| p.z)
        .fold(f64::NEG_INFINITY, f64::max);
    near(min, -0.3);
    near(max, 6.3);
}

#[test]
fn thick_walls_cannot_fold_across_a_narrow_notch() {
    let d = Document {
        room: Room {
            vertices: vec![
                Point::new(0.0, 0.0),
                Point::new(5.0, 0.0),
                Point::new(5.0, 5.0),
                Point::new(2.6, 5.0),
                Point::new(2.6, 2.0),
                Point::new(2.4, 2.0),
                Point::new(2.4, 5.0),
                Point::new(0.0, 5.0),
            ],
            thickness: 0.15,
            height: 2.7,
        },
        ..Default::default()
    };
    assert!(geometry::validate_polygon(&d.room.vertices));
    assert_eq!(geometry::validate(&d), Err(ValidationError::Wall));
}
#[test]
fn concave_miter_requires_space_before_an_opening() {
    let mut e = Editor::new(Document {
        room: Room::l_shape(),
        ..Default::default()
    });
    assert!(e.add_opening(OpeningKind::Door, 3, 0.05).is_err());
    assert!(e.add_opening(OpeningKind::Door, 3, 0.25).is_ok());
}
#[test]
fn exact_door_check_detects_arc_crossing_without_rectangle_corner_inside() {
    let d = Document::sample();
    let door = &d.openings[0];
    let theta = 46.875_f64.to_radians();
    let radial = Point::new(theta.cos(), theta.sin());
    let mut f = Furniture::new(
        5,
        FurnitureKind::Cabinet,
        Point::new(0.5, 0.0) + radial * (0.9 + 0.05 - 0.00005),
    );
    f.width = 0.1;
    f.depth = 0.1;
    f.angle = theta;
    assert!(geometry::blocks_door(&d.room, door, &f));
    assert!(!geometry::convex_overlap(
        &f.corners(),
        &geometry::door_swing(&d.room, door)
    ));
}
#[test]
fn every_parametric_object_stays_inside_its_declared_bounds() {
    for kind in FurnitureKind::ALL {
        let mut d = with_furniture();
        d.furniture[0].kind = kind;
        d.furniture[0].angle = 1.17;
        let s = Scene::from_document(&d);
        let f = &d.furniture[0];
        for v in s
            .triangles
            .iter()
            .filter(|t| t.object == Some(f.id))
            .flat_map(|t| t.vertices)
        {
            let p = f.local(Point::new(v.x, v.z));
            assert!(p.x.abs() <= f.width / 2.0 + EPS);
            assert!(p.y.abs() <= f.depth / 2.0 + EPS);
            assert!(v.y >= -EPS && v.y <= f.height + EPS);
        }
    }
}
#[test]
fn xml_control_characters_and_zero_id_allocator_are_rejected() {
    let d = Document {
        title: "bad\0title".into(),
        ..Default::default()
    };
    assert!(geometry::validate(&d).is_err());
    let d = Document {
        next_id: 0,
        ..Default::default()
    };
    assert!(geometry::validate(&d).is_err());
}

#[test]
fn furniture_edge_snaps_to_inner_wall_instead_of_putting_its_center_in_the_wall() {
    let d = with_furniture();
    let f = &d.furniture[0];
    let center = geometry::snap_placement(&d, f, Point::new(0.73, 2.0), 0.1, 0.08);
    let mut moved = f.clone();
    moved.center = center;
    near(moved.corners()[0].x, 0.0);
    assert!(
        moved
            .corners()
            .iter()
            .all(|&p| geometry::contains(&d.room.vertices, p))
    );
}
#[test]
fn rotated_furniture_edges_snap_to_a_matching_rotated_object() {
    let mut d = with_furniture();
    d.furniture[0].angle = 0.4;
    let original = d.furniture[0].clone();
    let mut moving = original.clone();
    moving.id = 2;
    moving.center = original.center + Point::new(original.width + 0.02, 0.0).rotate(original.angle);
    d.furniture.push(moving.clone());
    d.next_id = 3;
    let center = geometry::snap_placement(&d, &moving, moving.center, 0.001, 0.05);
    let along = (center - original.center).rotate(-original.angle);
    near(along.x, original.width);
}
