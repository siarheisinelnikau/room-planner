use crate::{geometry, model::*};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selection {
    Furniture(Id),
    Opening(Id),
    Vertex(usize),
    Wall(usize),
}

pub struct Editor {
    pub document: Document,
    pub selection: Option<Selection>,
    undo: Vec<Document>,
    redo: Vec<Document>,
    saved: Document,
    gesture: Option<Document>,
    pub revision: u64,
}
impl Editor {
    pub fn new(document: Document) -> Self {
        Self {
            saved: document.clone(),
            document,
            selection: None,
            undo: vec![],
            redo: vec![],
            gesture: None,
            revision: 0,
        }
    }
    pub fn dirty(&self) -> bool {
        self.document != self.saved
    }
    pub fn mark_saved(&mut self) {
        self.saved = self.document.clone();
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    pub fn replace(&mut self, document: Document) {
        *self = Self::new(document);
    }
    fn push_undo(&mut self, old: Document) {
        if self.undo.len() == 128 {
            self.undo.remove(0);
        }
        self.undo.push(old);
        self.redo.clear();
    }
    pub fn apply(&mut self, edit: impl FnOnce(&mut Document)) -> Result<bool, ValidationError> {
        let mut next = self.document.clone();
        edit(&mut next);
        geometry::validate(&next)?;
        if next == self.document {
            return Ok(false);
        }
        let old = std::mem::replace(&mut self.document, next);
        if self.gesture.is_none() {
            self.push_undo(old);
        }
        self.revision += 1;
        Ok(true)
    }
    pub fn begin_gesture(&mut self) {
        if self.gesture.is_none() {
            self.gesture = Some(self.document.clone());
        }
    }
    pub fn finish_gesture(&mut self) {
        if let Some(old) = self.gesture.take()
            && old != self.document
        {
            self.push_undo(old);
        }
    }
    pub fn cancel_gesture(&mut self) {
        if let Some(old) = self.gesture.take() {
            self.document = old;
            self.revision += 1;
        }
    }
    pub fn undo(&mut self) {
        self.cancel_gesture();
        if let Some(old) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.document, old));
            self.revision += 1;
            self.reconcile_selection();
        }
    }
    pub fn redo(&mut self) {
        self.cancel_gesture();
        if let Some(next) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.document, next));
            self.revision += 1;
            self.reconcile_selection();
        }
    }
    pub fn reconcile_selection(&mut self) {
        let valid = match self.selection {
            Some(Selection::Furniture(id)) => self.document.furniture.iter().any(|o| o.id == id),
            Some(Selection::Opening(id)) => self.document.openings.iter().any(|o| o.id == id),
            Some(Selection::Vertex(i) | Selection::Wall(i)) => {
                i < self.document.room.vertices.len()
            }
            None => true,
        };
        if !valid {
            self.selection = None;
        }
    }
    pub fn add_furniture(
        &mut self,
        kind: FurnitureKind,
        center: Point,
    ) -> Result<Id, ValidationError> {
        let id = self.document.next_id;
        self.apply(|d| {
            let id = d.allocate_id();
            d.furniture.push(Furniture::new(id, kind, center));
        })?;
        self.selection = Some(Selection::Furniture(id));
        Ok(id)
    }
    pub fn add_opening(
        &mut self,
        kind: OpeningKind,
        wall: usize,
        offset: f64,
    ) -> Result<Id, ValidationError> {
        let id = self.document.next_id;
        self.apply(|d| {
            let id = d.allocate_id();
            d.openings.push(Opening {
                id,
                wall,
                kind,
                offset,
                width: if kind == OpeningKind::Window {
                    1.2
                } else {
                    0.9
                },
                bottom: if kind == OpeningKind::Window {
                    0.9
                } else {
                    0.0
                },
                height: if kind == OpeningKind::Window {
                    1.2
                } else {
                    2.1
                },
                hinge_at_end: false,
            });
        })?;
        self.selection = Some(Selection::Opening(id));
        Ok(id)
    }
    pub fn delete_selection(&mut self) -> Result<(), ValidationError> {
        match self.selection {
            Some(Selection::Furniture(id)) => {
                self.apply(|d| d.furniture.retain(|f| f.id != id))?;
            }
            Some(Selection::Opening(id)) => {
                self.apply(|d| d.openings.retain(|o| o.id != id))?;
            }
            Some(Selection::Vertex(i)) => {
                let len = self.document.room.vertices.len();
                let prev = (i + len - 1) % len;
                if self
                    .document
                    .openings
                    .iter()
                    .any(|o| o.wall == i || o.wall == prev)
                {
                    return Err(ValidationError::Opening);
                }
                self.apply(|d| {
                    d.room.vertices.remove(i);
                    for o in &mut d.openings {
                        if o.wall > i {
                            o.wall -= 1;
                        }
                    }
                })?;
            }
            _ => return Ok(()),
        }
        self.selection = None;
        Ok(())
    }
    pub fn insert_vertex(&mut self, wall: usize, p: Point) -> Result<(), ValidationError> {
        if self.document.openings.iter().any(|o| o.wall == wall) {
            return Err(ValidationError::Opening);
        }
        self.apply(|d| {
            d.room.vertices.insert(wall + 1, p);
            for o in &mut d.openings {
                if o.wall > wall {
                    o.wall += 1;
                }
            }
        })?;
        self.selection = Some(Selection::Vertex(wall + 1));
        Ok(())
    }
}
