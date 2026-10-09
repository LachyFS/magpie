use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const MIN_ZOOM: f32 = 0.08;
pub const MAX_ZOOM: f32 = 6.0;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Camera {
    pub x: f32,
    pub y: f32,
    pub zoom: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            zoom: 1.0,
        }
    }
}

impl Camera {
    pub fn world(&self, screen: [f32; 2]) -> [f32; 2] {
        [
            (screen[0] - self.x) / self.zoom,
            (screen[1] - self.y) / self.zoom,
        ]
    }

    pub fn screen(&self, world: [f32; 2]) -> [f32; 2] {
        [world[0] * self.zoom + self.x, world[1] * self.zoom + self.y]
    }

    pub fn zoom_at(&mut self, screen: [f32; 2], zoom: f32) {
        let world = self.world(screen);
        self.zoom = zoom.clamp(MIN_ZOOM, MAX_ZOOM);
        self.x = screen[0] - world[0] * self.zoom;
        self.y = screen[1] - world[1] * self.zoom;
    }

    pub fn fit(&mut self, items: &[ImageItem], viewport: [f32; 2]) {
        let Some(first) = items.first() else {
            *self = Self::default();
            return;
        };
        let mut bounds = [
            first.x,
            first.y,
            first.x + first.width,
            first.y + first.height,
        ];
        for item in items.iter().skip(1) {
            bounds[0] = bounds[0].min(item.x);
            bounds[1] = bounds[1].min(item.y);
            bounds[2] = bounds[2].max(item.x + item.width);
            bounds[3] = bounds[3].max(item.y + item.height);
        }
        let available = [
            (viewport[0] - 160.0).max(100.0),
            (viewport[1] - 210.0).max(100.0),
        ];
        self.zoom = (available[0] / (bounds[2] - bounds[0]))
            .min(available[1] / (bounds[3] - bounds[1]))
            .clamp(MIN_ZOOM, 1.5);
        self.x = viewport[0] / 2.0 - (bounds[0] + bounds[2]) / 2.0 * self.zoom;
        self.y = viewport[1] / 2.0 - (bounds[1] + bounds[3]) / 2.0 * self.zoom;
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ImageItem {
    pub id: Uuid,
    pub asset: String,
    pub original: String,
    pub name: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl ImageItem {
    pub fn contains(&self, p: [f32; 2]) -> bool {
        p[0] >= self.x
            && p[0] <= self.x + self.width
            && p[1] >= self.y
            && p[1] <= self.y + self.height
    }

    pub fn resize(&mut self, initial: [f32; 2], delta: [f32; 2]) {
        let ratio = initial[1] / initial[0];
        let width = if delta[0].abs() >= delta[1].abs() {
            initial[0] + delta[0]
        } else {
            (initial[1] + delta[1]) / ratio
        };
        self.width = width.clamp(32.0, 12000.0);
        self.height = self.width * ratio;
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Board {
    pub id: Uuid,
    pub name: String,
    pub camera: Camera,
    pub images: Vec<ImageItem>,
}

impl Board {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            camera: Camera::default(),
            images: vec![],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Library {
    pub version: u32,
    pub active: Uuid,
    pub boards: Vec<Board>,
}

impl Default for Library {
    fn default() -> Self {
        let board = Board::new("Untitled board");
        Self {
            version: 1,
            active: board.id,
            boards: vec![board],
        }
    }
}

impl Library {
    pub fn board(&self) -> &Board {
        self.boards
            .iter()
            .find(|b| b.id == self.active)
            .expect("validated active board")
    }
    pub fn board_mut(&mut self) -> &mut Board {
        self.boards
            .iter_mut()
            .find(|b| b.id == self.active)
            .expect("validated active board")
    }

    pub fn add_board(&mut self) {
        let mut number = self.boards.len() + 1;
        while self
            .boards
            .iter()
            .any(|b| b.name == format!("Untitled {number}"))
        {
            number += 1;
        }
        let board = Board::new(format!("Untitled {number}"));
        self.active = board.id;
        self.boards.push(board);
    }

    pub fn delete_active(&mut self) {
        self.boards.retain(|b| b.id != self.active);
        if self.boards.is_empty() {
            self.boards.push(Board::new("Untitled board"));
        }
        self.active = self.boards.last().unwrap().id;
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.version == 1,
            "This library was created by a newer version of Magpie."
        );
        anyhow::ensure!(
            !self.boards.is_empty() && self.boards.iter().any(|b| b.id == self.active),
            "The library has no active board."
        );
        for board in &self.boards {
            anyhow::ensure!(
                board.camera.x.is_finite()
                    && board.camera.y.is_finite()
                    && (MIN_ZOOM..=MAX_ZOOM).contains(&board.camera.zoom),
                "Invalid canvas position."
            );
            for item in &board.images {
                anyhow::ensure!(
                    item.x.is_finite()
                        && item.y.is_finite()
                        && item.width.is_finite()
                        && item.height.is_finite()
                        && item.width > 0.0
                        && item.height > 0.0,
                    "Invalid image dimensions."
                );
                for file in [&item.asset, &item.original] {
                    anyhow::ensure!(
                        std::path::Path::new(file).components().count() == 1
                            && matches!(
                                std::path::Path::new(file).components().next(),
                                Some(std::path::Component::Normal(_))
                            ),
                        "Invalid image path."
                    );
                }
            }
        }
        Ok(())
    }
}

#[derive(Default)]
pub struct History {
    undo: Vec<Library>,
    redo: Vec<Library>,
}

impl History {
    pub fn checkpoint(&mut self, library: &Library) {
        self.undo.push(library.clone());
        if self.undo.len() > 60 {
            self.undo.remove(0);
        }
        self.redo.clear();
    }
    pub fn undo(&mut self, library: &mut Library) -> bool {
        if let Some(previous) = self.undo.pop() {
            self.redo.push(std::mem::replace(library, previous));
            true
        } else {
            false
        }
    }
    pub fn redo(&mut self, library: &mut Library) -> bool {
        if let Some(next) = self.redo.pop() {
            self.undo.push(std::mem::replace(library, next));
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn item() -> ImageItem {
        ImageItem {
            id: Uuid::new_v4(),
            asset: "preview.png".into(),
            original: "original.png".into(),
            name: "image".into(),
            x: -300.0,
            y: 500.0,
            width: 400.0,
            height: 200.0,
        }
    }
    #[test]
    fn zoom_keeps_the_point_under_the_cursor_fixed_even_at_limits() {
        let mut camera = Camera {
            x: -400.0,
            y: 700.0,
            zoom: 0.8,
        };
        let cursor = [750.0, 220.0];
        let world = camera.world(cursor);
        for zoom in [1.3, 0.0001, 100.0] {
            camera.zoom_at(cursor, zoom);
            let after = camera.world(cursor);
            assert!((world[0] - after[0]).abs() < 0.01 && (world[1] - after[1]).abs() < 0.01);
            assert!((MIN_ZOOM..=MAX_ZOOM).contains(&camera.zoom));
        }
    }
    #[test]
    fn fit_centers_content_with_negative_coordinates() {
        let item = item();
        let mut camera = Camera::default();
        camera.fit(std::slice::from_ref(&item), [1000.0, 800.0]);
        assert_eq!(
            camera.screen([item.x + item.width / 2.0, item.y + item.height / 2.0]),
            [500.0, 400.0]
        );
    }
    #[test]
    fn resizing_preserves_aspect_ratio_and_minimum_size() {
        let mut item = item();
        item.resize([400.0, 200.0], [-900.0, 0.0]);
        assert_eq!([item.width, item.height], [32.0, 16.0]);
        item.resize([400.0, 200.0], [0.0, 200.0]);
        assert_eq!([item.width, item.height], [800.0, 400.0]);
    }
    #[test]
    fn undo_restores_deleted_boards_and_images_and_new_edits_clear_redo() {
        let mut library = Library::default();
        library.board_mut().images.push(item());
        let original = library.clone();
        let mut history = History::default();
        history.checkpoint(&library);
        library.delete_active();
        library.validate().unwrap();
        assert!(history.undo(&mut library));
        assert_eq!(library, original);
        assert!(history.redo(&mut library));
        assert!(library.board().images.is_empty());
        history.undo(&mut library);
        history.checkpoint(&library);
        library.add_board();
        assert!(!history.redo(&mut library));
    }
}
