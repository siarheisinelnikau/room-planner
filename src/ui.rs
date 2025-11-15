use crate::{
    editor::{Editor, Selection},
    export, geometry,
    i18n::Language,
    model::*,
    scene::{self, Camera, Scene},
    settings::{Settings, Theme},
    storage, updates,
};
use egui::{
    Align2, Color32, FontId, Key, PointerButton, Pos2, Rect, Response, Sense, Stroke, Vec2,
};
use std::{
    path::PathBuf,
    sync::{Arc, LazyLock, mpsc},
    time::Instant,
};

#[derive(Clone, Copy, Debug, PartialEq)]
enum Tool {
    Select,
    Draw,
    Furniture(FurnitureKind),
    Opening(OpeningKind),
}
#[derive(Clone, Copy, Debug, PartialEq)]
enum View {
    Plan,
    Three,
}
#[derive(Clone)]
enum Drag {
    Move { original: Furniture, start: Point },
    Resize { original: Furniture, sign: Point },
    Rotate { original: Furniture },
    Vertex { index: usize },
    Opening { original: Opening, start: Point },
}
enum Pending {
    New(bool),
    Open,
    Close,
    Install,
}

struct PreviewCache {
    revision: u64,
    camera: Camera,
    size: [usize; 2],
    dark: bool,
    selected: Option<Id>,
    texture: egui::TextureHandle,
}
static LICENSE_LINES: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    concat!(
        include_str!("../LICENSE"),
        "\n\n",
        include_str!("../THIRD_PARTY_LICENSES.txt")
    )
    .lines()
    .collect()
});

pub struct Workspace {
    pub editor: Editor,
    pub settings: Settings,
    pub close_requested: bool,
    path: Option<PathBuf>,
    view: View,
    tool: Tool,
    settings_open: bool,
    licenses_open: bool,
    shape_open: bool,
    import_open: bool,
    import_text: String,
    draw_points: Vec<Point>,
    drag: Option<Drag>,
    pending: Option<Pending>,
    zoom: f64,
    pan: Vec2,
    fit: bool,
    camera: Camera,
    draft: Document,
    draft_revision: u64,
    draft_selection: Option<Selection>,
    room_width: f64,
    room_depth: f64,
    wall_length: f64,
    notice: Option<(&'static str, String)>,
    update: updates::State,
    sender: mpsc::Sender<updates::State>,
    receiver: mpsc::Receiver<updates::State>,
    wake: Arc<dyn Fn() + Send + Sync>,
    transport: Arc<dyn updates::Transport>,
    scene_cache: Option<(u64, Scene)>,
    image_cache: Option<PreviewCache>,
    check_cache: (u64, Vec<geometry::Issue>),
    pub last_layout_micros: u64,
}
impl Workspace {
    pub fn new(wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        let (settings, settings_error) = match Settings::load() {
            Ok(s) => (s, None),
            Err(e) => (Settings::default(), Some(("settings_load_error", e))),
        };
        let mut workspace = Self::with_settings(settings, Arc::new(updates::Https), wake);
        workspace.notice = settings_error;
        workspace
    }
    pub fn with_settings(
        settings: Settings,
        transport: Arc<dyn updates::Transport>,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> Self {
        let document = Document::default();
        let camera = Camera::fit(&document);
        let (sender, receiver) = mpsc::channel();
        let mut workspace = Self {
            editor: Editor::new(document.clone()),
            settings,
            close_requested: false,
            path: None,
            view: View::Plan,
            tool: Tool::Select,
            settings_open: false,
            licenses_open: false,
            shape_open: false,
            import_open: false,
            import_text: String::new(),
            draw_points: vec![],
            drag: None,
            pending: None,
            zoom: 100.0,
            pan: Vec2::ZERO,
            fit: true,
            camera,
            draft: document,
            draft_revision: u64::MAX,
            draft_selection: None,
            room_width: 5.0,
            room_depth: 4.0,
            wall_length: 0.0,
            notice: None,
            update: updates::State::Idle,
            sender,
            receiver,
            wake,
            transport,
            scene_cache: None,
            image_cache: None,
            check_cache: (u64::MAX, vec![]),
            last_layout_micros: 0,
        };
        if workspace.settings.startup_updates {
            workspace.check_updates();
        }
        workspace
    }
    pub fn request_close(&mut self) {
        self.request(Pending::Close);
    }
    pub fn open_path(&mut self, path: PathBuf) -> Result<(), String> {
        let loaded = storage::load_document(&path)?;
        self.editor.replace(loaded.document);
        self.path = if loaded.recovered { None } else { Some(path) };
        self.reset_view();
        if loaded.recovered {
            self.error("recovered", "");
        }
        Ok(())
    }
    pub fn update_state(&self) -> &updates::State {
        &self.update
    }
    pub fn report_open_error(&mut self, error: String) {
        self.error("io_error", error);
    }
    pub fn diagnostic_three_view(&mut self) {
        self.view = View::Three;
    }
    fn t(&self, key: &str) -> &'static str {
        self.settings.language.text(key)
    }
    fn error(&mut self, key: &'static str, detail: impl ToString) {
        self.notice = Some((key, detail.to_string()));
    }
    fn result(&mut self, result: Result<bool, ValidationError>) {
        if let Err(e) = result {
            self.error("invalid_geometry", e);
        }
    }
    fn request(&mut self, pending: Pending) {
        self.editor.finish_gesture();
        self.drag = None;
        if self.editor.dirty() {
            self.pending = Some(pending);
        } else {
            self.proceed(pending);
        }
    }
    fn proceed(&mut self, pending: Pending) {
        match pending {
            Pending::Close => self.close_requested = true,
            Pending::New(sample) => {
                let d = if sample {
                    Document::sample()
                } else {
                    Document::default()
                };
                self.editor.replace(d);
                self.path = None;
                self.reset_view();
            }
            Pending::Open => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter(self.t("project"), &["roomplan"])
                    .pick_file()
                    && let Err(e) = self.open_path(path)
                {
                    self.error("io_error", e);
                }
            }
            Pending::Install => {
                if let updates::State::Ready(path) = &self.update {
                    match updates::launch_helper(path, self.path.clone()) {
                        Ok(()) => self.close_requested = true,
                        Err(e) => {
                            self.update = updates::State::Failed {
                                key: "install_failed",
                                detail: e,
                            }
                        }
                    }
                }
            }
        }
    }
    fn reset_view(&mut self) {
        self.tool = Tool::Select;
        self.draw_points.clear();
        self.fit = true;
        self.camera = Camera::fit(&self.editor.document);
        self.draft_revision = u64::MAX;
        self.check_cache.0 = u64::MAX;
        self.scene_cache = None;
        self.image_cache = None;
    }
    fn save(&mut self, save_as: bool) -> bool {
        let path = if save_as || self.path.is_none() {
            rfd::FileDialog::new()
                .add_filter(self.t("project"), &["roomplan"])
                .set_file_name("Room.roomplan")
                .save_file()
        } else {
            self.path.clone()
        };
        let Some(path) = path else {
            return false;
        };
        match storage::save_document(&path, &self.editor.document) {
            Ok(()) => {
                self.path = Some(path);
                self.editor.mark_saved();
                self.notice = Some(("saved", String::new()));
                true
            }
            Err(e) => {
                self.error("io_error", e);
                false
            }
        }
    }
    fn export(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("SVG", &["svg"])
            .set_file_name("Room.svg")
            .save_file()
        {
            match export::save_svg(&path, &self.editor.document) {
                Ok(()) => self.notice = Some(("exported", String::new())),
                Err(e) => self.error("io_error", e),
            }
        }
    }
    pub fn check_updates(&mut self) {
        if matches!(
            self.update,
            updates::State::Checking | updates::State::Downloading { .. }
        ) {
            return;
        }
        self.update = updates::State::Checking;
        let sender = self.sender.clone();
        let wake = self.wake.clone();
        let transport = self.transport.clone();
        std::thread::spawn(move || {
            let state = match updates::check(transport.as_ref()) {
                Ok(Some(a)) => updates::State::Available(a),
                Ok(None) => updates::State::Current,
                Err(detail) => updates::State::Failed {
                    key: "update_failed",
                    detail,
                },
            };
            let _ = sender.send(state);
            wake();
        });
    }
    fn download_update(&mut self, available: updates::Available) {
        self.update = updates::State::Downloading {
            received: 0,
            total: available.asset.size,
        };
        let sender = self.sender.clone();
        let wake = self.wake.clone();
        let transport = self.transport.clone();
        std::thread::spawn(move || {
            let result = std::env::current_exe()
                .map_err(|e| e.to_string())
                .and_then(|target| {
                    updates::download(transport.as_ref(), &available, &target, &mut |received| {
                        let _ = sender.send(updates::State::Downloading {
                            received,
                            total: available.asset.size,
                        });
                        wake();
                    })
                });
            let state = match result {
                Ok(path) => updates::State::Ready(path),
                Err(detail) => updates::State::Failed {
                    key: "download_failed",
                    detail,
                },
            };
            let _ = sender.send(state);
            wake();
        });
    }
    pub fn poll_jobs(&mut self) {
        while let Ok(state) = self.receiver.try_recv() {
            self.update = state;
        }
    }
    pub fn show(&mut self, ctx: &egui::Context) {
        let start = Instant::now();
        self.poll_jobs();
        self.style(ctx);
        self.keyboard(ctx);
        let lang = self.settings.language;
        let t = |key| lang.text(key);
        egui::TopBottomPanel::top("header")
            .frame(
                egui::Frame::new()
                    .fill(self.surface())
                    .inner_margin(egui::Margin::symmetric(20, 12)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("RoomPlanner")
                            .size(23.0)
                            .strong()
                            .color(self.accent()),
                    );
                    ui.add_space(14.0);
                    ui.label(egui::RichText::new(t("about")).color(self.muted()));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(t("settings")).clicked() {
                            self.settings_open = true;
                        }
                        if matches!(self.update, updates::State::Available(_))
                            && ui.button(t("update_available")).clicked()
                        {
                            self.settings_open = true;
                        }
                    });
                });
                ui.add_space(10.0);
                ui.horizontal_wrapped(|ui| {
                    ui.menu_button(t("new"), |ui| {
                        for (key, sample) in [("new_empty", false), ("sample", true)] {
                            if ui.button(t(key)).clicked() {
                                self.request(Pending::New(sample));
                                ui.close();
                            }
                        }
                    });
                    if ui.button(t("open")).clicked() {
                        self.request(Pending::Open);
                    }
                    if ui.button(t("save")).on_hover_text("Ctrl+S").clicked() {
                        self.save(false);
                    }
                    if ui.button(t("save_as")).clicked() {
                        self.save(true);
                    }
                    if ui.button(t("export")).clicked() {
                        self.export();
                    }
                    ui.separator();
                    if ui
                        .add_enabled(self.editor.can_undo(), egui::Button::new(t("undo")))
                        .on_hover_text("Ctrl+Z")
                        .clicked()
                    {
                        self.editor.undo();
                    }
                    if ui
                        .add_enabled(self.editor.can_redo(), egui::Button::new(t("redo")))
                        .on_hover_text("Ctrl+Y")
                        .clicked()
                    {
                        self.editor.redo();
                    }
                    ui.separator();
                    ui.selectable_value(&mut self.view, View::Plan, t("plan"));
                    ui.selectable_value(&mut self.view, View::Three, t("view3d"));
                });
            });
        egui::TopBottomPanel::bottom("status")
            .frame(
                egui::Frame::new()
                    .fill(self.surface())
                    .inner_margin(egui::Margin::symmetric(16, 8)),
            )
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    let title = if self.editor.document.title == "Room" {
                        t("untitled")
                    } else {
                        &self.editor.document.title
                    };
                    ui.strong(format!(
                        "{}{}",
                        title,
                        if self.editor.dirty() { " *" } else { "" }
                    ));
                    ui.separator();
                    ui.label(t(match self.tool {
                        Tool::Draw => "draw_help",
                        Tool::Furniture(_) => "place_help",
                        Tool::Opening(_) => "opening_help",
                        Tool::Select => {
                            if self.view == View::Plan {
                                "plan_help"
                            } else {
                                "view_help"
                            }
                        }
                    }));
                });
                if let Some((key, detail)) = self.notice.clone() {
                    ui.horizontal_wrapped(|ui| {
                        ui.colored_label(
                            if matches!(key, "saved" | "exported") {
                                self.accent()
                            } else {
                                self.warning()
                            },
                            t(key),
                        );
                        if !detail.is_empty() {
                            ui.collapsing(t("details"), |ui| {
                                ui.label(&detail);
                            });
                        }
                        if ui.small_button("×").on_hover_text(t("close")).clicked() {
                            self.notice = None;
                        }
                    });
                }
            });
        egui::SidePanel::left("palette")
            .exact_width(172.0)
            .resizable(false)
            .frame(egui::Frame::new().fill(self.surface()).inner_margin(14))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.label(egui::RichText::new(t("objects")).strong());
                    ui.add_space(12.0);
                    if ui
                        .selectable_label(self.tool == Tool::Select, t("select"))
                        .clicked()
                    {
                        self.cancel_tool();
                    }
                    if ui
                        .selectable_label(self.tool == Tool::Draw, t("draw"))
                        .clicked()
                    {
                        self.cancel_tool();
                        self.tool = Tool::Draw;
                        self.view = View::Plan;
                    }
                    if ui.button(t("new_room")).clicked() {
                        self.shape_open = true;
                    }
                    ui.add_space(12.0);
                    ui.separator();
                    ui.add_space(8.0);
                    for kind in FurnitureKind::ALL {
                        if ui
                            .selectable_label(
                                self.tool == Tool::Furniture(kind),
                                t(furniture_key(kind)),
                            )
                            .clicked()
                        {
                            self.cancel_tool();
                            self.tool = Tool::Furniture(kind);
                            self.view = View::Plan;
                        }
                    }
                    ui.add_space(12.0);
                    ui.separator();
                    ui.add_space(8.0);
                    for kind in [OpeningKind::Door, OpeningKind::Window, OpeningKind::Passage] {
                        if ui
                            .selectable_label(
                                self.tool == Tool::Opening(kind),
                                t(opening_key(kind)),
                            )
                            .clicked()
                        {
                            self.cancel_tool();
                            self.tool = Tool::Opening(kind);
                            self.view = View::Plan;
                        }
                    }
                    ui.add_space(22.0);
                    let old = self.settings.clone();
                    ui.checkbox(&mut self.settings.snap, t("snap"));
                    if old != self.settings {
                        self.persist_settings();
                    }
                    if ui.button(t("fit")).clicked() {
                        self.fit = true;
                        self.camera = Camera::fit(&self.editor.document);
                    }
                    if self.tool == Tool::Draw {
                        if ui
                            .add_enabled(
                                self.draw_points.len() >= 3,
                                egui::Button::new(t("finish")),
                            )
                            .clicked()
                        {
                            self.finish_drawing();
                        }
                        if ui.button(t("cancel")).clicked() {
                            self.cancel_tool();
                        }
                    }
                    ui.add_space(20.0);
                    ui.label(
                        egui::RichText::new(t("measure"))
                            .small()
                            .color(self.muted()),
                    );
                });
            });
        egui::SidePanel::right("properties")
            .exact_width(270.0)
            .resizable(false)
            .frame(egui::Frame::new().fill(self.surface()).inner_margin(16))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| self.properties(ui));
            });
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(self.canvas()))
            .show(ctx, |ui| {
                let (rect, response) =
                    ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
                if self.view == View::Plan {
                    self.plan(ui, rect, response);
                } else {
                    self.three(ui, rect, response);
                }
            });
        self.settings_window(ctx);
        self.licenses_window(ctx);
        self.shape_window(ctx);
        self.import_window(ctx);
        self.unsaved_window(ctx);
        self.last_layout_micros = start.elapsed().as_micros() as u64;
    }
    fn style(&self, ctx: &egui::Context) {
        let mut style = egui::Style {
            visuals: if self.settings.theme == Theme::Dark {
                egui::Visuals::dark()
            } else {
                egui::Visuals::light()
            },
            ..Default::default()
        };
        style.visuals.panel_fill = self.surface();
        style.visuals.window_fill = self.surface();
        style.visuals.selection.bg_fill = self.accent();
        style.visuals.selection.stroke = Stroke::new(
            1.0,
            if self.settings.theme == Theme::Dark {
                Color32::from_rgb(10, 42, 34)
            } else {
                Color32::WHITE
            },
        );
        style.visuals.hyperlink_color = self.accent();
        style.visuals.text_cursor.blink = false;
        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        style.spacing.button_padding = egui::vec2(10.0, 7.0);
        style.spacing.interact_size.y = 30.0;
        style.animation_time = if self.settings.reduced_motion {
            0.0
        } else {
            0.08
        };
        if self.settings.reduced_motion {
            style.scroll_animation = egui::style::ScrollAnimation::none();
        }
        style
            .text_styles
            .insert(egui::TextStyle::Body, FontId::proportional(14.0));
        style
            .text_styles
            .insert(egui::TextStyle::Button, FontId::proportional(14.0));
        style
            .text_styles
            .insert(egui::TextStyle::Heading, FontId::proportional(19.0));
        ctx.set_style(style);
    }
    fn surface(&self) -> Color32 {
        if self.settings.theme == Theme::Dark {
            Color32::from_rgb(29, 40, 42)
        } else {
            Color32::from_rgb(251, 252, 247)
        }
    }
    fn canvas(&self) -> Color32 {
        if self.settings.theme == Theme::Dark {
            Color32::from_rgb(22, 31, 34)
        } else {
            Color32::from_rgb(236, 241, 234)
        }
    }
    fn accent(&self) -> Color32 {
        if self.settings.theme == Theme::Dark {
            Color32::from_rgb(108, 202, 178)
        } else {
            Color32::from_rgb(30, 112, 94)
        }
    }
    fn muted(&self) -> Color32 {
        if self.settings.theme == Theme::Dark {
            Color32::from_rgb(169, 188, 183)
        } else {
            Color32::from_rgb(90, 111, 104)
        }
    }
    fn warning(&self) -> Color32 {
        if self.settings.theme == Theme::Dark {
            Color32::from_rgb(246, 185, 117)
        } else {
            Color32::from_rgb(157, 76, 25)
        }
    }
    fn persist_settings(&mut self) {
        if let Err(e) = self.settings.save() {
            self.error("settings_error", e);
        }
    }
    fn keyboard(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.cancel_tool();
            self.pending = None;
        }
        if self.settings_open
            || self.pending.is_some()
            || self.import_open
            || self.shape_open
            || ctx.wants_keyboard_input()
        {
            return;
        }
        let (save, open, undo, redo, delete, finish) = ctx.input(|i| {
            (
                i.modifiers.command && i.key_pressed(Key::S),
                i.modifiers.command && i.key_pressed(Key::O),
                i.modifiers.command && i.key_pressed(Key::Z) && !i.modifiers.shift,
                i.modifiers.command
                    && (i.key_pressed(Key::Y) || (i.modifiers.shift && i.key_pressed(Key::Z))),
                i.key_pressed(Key::Delete),
                i.key_pressed(Key::Enter),
            )
        });
        if save {
            self.save(false);
        }
        if open {
            self.request(Pending::Open);
        }
        if undo {
            self.editor.undo();
        }
        if redo {
            self.editor.redo();
        }
        if delete {
            self.delete();
        }
        if finish {
            match self.tool {
                Tool::Draw => self.finish_drawing(),
                Tool::Furniture(kind) => {
                    let (min, max) = scene::bounds(&self.editor.document.room.vertices);
                    if let Err(e) = self.editor.add_furniture(kind, (min + max) / 2.0) {
                        self.error("invalid_geometry", e);
                    } else {
                        self.tool = Tool::Select;
                    }
                }
                Tool::Opening(kind) => {
                    if let Some(Selection::Wall(wall)) = self.editor.selection {
                        let (a, b) = self.editor.document.room.edge(wall);
                        let offset = (a.distance(b) / 2.0 - 0.6).max(0.05);
                        if let Err(e) = self.editor.add_opening(kind, wall, offset) {
                            self.error("invalid_geometry", e);
                        } else {
                            self.tool = Tool::Select;
                        }
                    }
                }
                Tool::Select => {}
            }
        }
    }
    fn delete(&mut self) {
        if let Err(e) = self.editor.delete_selection() {
            self.error(
                if e == ValidationError::Opening {
                    "delete_blocked"
                } else {
                    "invalid_geometry"
                },
                e,
            );
        }
    }
    fn cancel_tool(&mut self) {
        self.editor.cancel_gesture();
        self.drag = None;
        self.draw_points.clear();
        self.tool = Tool::Select;
    }
    fn finish_drawing(&mut self) {
        let mut vertices = self.draw_points.clone();
        if geometry::signed_area(&vertices) < 0.0 {
            vertices.reverse();
        }
        let result = self.editor.apply(|d| {
            d.room.vertices = vertices;
            d.openings.clear();
        });
        if result.is_ok() {
            self.draw_points.clear();
            self.tool = Tool::Select;
            self.editor.selection = None;
            self.fit = true;
        }
        self.result(result);
    }
    fn sync_draft(&mut self) {
        if self.draft_revision != self.editor.revision
            || self.draft_selection != self.editor.selection
        {
            self.draft = self.editor.document.clone();
            self.draft_revision = self.editor.revision;
            self.draft_selection = self.editor.selection;
            let (min, max) = scene::bounds(&self.draft.room.vertices);
            self.room_width = max.x - min.x;
            self.room_depth = max.y - min.y;
            if let Some(Selection::Wall(i)) = self.editor.selection {
                let (a, b) = self.draft.room.edge(i);
                self.wall_length = a.distance(b);
            }
        }
    }
    fn properties(&mut self, ui: &mut egui::Ui) {
        self.sync_draft();
        let language = self.settings.language;
        let t = |key| language.text(key);
        let units = self.draft.units;
        ui.heading(t("properties"));
        ui.label(t("selection"));
        let mut selected = self.editor.selection;
        let selection_text = selection_name(selected, &self.editor.document, language);
        egui::ComboBox::from_id_salt("selection")
            .width(ui.available_width())
            .selected_text(selection_text)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut selected, None, t("room"));
                for f in &self.editor.document.furniture {
                    ui.selectable_value(
                        &mut selected,
                        Some(Selection::Furniture(f.id)),
                        format!("{} #{}", t(furniture_key(f.kind)), f.id),
                    );
                }
                for o in &self.editor.document.openings {
                    ui.selectable_value(
                        &mut selected,
                        Some(Selection::Opening(o.id)),
                        format!("{} #{}", t(opening_key(o.kind)), o.id),
                    );
                }
                for i in 0..self.editor.document.room.vertices.len() {
                    ui.selectable_value(
                        &mut selected,
                        Some(Selection::Wall(i)),
                        format!("{} {}", t("wall"), i + 1),
                    );
                    ui.selectable_value(
                        &mut selected,
                        Some(Selection::Vertex(i)),
                        format!("{} {}", t("corner"), i + 1),
                    );
                }
            });
        if selected != self.editor.selection {
            self.editor.selection = selected;
            self.sync_draft();
        }
        ui.add_space(10.0);
        let selection = self.editor.selection;
        match selection {
            Some(Selection::Furniture(id)) => {
                if let Some(f) = self.draft.furniture.iter_mut().find(|f| f.id == id) {
                    ui.label(
                        egui::RichText::new(format!("{} #{}", t(furniture_key(f.kind)), id))
                            .strong(),
                    );
                    number(ui, t("x"), &mut f.center.x, units, -1000.0, 1000.0);
                    number(ui, t("y"), &mut f.center.y, units, -1000.0, 1000.0);
                    number(ui, t("width"), &mut f.width, units, 0.1, 20.0);
                    number(ui, t("depth"), &mut f.depth, units, 0.1, 20.0);
                    number(ui, t("height"), &mut f.height, units, 0.1, 10.0);
                    ui.horizontal(|ui| {
                        ui.label(t("angle"));
                        let mut degrees = f.angle.to_degrees();
                        if ui
                            .add(egui::DragValue::new(&mut degrees).suffix("°").speed(1.0))
                            .changed()
                        {
                            f.angle = degrees.to_radians();
                        }
                    });
                }
            }
            Some(Selection::Opening(id)) => {
                if let Some(o) = self.draft.openings.iter_mut().find(|o| o.id == id) {
                    ui.strong(format!("{} #{}", t(opening_key(o.kind)), id));
                    ui.label(format!("{} {}", t("wall"), o.wall + 1));
                    number(ui, t("offset"), &mut o.offset, units, 0.05, 1000.0);
                    number(ui, t("width"), &mut o.width, units, 0.1, 1000.0);
                    number(ui, t("height"), &mut o.height, units, 0.1, 10.0);
                    if o.kind == OpeningKind::Window {
                        number(ui, t("bottom"), &mut o.bottom, units, 0.0, 10.0);
                    }
                    if o.kind == OpeningKind::Door {
                        ui.checkbox(&mut o.hinge_at_end, t("hinge"));
                    }
                }
            }
            Some(Selection::Vertex(i)) => {
                ui.strong(format!("{} {}", t("corner"), i + 1));
                number(
                    ui,
                    t("x"),
                    &mut self.draft.room.vertices[i].x,
                    units,
                    -1000.0,
                    1000.0,
                );
                number(
                    ui,
                    t("y"),
                    &mut self.draft.room.vertices[i].y,
                    units,
                    -1000.0,
                    1000.0,
                );
            }
            Some(Selection::Wall(i)) => {
                ui.strong(format!("{} {}", t("wall"), i + 1));
                number(ui, t("width"), &mut self.wall_length, units, 0.1, 1000.0);
                number(
                    ui,
                    t("height"),
                    &mut self.draft.room.height,
                    units,
                    1.0,
                    10.0,
                );
                number(
                    ui,
                    t("thickness"),
                    &mut self.draft.room.thickness,
                    units,
                    0.05,
                    1.0,
                );
            }
            None => {
                ui.strong(t("room"));
                ui.label(t("title"));
                ui.add(
                    egui::TextEdit::singleline(&mut self.draft.title)
                        .char_limit(100)
                        .desired_width(ui.available_width()),
                );
                number(ui, t("width"), &mut self.room_width, units, 0.5, 1000.0);
                number(ui, t("depth"), &mut self.room_depth, units, 0.5, 1000.0);
                number(
                    ui,
                    t("height"),
                    &mut self.draft.room.height,
                    units,
                    1.0,
                    10.0,
                );
                number(
                    ui,
                    t("thickness"),
                    &mut self.draft.room.thickness,
                    units,
                    0.05,
                    1.0,
                );
            }
        }
        ui.add_space(8.0);
        if ui.button(t("apply")).clicked() {
            if selection.is_none() {
                let (min, max) = scene::bounds(&self.editor.document.room.vertices);
                for p in &mut self.draft.room.vertices {
                    p.x = min.x + (p.x - min.x) * self.room_width / (max.x - min.x);
                    p.y = min.y + (p.y - min.y) * self.room_depth / (max.y - min.y);
                }
            }
            if let Some(Selection::Wall(i)) = selection {
                let (a, b) = self.draft.room.edge(i);
                let j = (i + 1) % self.draft.room.vertices.len();
                self.draft.room.vertices[j] = a + (b - a) * (self.wall_length / a.distance(b));
            }
            let next = self.draft.clone();
            let result = self.editor.apply(|d| *d = next);
            self.result(result);
            self.draft_revision = u64::MAX;
        }
        if matches!(
            selection,
            Some(Selection::Furniture(_) | Selection::Opening(_) | Selection::Vertex(_))
        ) {
            ui.horizontal_wrapped(|ui| {
                if ui.button(t("delete")).clicked() {
                    self.delete();
                }
                if let Some(Selection::Furniture(id)) = selection
                    && ui.button(t("duplicate")).clicked()
                    && let Some(mut f) = self
                        .editor
                        .document
                        .furniture
                        .iter()
                        .find(|f| f.id == id)
                        .cloned()
                {
                    let id = self.editor.document.next_id;
                    f.id = id;
                    f.center = f.center + Point::new(0.2, 0.2);
                    let result = self.editor.apply(|d| {
                        d.allocate_id();
                        d.furniture.push(f);
                    });
                    self.result(result);
                    self.editor.selection = Some(Selection::Furniture(id));
                }
            });
        }
        ui.add_space(14.0);
        ui.separator();
        ui.add_space(8.0);
        let old = self.editor.document.units;
        let mut selected = old;
        egui::ComboBox::from_id_salt("units")
            .selected_text(t(units_key(old)))
            .show_ui(ui, |ui| {
                for units in [Units::Metres, Units::Centimetres, Units::Feet] {
                    ui.selectable_value(&mut selected, units, t(units_key(units)));
                }
            });
        if old != selected {
            let result = self.editor.apply(|d| d.units = selected);
            self.result(result);
        }
        let area = geometry::signed_area(&self.editor.document.room.vertices)
            * self.editor.document.units.factor().powi(2);
        ui.label(format!(
            "{}: {:.2} {}²",
            t("area"),
            area,
            self.editor.document.units.symbol()
        ));
        if let Some(Selection::Furniture(id)) = selection
            && let Some(f) = self.editor.document.furniture.iter().find(|f| f.id == id)
        {
            ui.label(format!(
                "{}: {}",
                t("clearance"),
                self.editor
                    .document
                    .units
                    .format(geometry::clearance(&self.editor.document, f))
            ));
        }
        ui.add_space(18.0);
        ui.strong(t("checks"));
        if self.check_cache.0 != self.editor.revision {
            self.check_cache = (
                self.editor.revision,
                geometry::checks(&self.editor.document),
            );
        }
        if self.check_cache.1.is_empty() {
            ui.colored_label(self.accent(), t("checks_ok"));
        } else {
            for issue in self.check_cache.1.clone() {
                let key = match issue.kind {
                    geometry::IssueKind::Overlap => "overlap",
                    geometry::IssueKind::Outside => "outside",
                    geometry::IssueKind::DoorSwing => "door_blocked",
                };
                if ui
                    .selectable_label(
                        selection == Some(Selection::Furniture(issue.object)),
                        format!(
                            "{} · #{}{}",
                            t(key),
                            issue.object,
                            issue
                                .other
                                .map(|id| format!(" / #{id}"))
                                .unwrap_or_default()
                        ),
                    )
                    .clicked()
                {
                    self.editor.selection = Some(Selection::Furniture(issue.object));
                }
            }
        }
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new(t("checks_note"))
                .small()
                .color(self.muted()),
        );
    }
    fn world(&self, p: Pos2, rect: Rect) -> Point {
        let origin = rect.center() + self.pan;
        Point::new(
            (p.x - origin.x) as f64 / self.zoom,
            -(p.y - origin.y) as f64 / self.zoom,
        )
    }
    fn screen(&self, p: Point, rect: Rect) -> Pos2 {
        rect.center() + self.pan + egui::vec2((p.x * self.zoom) as f32, (-p.y * self.zoom) as f32)
    }
    fn snapped(&self, p: Point, exclude: Option<Id>, disabled: bool) -> Point {
        if self.settings.snap && !disabled {
            geometry::snap(
                &self.editor.document,
                p,
                self.settings.grid,
                8.0 / self.zoom,
                exclude,
            )
        } else {
            p
        }
    }
    fn plan(&mut self, ui: &mut egui::Ui, rect: Rect, response: Response) {
        if self.fit {
            let (min, max) = scene::bounds(&geometry::outer_corners(&self.editor.document.room));
            let size = max - min;
            self.zoom = ((rect.width() as f64 - 100.0) / size.x)
                .min((rect.height() as f64 - 100.0) / size.y)
                .clamp(12.0, 800.0);
            let center = (min + max) / 2.0;
            self.pan = egui::vec2(
                (-center.x * self.zoom) as f32,
                (center.y * self.zoom) as f32,
            );
            self.fit = false;
        }
        if response.hovered() {
            let scroll = ui.input(|i| i.raw_scroll_delta.y);
            if scroll.abs() > 0.0
                && let Some(pos) = ui.input(|i| i.pointer.hover_pos())
            {
                let before = self.world(pos, rect);
                self.zoom = (self.zoom * (scroll as f64 * 0.0015).exp()).clamp(12.0, 1200.0);
                self.pan += pos - self.screen(before, rect);
            }
        }
        if response.dragged_by(PointerButton::Middle)
            || response.dragged_by(PointerButton::Secondary)
        {
            self.pan += ui.input(|i| i.pointer.delta());
        }
        let pointer = response.interact_pointer_pos();
        let disabled = ui.input(|i| i.modifiers.alt);
        if response.clicked_by(PointerButton::Primary)
            && let Some(pos) = pointer
        {
            let p = self.snapped(self.world(pos, rect), None, disabled);
            match self.tool {
                Tool::Draw => {
                    if self.draw_points.len() >= 3
                        && self.screen(self.draw_points[0], rect).distance(pos) < 12.0
                    {
                        self.finish_drawing();
                    } else {
                        self.draw_points.push(p);
                    }
                }
                Tool::Furniture(kind) => match self.editor.add_furniture(kind, p) {
                    Ok(_) => self.tool = Tool::Select,
                    Err(e) => self.error("invalid_geometry", e),
                },
                Tool::Opening(kind) => {
                    if let Some((wall, distance, t)) = self.nearest_wall(self.world(pos, rect))
                        && distance * self.zoom < 20.0
                    {
                        let (a, b) = self.editor.document.room.edge(wall);
                        let width = if kind == OpeningKind::Window {
                            1.2
                        } else {
                            0.9
                        };
                        let offset = (a.distance(b) * t - width / 2.0).max(0.05);
                        match self.editor.add_opening(kind, wall, offset) {
                            Ok(_) => self.tool = Tool::Select,
                            Err(e) => self.error("invalid_geometry", e),
                        }
                    }
                }
                Tool::Select => {
                    self.editor.selection = self.hit(pos, rect);
                }
            }
        }
        if response.double_clicked()
            && self.tool == Tool::Select
            && let Some(pos) = pointer
            && let Some((wall, distance, t)) = self.nearest_wall(self.world(pos, rect))
            && distance * self.zoom < 12.0
        {
            let (a, b) = self.editor.document.room.edge(wall);
            if let Err(e) = self.editor.insert_vertex(wall, a + (b - a) * t) {
                self.error("invalid_geometry", e);
            }
        }
        if response.drag_started_by(PointerButton::Primary)
            && self.tool == Tool::Select
            && let Some(pos) = ui.input(|i| i.pointer.press_origin())
        {
            self.start_drag(pos, rect);
        }
        if response.dragged_by(PointerButton::Primary)
            && let Some(pos) = pointer
        {
            self.drag_to(self.world(pos, rect), disabled);
        }
        if response.drag_stopped_by(PointerButton::Primary) {
            self.editor.finish_gesture();
            self.drag = None;
        }
        let painter = ui.painter_at(rect);
        self.paint_grid(&painter, rect);
        let d = &self.editor.document;
        let poly: Vec<_> = d
            .room
            .vertices
            .iter()
            .map(|&p| self.screen(p, rect))
            .collect();
        for triangle in geometry::triangulate(&d.room.vertices) {
            painter.add(egui::Shape::convex_polygon(
                triangle.map(|i| poly[i]).to_vec(),
                if self.settings.theme == Theme::Dark {
                    Color32::from_rgb(46, 58, 57)
                } else {
                    Color32::from_rgb(251, 249, 239)
                },
                Stroke::NONE,
            ));
        }
        for i in 0..d.room.vertices.len() {
            let (a, b) = d.room.edge(i);
            let len = a.distance(b);
            let selected = self.editor.selection == Some(Selection::Wall(i));
            let points = geometry::wall_quad(&d.room, i, 0.0, len).map(|p| self.screen(p, rect));
            painter.add(egui::Shape::convex_polygon(
                points.to_vec(),
                if selected {
                    self.accent()
                } else {
                    self.muted()
                },
                Stroke::NONE,
            ));
            let u = (b - a) / len;
            let outward = Point::new(u.y, -u.x);
            let from = self.screen(a + outward * (d.room.thickness + 0.22), rect);
            let to = self.screen(b + outward * (d.room.thickness + 0.22), rect);
            painter.line_segment([from, to], Stroke::new(1.0, self.muted()));
            for p in [from, to] {
                painter.circle_filled(p, 2.0, self.muted());
            }
            self.label(
                &painter,
                from.lerp(to, 0.5),
                &d.units.format(len),
                12.0,
                self.muted(),
            );
        }
        for o in &d.openings {
            let quad = geometry::wall_quad(&d.room, o.wall, o.offset, o.offset + o.width)
                .map(|p| self.screen(p, rect));
            let selected = self.editor.selection == Some(Selection::Opening(o.id));
            painter.add(egui::Shape::convex_polygon(
                quad.to_vec(),
                if o.kind == OpeningKind::Window {
                    Color32::from_rgb(121, 186, 194)
                } else {
                    self.canvas()
                },
                Stroke::new(
                    if selected { 2.0 } else { 1.0 },
                    if selected {
                        self.accent()
                    } else {
                        self.muted()
                    },
                ),
            ));
            if o.kind == OpeningKind::Door {
                let swing = geometry::door_swing(&d.room, o);
                let mut path: Vec<_> = swing.iter().map(|&p| self.screen(p, rect)).collect();
                path.push(path[0]);
                painter.add(egui::Shape::line(path, Stroke::new(1.3, self.warning())));
            }
        }
        for f in &d.furniture {
            let selected = self.editor.selection == Some(Selection::Furniture(f.id));
            let bad = self
                .check_cache
                .1
                .iter()
                .any(|i| i.object == f.id || i.other == Some(f.id));
            let corners = f.corners().map(|p| self.screen(p, rect));
            let fill = if self.settings.theme == Theme::Dark {
                Color32::from_rgb(57, 101, 90)
            } else {
                Color32::from_rgb(183, 210, 191)
            };
            painter.add(egui::Shape::convex_polygon(
                corners.to_vec(),
                fill,
                Stroke::new(
                    if selected { 2.5 } else { 1.2 },
                    if bad { self.warning() } else { self.accent() },
                ),
            ));
            self.paint_furniture(&painter, f, rect);
            if self.zoom * f.width > 65.0 {
                self.label(
                    &painter,
                    self.screen(f.center, rect),
                    self.t(furniture_key(f.kind)),
                    12.0,
                    if self.settings.theme == Theme::Dark {
                        Color32::WHITE
                    } else {
                        Color32::from_rgb(25, 67, 55)
                    },
                );
            }
            if selected {
                for p in corners {
                    painter.rect_filled(
                        Rect::from_center_size(p, egui::vec2(8.0, 8.0)),
                        2.0,
                        self.accent(),
                    );
                }
                let handle = self.rotation_handle(f, rect);
                let start = self.screen(
                    f.center + Point::new(0.0, -f.depth / 2.0).rotate(f.angle),
                    rect,
                );
                painter.line_segment([start, handle], Stroke::new(1.2, self.accent()));
                painter.circle_filled(handle, 5.5, self.accent());
            }
        }
        for (i, &p) in d.room.vertices.iter().enumerate() {
            painter.circle_filled(
                self.screen(p, rect),
                if self.editor.selection == Some(Selection::Vertex(i)) {
                    6.0
                } else {
                    3.5
                },
                self.accent(),
            );
        }
        if !self.draw_points.is_empty() {
            let mut path: Vec<_> = self
                .draw_points
                .iter()
                .map(|&p| self.screen(p, rect))
                .collect();
            if let Some(pos) = response.hover_pos() {
                path.push(self.screen(self.snapped(self.world(pos, rect), None, disabled), rect));
            }
            painter.add(egui::Shape::line(path, Stroke::new(2.0, self.accent())));
            for &p in &self.draw_points {
                painter.circle_filled(self.screen(p, rect), 5.0, self.accent());
            }
        }
    }
    fn label(&self, painter: &egui::Painter, p: Pos2, text: &str, size: f32, color: Color32) {
        let galley = painter.layout_no_wrap(text.to_owned(), FontId::proportional(size), color);
        let r = Rect::from_center_size(p, galley.size() + egui::vec2(8.0, 3.0));
        painter.rect_filled(r, 3.0, self.canvas());
        painter.galley(p - galley.size() / 2.0, galley, color);
    }
    fn paint_grid(&self, painter: &egui::Painter, rect: Rect) {
        let step = [
            0.01, 0.02, 0.05, 0.1, 0.2, 0.5, 1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0,
        ]
        .into_iter()
        .find(|s| s * self.zoom >= 28.0)
        .unwrap_or(100.0);
        let min = self.world(rect.left_bottom(), rect);
        let max = self.world(rect.right_top(), rect);
        let color = self.muted().gamma_multiply(0.2);
        let x0 = (min.x / step).floor() as i32;
        let x1 = (max.x / step).ceil() as i32;
        let y0 = (min.y / step).floor() as i32;
        let y1 = (max.y / step).ceil() as i32;
        for i in x0..=x1 {
            let x = self.screen(Point::new(i as f64 * step, 0.0), rect).x;
            painter.line_segment(
                [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                Stroke::new(0.7, color),
            );
            if i % 2 == 0 {
                painter.text(
                    egui::pos2(x + 3.0, rect.top() + 5.0),
                    Align2::LEFT_TOP,
                    self.editor.document.units.format(i as f64 * step),
                    FontId::proportional(10.0),
                    self.muted(),
                );
            }
        }
        for i in y0..=y1 {
            let y = self.screen(Point::new(0.0, i as f64 * step), rect).y;
            painter.line_segment(
                [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
                Stroke::new(0.7, color),
            );
            if i % 2 == 0 {
                painter.text(
                    egui::pos2(rect.left() + 5.0, y + 2.0),
                    Align2::LEFT_TOP,
                    self.editor.document.units.format(i as f64 * step),
                    FontId::proportional(10.0),
                    self.muted(),
                );
            }
        }
    }
    fn paint_furniture(&self, painter: &egui::Painter, f: &Furniture, rect: Rect) {
        let line = |a: Point, b: Point| {
            painter.line_segment(
                [
                    self.screen(f.center + a.rotate(f.angle), rect),
                    self.screen(f.center + b.rotate(f.angle), rect),
                ],
                Stroke::new(1.0, self.accent().gamma_multiply(0.7)),
            );
        };
        match f.kind {
            FurnitureKind::Bed => {
                line(
                    Point::new(-f.width * 0.45, -f.depth * 0.2),
                    Point::new(f.width * 0.45, -f.depth * 0.2),
                );
                line(
                    Point::new(0.0, -f.depth * 0.46),
                    Point::new(0.0, -f.depth * 0.2),
                );
            }
            FurnitureKind::Sofa => {
                line(
                    Point::new(-f.width * 0.4, f.depth * 0.3),
                    Point::new(f.width * 0.4, f.depth * 0.3),
                );
                for x in [-0.4, 0.4] {
                    line(
                        Point::new(x * f.width, -f.depth * 0.5),
                        Point::new(x * f.width, f.depth * 0.5),
                    );
                }
            }
            FurnitureKind::Cabinet => line(
                Point::new(0.0, -f.depth * 0.5),
                Point::new(0.0, f.depth * 0.5),
            ),
            FurnitureKind::Desk => {
                for x in [-0.4, 0.4] {
                    for y in [-0.32, 0.32] {
                        let p = self.screen(
                            f.center + Point::new(x * f.width, y * f.depth).rotate(f.angle),
                            rect,
                        );
                        painter.circle_filled(p, 2.5, self.accent());
                    }
                }
            }
        }
    }
    fn rotation_handle(&self, f: &Furniture, rect: Rect) -> Pos2 {
        self.screen(
            f.center + Point::new(0.0, -f.depth / 2.0 - 26.0 / self.zoom).rotate(f.angle),
            rect,
        )
    }
    fn nearest_wall(&self, p: Point) -> Option<(usize, f64, f64)> {
        (0..self.editor.document.room.vertices.len())
            .map(|i| {
                let (a, b) = self.editor.document.room.edge(i);
                let (distance, t) = geometry::distance_segment(p, a, b);
                (i, distance, t)
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
    }
    fn hit(&self, pos: Pos2, rect: Rect) -> Option<Selection> {
        let d = &self.editor.document;
        let p = self.world(pos, rect);
        if let Some(i) = d
            .room
            .vertices
            .iter()
            .position(|&p| self.screen(p, rect).distance(pos) < 9.0)
        {
            return Some(Selection::Vertex(i));
        }
        for f in d.furniture.iter().rev() {
            if f.contains(p) {
                return Some(Selection::Furniture(f.id));
            }
        }
        if let Some((wall, distance, t)) = self.nearest_wall(p)
            && distance * self.zoom < 16.0
        {
            let (a, b) = d.room.edge(wall);
            let offset = a.distance(b) * t;
            for o in &d.openings {
                if o.wall == wall && offset >= o.offset && offset <= o.offset + o.width {
                    return Some(Selection::Opening(o.id));
                }
            }
            return Some(Selection::Wall(wall));
        }
        None
    }
    fn start_drag(&mut self, pos: Pos2, rect: Rect) {
        if let Some(Selection::Furniture(id)) = self.editor.selection
            && let Some(f) = self
                .editor
                .document
                .furniture
                .iter()
                .find(|f| f.id == id)
                .cloned()
        {
            if self.rotation_handle(&f, rect).distance(pos) < 11.0 {
                self.drag = Some(Drag::Rotate { original: f });
            } else if let Some(i) = f
                .corners()
                .iter()
                .position(|&p| self.screen(p, rect).distance(pos) < 11.0)
            {
                self.drag = Some(Drag::Resize {
                    original: f,
                    sign: [
                        Point::new(-1.0, -1.0),
                        Point::new(1.0, -1.0),
                        Point::new(1.0, 1.0),
                        Point::new(-1.0, 1.0),
                    ][i],
                });
            }
        }
        if self.drag.is_none() {
            self.editor.selection = self.hit(pos, rect);
            let p = self.world(pos, rect);
            self.drag = match self.editor.selection {
                Some(Selection::Furniture(id)) => self
                    .editor
                    .document
                    .furniture
                    .iter()
                    .find(|f| f.id == id)
                    .cloned()
                    .map(|original| Drag::Move { original, start: p }),
                Some(Selection::Vertex(index)) => Some(Drag::Vertex { index }),
                Some(Selection::Opening(id)) => self
                    .editor
                    .document
                    .openings
                    .iter()
                    .find(|o| o.id == id)
                    .cloned()
                    .map(|original| Drag::Opening { original, start: p }),
                _ => None,
            };
        }
        if self.drag.is_some() {
            self.editor.begin_gesture();
        }
    }
    fn drag_to(&mut self, p: Point, disabled: bool) {
        let Some(drag) = self.drag.clone() else {
            return;
        };
        let result = match drag {
            Drag::Move {
                mut original,
                start,
            } => {
                let center = original.center + p - start;
                original.center = if self.settings.snap && !disabled {
                    geometry::snap_placement(
                        &self.editor.document,
                        &original,
                        center,
                        self.settings.grid,
                        8.0 / self.zoom,
                    )
                } else {
                    center
                };
                self.editor.apply(|d| {
                    if let Some(f) = d.furniture.iter_mut().find(|f| f.id == original.id) {
                        *f = original;
                    }
                })
            }
            Drag::Resize { mut original, sign } => {
                let point = self.snapped(p, Some(original.id), disabled);
                let local = original.local(point);
                let anchor = Point::new(
                    -sign.x * original.width / 2.0,
                    -sign.y * original.depth / 2.0,
                );
                let width = ((local.x - anchor.x) * sign.x).clamp(0.1, 20.0);
                let depth = ((local.y - anchor.y) * sign.y).clamp(0.1, 20.0);
                let corner = anchor + Point::new(sign.x * width, sign.y * depth);
                original.center =
                    original.center + ((anchor + corner) / 2.0).rotate(original.angle);
                original.width = width;
                original.depth = depth;
                self.editor.apply(|d| {
                    if let Some(f) = d.furniture.iter_mut().find(|f| f.id == original.id) {
                        *f = original;
                    }
                })
            }
            Drag::Rotate { mut original } => {
                let delta = p - original.center;
                let mut angle = delta.y.atan2(delta.x) + std::f64::consts::FRAC_PI_2;
                if self.settings.snap && !disabled {
                    angle = (angle.to_degrees() / 15.0).round() * 15.0_f64.to_radians();
                }
                original.angle = angle;
                self.editor.apply(|d| {
                    if let Some(f) = d.furniture.iter_mut().find(|f| f.id == original.id) {
                        *f = original;
                    }
                })
            }
            Drag::Vertex { index } => {
                let point = self.snapped(p, None, disabled);
                self.editor.apply(|d| d.room.vertices[index] = point)
            }
            Drag::Opening {
                mut original,
                start,
            } => {
                let (a, b) = self.editor.document.room.edge(original.wall);
                let u = (b - a) / a.distance(b);
                let delta = (p - start).dot(u);
                original.offset += delta;
                if self.settings.snap && !disabled {
                    original.offset =
                        (original.offset / self.settings.grid).round() * self.settings.grid;
                }
                self.editor.apply(|d| {
                    if let Some(o) = d.openings.iter_mut().find(|o| o.id == original.id) {
                        *o = original;
                    }
                })
            }
        };
        // During a corner drag retain the last valid geometry; the gesture remains cancellable.
        if let Err(e) = result {
            self.error("invalid_geometry", e);
        }
    }
    fn three(&mut self, ui: &mut egui::Ui, rect: Rect, response: Response) {
        let language = self.settings.language;
        let t = |key| language.text(key);
        if response.dragged_by(PointerButton::Primary) {
            let delta = ui.input(|i| i.pointer.delta());
            self.camera.yaw -= delta.x as f64 * 0.009;
            self.camera.elevation =
                (self.camera.elevation + delta.y as f64 * 0.007).clamp(0.12, 1.45);
        }
        if response.hovered() {
            let scroll = ui.input(|i| i.raw_scroll_delta.y);
            if scroll.abs() > 0.0 {
                self.camera.scale =
                    (self.camera.scale * (scroll as f64 * 0.0015).exp()).clamp(0.3, 4.0);
            }
        }
        let size = [
            rect.width().clamp(1.0, 1600.0) as usize,
            rect.height().clamp(1.0, 1200.0) as usize,
        ];
        let selected = match self.editor.selection {
            Some(Selection::Furniture(id) | Selection::Opening(id)) => Some(id),
            _ => None,
        };
        let dark = self.settings.theme == Theme::Dark;
        let cache_matches = self.image_cache.as_ref().is_some_and(|cache| {
            cache.revision == self.editor.revision
                && cache.camera == self.camera
                && cache.size == size
                && cache.dark == dark
                && cache.selected == selected
        });
        if !cache_matches {
            if self
                .scene_cache
                .as_ref()
                .is_none_or(|(revision, _)| *revision != self.editor.revision)
            {
                self.scene_cache = Some((
                    self.editor.revision,
                    Scene::from_document(&self.editor.document),
                ));
            }
            let image = scene::render(
                &self.scene_cache.as_ref().unwrap().1,
                &self.editor.document,
                self.camera,
                size[0],
                size[1],
                dark,
                selected,
            );
            let handle = if let Some(PreviewCache { mut texture, .. }) = self.image_cache.take() {
                texture.set(image, egui::TextureOptions::LINEAR);
                texture
            } else {
                ui.ctx()
                    .load_texture("room-3d", image, egui::TextureOptions::LINEAR)
            };
            self.image_cache = Some(PreviewCache {
                revision: self.editor.revision,
                camera: self.camera,
                size,
                dark,
                selected,
                texture: handle,
            });
        }
        if let Some(PreviewCache { texture, .. }) = &self.image_cache {
            ui.painter_at(rect).image(
                texture.id(),
                rect,
                Rect::from_min_max(Pos2::ZERO, egui::pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        let overlay = Rect::from_min_size(
            rect.min + egui::vec2(14.0, 12.0),
            egui::vec2(rect.width() - 28.0, 32.0),
        );
        ui.scope_builder(egui::UiBuilder::new().max_rect(overlay), |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.checkbox(&mut self.camera.cutaway, t("cutaway"));
                if ui.button(t("reset_camera")).clicked() {
                    self.camera = Camera::fit(&self.editor.document);
                }
            });
        });
    }
    fn settings_window(&mut self, ctx: &egui::Context) {
        if !self.settings_open {
            return;
        }
        let old = self.settings.clone();
        let language = self.settings.language;
        let t = |key| language.text(key);
        let mut open = true;
        egui::Window::new(t("settings"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(440.0)
            .show(ctx, |ui| {
                ui.label(t("language"));
                egui::ComboBox::from_id_salt("language")
                    .selected_text(self.settings.language.name())
                    .show_ui(ui, |ui| {
                        for language in Language::ALL {
                            ui.selectable_value(
                                &mut self.settings.language,
                                language,
                                language.name(),
                            );
                        }
                    });
                ui.add_space(12.0);
                ui.strong(t("appearance"));
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.settings.theme, Theme::Light, t("light"));
                    ui.selectable_value(&mut self.settings.theme, Theme::Dark, t("dark"));
                });
                ui.checkbox(&mut self.settings.reduced_motion, t("reduced_motion"));
                number(
                    ui,
                    t("grid"),
                    &mut self.settings.grid,
                    Units::Metres,
                    0.01,
                    1.0,
                );
                ui.add_space(14.0);
                ui.separator();
                ui.add_space(10.0);
                ui.strong(t("updates"));
                ui.label(format!("{}: {}", t("version"), env!("CARGO_PKG_VERSION")));
                ui.checkbox(&mut self.settings.startup_updates, t("startup_updates"));
                if ui
                    .add_enabled(
                        !matches!(
                            self.update,
                            updates::State::Checking | updates::State::Downloading { .. }
                        ),
                        egui::Button::new(t("check_updates")),
                    )
                    .clicked()
                {
                    self.check_updates();
                }
                match self.update.clone() {
                    updates::State::Idle => {
                        ui.label(t("update_idle"));
                    }
                    updates::State::Checking => {
                        ui.label(t("checking"));
                    }
                    updates::State::Current => {
                        ui.label(t("up_to_date"));
                    }
                    updates::State::Available(available) => {
                        ui.colored_label(
                            self.accent(),
                            format!("{} · {}", t("update_available"), available.version),
                        );
                        ui.label(t("update_restart"));
                        if ui.button(t("download_install")).clicked() {
                            self.download_update(available);
                        }
                    }
                    updates::State::Downloading { received, total } => {
                        ui.label(t("downloading"));
                        ui.add(
                            egui::ProgressBar::new(received as f32 / total.max(1) as f32)
                                .show_percentage()
                                .animate(false),
                        );
                    }
                    updates::State::Ready(_) => {
                        ui.label(t("update_restart"));
                        if ui.button(t("restart_install")).clicked() {
                            self.request(Pending::Install);
                        }
                    }
                    updates::State::Failed { key, detail } => {
                        ui.colored_label(self.warning(), t(key));
                        ui.collapsing(t("details"), |ui| {
                            ui.label(detail);
                        });
                    }
                }
                ui.add_space(12.0);
                ui.small(t("shortcuts"));
                if ui.button(t("licenses")).clicked() {
                    self.licenses_open = true;
                }
                if ui.button(t("done")).clicked() {
                    self.settings_open = false;
                }
            });
        self.settings_open &= open;
        if old != self.settings {
            self.persist_settings();
        }
    }
    fn licenses_window(&mut self, ctx: &egui::Context) {
        if !self.licenses_open {
            return;
        }
        let mut open = true;
        egui::Window::new(self.t("licenses"))
            .open(&mut open)
            .default_size(egui::vec2(700.0, 450.0))
            .show(ctx, |ui| {
                egui::ScrollArea::both().show_rows(ui, 16.0, LICENSE_LINES.len(), |ui, range| {
                    for i in range {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(LICENSE_LINES[i]).monospace().size(12.0),
                            )
                            .wrap_mode(egui::TextWrapMode::Extend),
                        );
                    }
                });
            });
        self.licenses_open = open;
    }
    fn shape_window(&mut self, ctx: &egui::Context) {
        if !self.shape_open {
            return;
        }
        let language = self.settings.language;
        let t = |key| language.text(key);
        let mut open = true;
        egui::Window::new(t("new_room"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(380.0)
            .show(ctx, |ui| {
                ui.label(t("new_help"));
                ui.add_space(10.0);
                for (key, room) in [
                    ("rectangle", Room::rectangle(5.0, 4.0)),
                    ("lshape", Room::l_shape()),
                ] {
                    if ui.button(t(key)).clicked() {
                        let result = self.editor.apply(|d| {
                            d.room = room;
                            d.openings.clear();
                        });
                        self.result(result);
                        self.editor.selection = None;
                        self.fit = true;
                        self.shape_open = false;
                    }
                }
                if ui.button(t("import")).clicked() {
                    self.import_open = true;
                    self.shape_open = false;
                }
            });
        self.shape_open &= open;
    }
    fn import_window(&mut self, ctx: &egui::Context) {
        if !self.import_open {
            return;
        }
        let language = self.settings.language;
        let t = |key| language.text(key);
        let mut open = true;
        egui::Window::new(t("import"))
            .open(&mut open)
            .collapsible(false)
            .default_width(400.0)
            .show(ctx, |ui| {
                ui.label(t("coordinates"));
                ui.add(
                    egui::TextEdit::multiline(&mut self.import_text)
                        .desired_rows(9)
                        .desired_width(ui.available_width())
                        .char_limit(16 * 1024),
                );
                if ui.button(t("replace_room")).clicked() {
                    match parse_coordinates(&self.import_text) {
                        Ok(vertices) => {
                            let result = self.editor.apply(|d| {
                                d.room.vertices = vertices;
                                d.openings.clear();
                            });
                            if result.is_ok() {
                                self.import_open = false;
                                self.editor.selection = None;
                                self.fit = true;
                            }
                            self.result(result);
                        }
                        Err(e) => self.error("invalid_coordinates", e),
                    }
                }
            });
        self.import_open &= open;
    }
    fn unsaved_window(&mut self, ctx: &egui::Context) {
        if self.pending.is_none() {
            return;
        }
        let language = self.settings.language;
        let t = |key| language.text(key);
        egui::Modal::new(egui::Id::new("unsaved")).show(ctx, |ui| {
            ui.heading(t("unsaved"));
            ui.label(t("unsaved_text"));
            ui.horizontal(|ui| {
                if ui.button(t("save")).clicked() && self.save(false) {
                    let pending = self.pending.take().unwrap();
                    self.proceed(pending);
                }
                if ui.button(t("discard")).clicked() {
                    let pending = self.pending.take().unwrap();
                    self.proceed(pending);
                }
                if ui.button(t("cancel")).clicked() {
                    self.pending = None;
                }
            });
        });
    }
}
fn number(ui: &mut egui::Ui, label: &str, value: &mut f64, units: Units, min: f64, max: f64) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 3.0;
        ui.label(label);
        let mut display = units.display(*value);
        if ui
            .add_sized(
                [ui.available_width(), 28.0],
                egui::DragValue::new(&mut display)
                    .speed(0.01 * units.factor())
                    .range(units.display(min)..=units.display(max))
                    .max_decimals(3)
                    .suffix(format!(" {}", units.symbol())),
            )
            .changed()
        {
            *value = units.from_display(display);
        }
    });
}
fn selection_name(selection: Option<Selection>, d: &Document, language: Language) -> String {
    let t = |key| language.text(key);
    match selection {
        Some(Selection::Furniture(id)) => format!(
            "{} #{}",
            d.furniture
                .iter()
                .find(|f| f.id == id)
                .map(|f| t(furniture_key(f.kind)))
                .unwrap_or(t("selection")),
            id
        ),
        Some(Selection::Opening(id)) => format!(
            "{} #{}",
            d.openings
                .iter()
                .find(|o| o.id == id)
                .map(|o| t(opening_key(o.kind)))
                .unwrap_or(t("selection")),
            id
        ),
        Some(Selection::Wall(i)) => format!("{} {}", t("wall"), i + 1),
        Some(Selection::Vertex(i)) => format!("{} {}", t("corner"), i + 1),
        None => t("room").to_owned(),
    }
}
pub fn furniture_key(kind: FurnitureKind) -> &'static str {
    match kind {
        FurnitureKind::Desk => "desk",
        FurnitureKind::Bed => "bed",
        FurnitureKind::Sofa => "sofa",
        FurnitureKind::Cabinet => "cabinet",
    }
}
fn opening_key(kind: OpeningKind) -> &'static str {
    match kind {
        OpeningKind::Door => "door",
        OpeningKind::Window => "window",
        OpeningKind::Passage => "passage",
    }
}
fn units_key(units: Units) -> &'static str {
    match units {
        Units::Metres => "metres",
        Units::Centimetres => "centimetres",
        Units::Feet => "feet",
    }
}
pub fn parse_coordinates(text: &str) -> Result<Vec<Point>, String> {
    let mut points = vec![];
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let numbers: Result<Vec<f64>, _> = line.split_whitespace().map(str::parse).collect();
        let values = numbers.map_err(|_| "Coordinates must be numbers")?;
        if values.len() != 2 {
            return Err("Each line must contain X Y".into());
        }
        points.push(Point::new(values[0], values[1]));
        if points.len() > 128 {
            return Err("At most 128 corners are supported".into());
        }
    }
    if geometry::signed_area(&points) < 0.0 {
        points.reverse();
    }
    if !geometry::validate_polygon(&points) {
        return Err("The outline must be a simple polygon with positive area".into());
    }
    Ok(points)
}
