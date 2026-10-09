use crate::input::TextInput;
use gpui::{prelude::*, *};
use magpie::navigation::{CameraTransition, format_zoom, grid_spacing, parse_zoom, step_zoom};
use magpie::{
    model::{Camera, History, ImageItem, Library},
    storage::Storage,
};
use std::{
    collections::HashSet,
    path::PathBuf,
    time::{Duration, Instant},
};

fn coord(value: Pixels) -> f64 {
    f32::from(value) as f64
}
fn pixels(value: f64) -> Pixels {
    px(value as f32)
}
use uuid::Uuid;

const BG: u32 = 0x17191b;
const PANEL: u32 = 0x222426;
const BORDER: u32 = 0x343739;
const TEXT: u32 = 0xe9e9e3;
const MUTED: u32 = 0x909593;
const ACCENT: u32 = 0xc6d5b5;
const SIDEBAR_WIDTH: f64 = 240.0;

enum Gesture {
    Pan {
        start: [f64; 2],
        camera: Camera,
    },
    Move {
        start: [f64; 2],
        positions: Vec<(Uuid, f64, f64)>,
    },
    Resize {
        start: [f64; 2],
        id: Uuid,
        size: [f64; 2],
    },
    Marquee {
        start: [f64; 2],
        end: [f64; 2],
    },
}

pub struct Magpie {
    storage: Storage,
    library: Library,
    history: History,
    selected: HashSet<Uuid>,
    focus: FocusHandle,
    gesture: Option<Gesture>,
    before_gesture: Option<Library>,
    space: bool,
    hand: bool,
    boards_open: bool,
    help_open: bool,
    rename: Option<Entity<TextInput>>,
    zoom_input: Option<Entity<TextInput>>,
    zoom_error: bool,
    motion: Option<(Instant, CameraTransition)>,
    #[cfg(target_os = "macos")]
    _pinch_monitor: Option<crate::macos_gestures::MagnifyMonitor>,
    importing: bool,
    save_task: Option<Task<()>>,
    saving: bool,
    message: Option<String>,
    message_task: Option<Task<()>>,
    viewport: [f64; 2],
}

impl Magpie {
    pub fn new(
        storage: Storage,
        library: Library,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();
        focus.focus(window);
        cx.on_app_quit(|this, _| {
            if let Err(error) = this.storage.save(&this.library) {
                eprintln!("Couldn't save boards: {error:#}");
            }
            async {}
        })
        .detach();
        let view = cx.weak_entity();
        window.on_window_should_close(cx, move |_, cx| {
            view.update(cx, |this, cx| this.prepare_close(cx))
                .unwrap_or(true)
        });
        Self {
            storage,
            library,
            history: History::default(),
            selected: HashSet::new(),
            focus,
            gesture: None,
            before_gesture: None,
            space: false,
            hand: false,
            boards_open: true,
            help_open: false,
            rename: None,
            zoom_input: None,
            zoom_error: false,
            motion: None,
            #[cfg(target_os = "macos")]
            _pinch_monitor: crate::macos_gestures::MagnifyMonitor::install(window, cx),
            importing: false,
            save_task: None,
            saving: false,
            message: None,
            message_task: None,
            viewport: [1280.0, 820.0],
        }
    }

    fn prepare_close(&mut self, cx: &mut Context<Self>) -> bool {
        if self.importing {
            self.toast("Finishing your import. Close again in a moment.", cx);
            return false;
        }
        match self.storage.save(&self.library) {
            Ok(()) => true,
            Err(error) => {
                self.toast(format!("Couldn't save your boards: {error}"), cx);
                false
            }
        }
    }

    fn toast(&mut self, message: impl Into<String>, cx: &mut Context<Self>) {
        self.message = Some(message.into());
        self.message_task = Some(cx.spawn(async |this, cx| {
            cx.background_executor().timer(Duration::from_secs(5)).await;
            let _ = this.update(cx, |this, cx| {
                this.message = None;
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        self.saving = true;
        self.save_task = Some(cx.spawn(async |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(350))
                .await;
            let _ = this.update(cx, |this, cx| {
                let result = this.storage.save(&this.library);
                this.saving = false;
                if let Err(error) = result {
                    this.toast(format!("Couldn't save: {error}"), cx);
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn checkpoint(&mut self) {
        self.history.checkpoint(&self.library);
    }

    fn new_board(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.finish_gesture(cx);
        self.motion = None;
        self.zoom_input = None;
        self.checkpoint();
        self.library.add_board();
        self.selected.clear();
        self.rename = None;
        self.focus.focus(window);
        self.save(cx);
    }

    fn switch_board(&mut self, id: Uuid, window: &mut Window, cx: &mut Context<Self>) {
        self.finish_gesture(cx);
        self.motion = None;
        self.zoom_input = None;
        self.library.active = id;
        self.selected.clear();
        self.focus.focus(window);
        self.save(cx);
    }

    fn rename_board(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.zoom_input = None;
        let name = self.library.board().name.clone();
        let input = cx.new(|cx| TextInput::new(name, cx));
        input.focus_handle(cx).focus(window);
        self.rename = Some(input);
        cx.notify();
    }

    fn finish_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(input) = self.rename.take() {
            let name = input
                .read(cx)
                .content
                .trim()
                .chars()
                .take(100)
                .collect::<String>();
            if !name.is_empty() && name != self.library.board().name {
                self.checkpoint();
                self.library.board_mut().name = name;
                self.save(cx);
            }
        }
        self.focus.focus(window);
        cx.notify();
    }

    fn choose_images(&mut self, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Add to board".into()),
        });
        cx.spawn(async move |this, cx| match receiver.await {
            Ok(Ok(Some(paths))) => {
                let _ = this.update(cx, |this, cx| {
                    let center = [this.center()[0] - 180.0, this.center()[1] - 130.0];
                    this.import_paths(paths, center, cx);
                });
            }
            Ok(Err(error)) => {
                let _ = this.update(cx, |this, cx| {
                    this.toast(format!("Couldn't open the file picker: {error}"), cx)
                });
            }
            _ => {}
        })
        .detach();
    }

    pub fn import_paths(
        &mut self,
        paths: Vec<PathBuf>,
        position: [f64; 2],
        cx: &mut Context<Self>,
    ) {
        if self.importing {
            self.toast("Adding images… one moment", cx);
            return;
        }
        if paths.is_empty() {
            return;
        }
        self.importing = true;
        let storage = self.storage.clone();
        let board = self.library.active;
        let world = self.library.board().camera.world(position);
        let task = cx.background_executor().spawn(async move {
            let mut images = vec![];
            let mut errors = vec![];
            for path in paths {
                match storage.import_path(&path) {
                    Ok(image) => images.push(image),
                    Err(error) => errors.push(format!(
                        "{}: {error}",
                        path.file_name().unwrap_or_default().to_string_lossy()
                    )),
                }
            }
            (images, errors)
        });
        cx.spawn(async move |this, cx| {
            let (images, errors) = task.await;
            let _ = this.update(cx, |this, cx| {
                this.finish_import(board, world, images, errors, cx)
            });
        })
        .detach();
        cx.notify();
    }

    fn finish_import(
        &mut self,
        board: Uuid,
        position: [f64; 2],
        images: Vec<ImageItem>,
        errors: Vec<String>,
        cx: &mut Context<Self>,
    ) {
        self.importing = false;
        let count = images.len();
        if count > 0 && self.library.boards.iter().any(|b| b.id == board) {
            self.checkpoint();
            let target = self
                .library
                .boards
                .iter_mut()
                .find(|b| b.id == board)
                .unwrap();
            let mut x = position[0];
            let mut y = position[1];
            let mut row_height: f64 = 0.0;
            if self.library.active == board {
                self.selected.clear();
            }
            for (index, mut item) in images.into_iter().enumerate() {
                if index > 0 && index % 3 == 0 {
                    x = position[0];
                    y += row_height + 24.0;
                    row_height = 0.0;
                }
                item.x = x;
                item.y = y;
                x += item.width + 24.0;
                row_height = row_height.max(item.height);
                if self.library.active == board {
                    self.selected.insert(item.id);
                }
                target.images.push(item);
            }
            self.save(cx);
        }
        if let Some(error) = errors.first() {
            self.toast(
                format!("{count} added · {} skipped. {error}", errors.len()),
                cx,
            );
        } else if count > 0 {
            self.toast(
                format!(
                    "{count} {} added",
                    if count == 1 { "image" } else { "images" }
                ),
                cx,
            );
        }
        cx.notify();
    }

    fn paste(&mut self, cx: &mut Context<Self>) {
        if self.importing {
            return;
        }
        let Some(clipboard) = cx.read_from_clipboard() else {
            return;
        };
        let images = clipboard
            .into_entries()
            .filter_map(|entry| match entry {
                ClipboardEntry::Image(image) => Some(image.bytes.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        if images.is_empty() {
            self.toast("Copy an image, then paste it here", cx);
            return;
        }
        self.importing = true;
        let board = self.library.active;
        let world = self
            .library
            .board()
            .camera
            .world([self.center()[0] - 180.0, self.center()[1] - 130.0]);
        let storage = self.storage.clone();
        let task = cx.background_executor().spawn(async move {
            let mut result = vec![];
            let mut errors = vec![];
            for bytes in images {
                match storage.import_bytes(&bytes, "Pasted image") {
                    Ok(image) => result.push(image),
                    Err(e) => errors.push(e.to_string()),
                }
            }
            (result, errors)
        });
        cx.spawn(async move |this, cx| {
            let (images, errors) = task.await;
            let _ = this.update(cx, |this, cx| {
                this.finish_import(board, world, images, errors, cx)
            });
        })
        .detach();
        cx.notify();
    }

    fn delete_selection(&mut self, cx: &mut Context<Self>) {
        if self.selected.is_empty() {
            return;
        }
        self.checkpoint();
        self.library
            .board_mut()
            .images
            .retain(|item| !self.selected.contains(&item.id));
        self.selected.clear();
        self.save(cx);
    }

    fn duplicate(&mut self, cx: &mut Context<Self>) {
        if self.selected.is_empty() {
            return;
        }
        self.checkpoint();
        let images = self
            .library
            .board()
            .images
            .iter()
            .filter(|i| self.selected.contains(&i.id))
            .cloned()
            .collect::<Vec<_>>();
        self.selected.clear();
        for mut item in images {
            item.id = Uuid::new_v4();
            item.x += 28.0;
            item.y += 28.0;
            self.selected.insert(item.id);
            self.library.board_mut().images.push(item);
        }
        self.save(cx);
    }

    fn canvas_left(&self) -> f64 {
        if self.boards_open { SIDEBAR_WIDTH } else { 0.0 }
    }

    fn center(&self) -> [f64; 2] {
        [
            (self.viewport[0] + self.canvas_left()) / 2.0,
            self.viewport[1] / 2.0,
        ]
    }

    fn toggle_sidebar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.finish_gesture(cx);
        self.motion = None;
        self.zoom_input = None;
        self.boards_open = !self.boards_open;
        self.focus.focus(window);
        cx.notify();
    }

    fn target_zoom(&self) -> f64 {
        self.motion
            .map_or(self.library.board().camera.zoom, |(_, motion)| {
                motion.to.zoom
            })
    }

    fn animate_to(&mut self, to: Camera, anchor: [f64; 2], cx: &mut Context<Self>) {
        self.motion = Some((
            Instant::now(),
            CameraTransition {
                from: self.library.board().camera,
                to,
                anchor,
            },
        ));
        self.save(cx);
    }

    fn animate_frame(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some((started, transition)) = self.motion {
            let progress = started.elapsed().as_secs_f64() / 0.16;
            self.library.board_mut().camera = transition.sample(progress);
            if progress >= 1.0 {
                self.motion = None;
                self.save(cx);
            } else {
                window.request_animation_frame();
            }
        }
    }

    fn fit(&mut self, selection: bool, cx: &mut Context<Self>) {
        if self.gesture.is_some() {
            return;
        }
        let images: Vec<_> = self
            .library
            .board()
            .images
            .iter()
            .filter(|item| !selection || self.selected.contains(&item.id))
            .cloned()
            .collect();
        if selection && images.is_empty() {
            self.toast("Select an image to zoom to it", cx);
            return;
        }
        let mut camera = self.library.board().camera;
        camera.fit(
            &images,
            [self.viewport[0] - self.canvas_left(), self.viewport[1]],
        );
        camera.x += self.canvas_left();
        self.animate_to(camera, self.center(), cx);
    }

    fn zoom_at(&mut self, anchor: [f64; 2], zoom: f64, smooth: bool, cx: &mut Context<Self>) {
        if self.gesture.is_some() {
            return;
        }
        let mut camera = self.library.board().camera;
        camera.zoom_at(anchor, zoom);
        if smooth {
            self.animate_to(camera, anchor, cx);
        } else {
            self.motion = None;
            self.library.board_mut().camera = camera;
            self.save(cx);
        }
    }

    fn zoom_step(&mut self, zoom_in: bool, cx: &mut Context<Self>) {
        self.zoom_at(
            self.center(),
            step_zoom(self.target_zoom(), zoom_in),
            true,
            cx,
        );
    }

    fn close_zoom(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.zoom_input = None;
        self.zoom_error = false;
        self.focus.focus(window);
        cx.notify();
    }

    fn toggle_zoom(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.zoom_input.is_some() {
            self.close_zoom(window, cx);
        } else {
            let value = format_zoom(self.target_zoom());
            let input = cx.new(|cx| TextInput::with_placeholder(value, "Zoom percentage", cx));
            input.focus_handle(cx).focus(window);
            self.zoom_input = Some(input);
            self.zoom_error = false;
            self.help_open = false;
            cx.notify();
        }
    }

    fn apply_zoom(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let zoom = self
            .zoom_input
            .as_ref()
            .and_then(|input| parse_zoom(&input.read(cx).content));
        if let Some(zoom) = zoom {
            self.zoom_at(self.center(), zoom, true, cx);
            self.close_zoom(window, cx);
        } else {
            self.zoom_error = true;
            cx.notify();
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn pinch(
        &mut self,
        anchor: [f64; 2],
        magnification: f64,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.rename.is_some()
            || self.zoom_input.is_some()
            || self.gesture.is_some()
            || anchor[0] < self.canvas_left()
            || self.help_open
            || anchor[1] < 80.0
            || anchor[1] > self.viewport[1] - 80.0
        {
            return false;
        }
        let factor = 1.0 + magnification;
        if !factor.is_finite() || factor <= 0.0 {
            return false;
        }
        self.zoom_at(anchor, self.library.board().camera.zoom * factor, false, cx);
        true
    }

    fn mouse_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.focus.focus(window);
        self.zoom_input = None;
        self.motion = None;
        self.help_open = false;
        let p = [coord(event.position.x), coord(event.position.y)];
        let board = self.library.board();
        let camera = board.camera;
        if self.space || self.hand || event.button != MouseButton::Left {
            self.gesture = Some(Gesture::Pan { start: p, camera });
        } else {
            let world = camera.world(p);
            let handle = if self.selected.len() == 1 {
                board
                    .images
                    .iter()
                    .find(|i| self.selected.contains(&i.id))
                    .filter(|i| {
                        let corner = camera.screen([i.x + i.width, i.y + i.height]);
                        (corner[0] - p[0]).abs() < 10.0 && (corner[1] - p[1]).abs() < 10.0
                    })
            } else {
                None
            };
            if let Some(item) = handle {
                self.gesture = Some(Gesture::Resize {
                    start: p,
                    id: item.id,
                    size: [item.width, item.height],
                });
                self.before_gesture = Some(self.library.clone());
            } else if let Some(item) = board.images.iter().rev().find(|i| i.contains(world)) {
                let id = item.id;
                if event.modifiers.shift {
                    if !self.selected.insert(id) {
                        self.selected.remove(&id);
                    }
                } else if !self.selected.contains(&id) {
                    self.selected.clear();
                    self.selected.insert(id);
                }
                let positions = self
                    .library
                    .board()
                    .images
                    .iter()
                    .filter(|i| self.selected.contains(&i.id))
                    .map(|i| (i.id, i.x, i.y))
                    .collect();
                self.gesture = Some(Gesture::Move {
                    start: p,
                    positions,
                });
                self.before_gesture = Some(self.library.clone());
            } else if event.modifiers.shift {
                self.gesture = Some(Gesture::Marquee { start: p, end: p });
            } else {
                self.selected.clear();
                self.gesture = Some(Gesture::Pan { start: p, camera });
            }
        }
        cx.notify();
    }

    fn mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.gesture.is_none() {
            return;
        }
        if event.pressed_button.is_none() {
            self.finish_gesture(cx);
            return;
        }
        let p = [coord(event.position.x), coord(event.position.y)];
        let board = self.library.board_mut();
        match self.gesture.as_mut().unwrap() {
            Gesture::Pan { start, camera } => {
                board.camera.x = camera.x + p[0] - start[0];
                board.camera.y = camera.y + p[1] - start[1];
            }
            Gesture::Move { start, positions } => {
                let delta = [
                    (p[0] - start[0]) / board.camera.zoom,
                    (p[1] - start[1]) / board.camera.zoom,
                ];
                for (id, x, y) in positions {
                    if let Some(item) = board.images.iter_mut().find(|i| i.id == *id) {
                        item.x = *x + delta[0];
                        item.y = *y + delta[1];
                    }
                }
            }
            Gesture::Resize { start, id, size } => {
                let delta = [
                    (p[0] - start[0]) / board.camera.zoom,
                    (p[1] - start[1]) / board.camera.zoom,
                ];
                if let Some(item) = board.images.iter_mut().find(|i| i.id == *id) {
                    item.resize(*size, delta);
                }
            }
            Gesture::Marquee { end, .. } => *end = p,
        }
        cx.notify();
    }

    fn finish_gesture(&mut self, cx: &mut Context<Self>) {
        if let Some(Gesture::Marquee { start, end }) = &self.gesture {
            let board = self.library.board();
            let a = board.camera.world(*start);
            let b = board.camera.world(*end);
            for item in &board.images {
                if item.x < a[0].max(b[0])
                    && item.x + item.width > a[0].min(b[0])
                    && item.y < a[1].max(b[1])
                    && item.y + item.height > a[1].min(b[1])
                {
                    self.selected.insert(item.id);
                }
            }
        }
        if let Some(before) = self.before_gesture.take()
            && before != self.library
        {
            self.history.checkpoint(&before);
        }
        if self.gesture.take().is_some() {
            self.save(cx);
        }
    }

    fn scroll(&mut self, event: &ScrollWheelEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.rename.is_some() || self.zoom_input.is_some() || self.gesture.is_some() {
            return;
        }
        let delta = event.delta.pixel_delta(pixels(40.0));
        if event.modifiers.control || event.modifiers.platform {
            let anchor = [coord(event.position.x), coord(event.position.y)];
            // High-resolution trackpad input follows the fingers directly. Wheel
            // ticks ease towards a target, accumulating even between frames.
            let smooth = !event.delta.precise();
            let zoom = if smooth {
                self.target_zoom()
            } else {
                self.library.board().camera.zoom
            };
            let distance = if event.modifiers.shift && delta.y == pixels(0.0) {
                coord(delta.x)
            } else {
                coord(delta.y)
            };
            let sensitivity = if smooth { 0.0015 } else { 0.004 };
            let factor = (distance.clamp(-500.0, 500.0) * sensitivity).exp();
            self.zoom_at(anchor, zoom * factor, smooth, cx);
        } else {
            self.motion = None;
            let camera = &mut self.library.board_mut().camera;
            if event.modifiers.shift && !event.delta.precise() {
                // X11 already translates Shift+wheel into horizontal deltas.
                camera.x += if delta.y == pixels(0.0) {
                    coord(delta.x)
                } else {
                    coord(delta.y)
                };
            } else {
                camera.x += coord(delta.x);
                camera.y += coord(delta.y);
            }
            self.save(cx);
        }
        cx.stop_propagation();
    }

    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        if self.zoom_input.is_some() {
            match key {
                "enter" => self.apply_zoom(window, cx),
                "escape" => self.close_zoom(window, cx),
                _ => return,
            }
            cx.stop_propagation();
            return;
        }
        if self.rename.is_some() {
            match key {
                "enter" => self.finish_rename(window, cx),
                "escape" => {
                    self.rename = None;
                    self.focus.focus(window);
                    cx.notify();
                }
                _ => return,
            }
            cx.stop_propagation();
            return;
        }
        let command = event.keystroke.modifiers.control || event.keystroke.modifiers.platform;
        if command {
            match key {
                "=" | "+" => self.zoom_step(true, cx),
                "-" | "_" => self.zoom_step(false, cx),
                "0" => self.zoom_at(self.center(), 1.0, true, cx),
                "b" => self.toggle_sidebar(window, cx),
                "n" => self.new_board(window, cx),
                "o" | "i" => self.choose_images(cx),
                "v" => self.paste(cx),
                "a" => {
                    self.selected = self.library.board().images.iter().map(|i| i.id).collect();
                    cx.notify();
                }
                "d" => self.duplicate(cx),
                "z" | "y" => {
                    self.finish_gesture(cx);
                    self.motion = None;
                    if event.keystroke.modifiers.shift || key == "y" {
                        self.history.redo(&mut self.library);
                    } else {
                        self.history.undo(&mut self.library);
                    }
                    self.selected.clear();
                    self.save(cx);
                }
                "s" => self.save(cx),
                "q" => {
                    if self.prepare_close(cx) {
                        cx.quit();
                    }
                }
                _ => return,
            }
        } else {
            match key {
                "space" => {
                    self.space = true;
                    cx.notify();
                }
                "h" => {
                    self.hand = true;
                    cx.notify();
                }
                "v" => {
                    self.hand = false;
                    cx.notify();
                }
                "f" | "1" | "!" => self.fit(false, cx),
                "2" | "@" => self.fit(true, cx),
                "0" | ")" => self.zoom_at(self.center(), 1.0, true, cx),
                "=" | "+" => self.zoom_step(true, cx),
                "-" | "_" => self.zoom_step(false, cx),
                "backspace" | "delete" => self.delete_selection(cx),
                "f2" => self.rename_board(window, cx),
                "escape" => {
                    self.motion = None;
                    if let Some(before) = self.before_gesture.take() {
                        self.library = before;
                    }
                    self.gesture = None;
                    self.selected.clear();
                    self.help_open = false;
                    self.space = false;
                    cx.notify();
                }
                "?" | "/" => {
                    self.help_open = !self.help_open;
                    cx.notify();
                }
                _ => return,
            }
        }
        cx.stop_propagation();
    }

    fn button(id: &'static str, label: impl Into<SharedString>) -> Stateful<Div> {
        div()
            .id(id)
            .h(pixels(34.0))
            .px(pixels(12.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(pixels(7.0))
            .text_size(pixels(12.0))
            .text_color(rgb(TEXT))
            .cursor_pointer()
            .hover(move |s| {
                s.bg(rgb(if id == "save-name" {
                    0xd5e2c8
                } else {
                    0x323537
                }))
            })
            .child(label.into())
    }

    fn panel() -> Div {
        div()
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .bg(rgb(PANEL))
            .border_1()
            .border_color(rgb(BORDER))
            .rounded(pixels(12.0))
            .shadow_lg()
    }

    fn header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .absolute()
            .top(pixels(22.0))
            .left(pixels(self.canvas_left() + 24.0))
            .right(pixels(24.0))
            .h(pixels(36.0))
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(pixels(12.0))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .when(!self.boards_open, |header| {
                        header.child(
                            Self::button("show-sidebar", "☰")
                                .w(pixels(32.0))
                                .px_0()
                                .text_color(rgb(MUTED))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.toggle_sidebar(window, cx)
                                })),
                        )
                    })
                    .child(
                        div()
                            .max_w(pixels(320.0))
                            .text_ellipsis()
                            .text_size(pixels(13.0))
                            .text_color(rgb(MUTED))
                            .child(self.library.board().name.clone()),
                    ),
            )
            .when(self.saving || self.importing, |header| {
                header.child(Self::saving_spinner())
            })
    }

    fn saving_spinner() -> impl IntoElement {
        div().size(pixels(16.0)).flex_shrink_0().with_animation(
            "saving-spinner",
            Animation::new(Duration::from_millis(900)).repeat(),
            |spinner, progress| {
                spinner.child(
                    canvas(
                        move |_, _, _| progress,
                        |bounds, progress, window, _| {
                            let center = bounds.center();
                            let angle = progress * std::f32::consts::TAU;
                            let mut path = PathBuilder::stroke(px(1.5));
                            for step in 0..=32 {
                                let theta =
                                    angle + step as f32 / 32.0 * std::f32::consts::TAU * 0.75;
                                let point = point(
                                    center.x + px(theta.cos() * 6.0),
                                    center.y + px(theta.sin() * 6.0),
                                );
                                if step == 0 {
                                    path.move_to(point);
                                } else {
                                    path.line_to(point);
                                }
                            }
                            if let Ok(path) = path.build() {
                                window.paint_path(path, rgb(MUTED));
                            }
                        },
                    )
                    .size_full(),
                )
            },
        )
    }

    fn board_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("board-sidebar")
            .absolute()
            .left_0()
            .top_0()
            .bottom_0()
            .w(pixels(SIDEBAR_WIDTH))
            .bg(rgb(0x1d1f21))
            .border_r_1()
            .border_color(rgb(BORDER))
            .flex()
            .flex_col()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Middle, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .h(pixels(80.0))
                    .flex_shrink_0()
                    .px(pixels(20.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(pixels(24.0))
                            .font_weight(FontWeight::MEDIUM)
                            .child("magpie"),
                    )
                    .child(
                        Self::button("hide-sidebar", "‹")
                            .w(pixels(28.0))
                            .px_0()
                            .text_size(pixels(22.0))
                            .text_color(rgb(MUTED))
                            .on_click(
                                cx.listener(|this, _, window, cx| this.toggle_sidebar(window, cx)),
                            ),
                    ),
            )
            .child(
                div().px(pixels(16.0)).pb(pixels(24.0)).child(
                    Self::button("new-board", "+  New board")
                        .w_full()
                        .justify_start()
                        .bg(rgb(0x292d29))
                        .text_color(rgb(ACCENT))
                        .on_click(cx.listener(|this, _, window, cx| this.new_board(window, cx))),
                ),
            )
            .child(
                div()
                    .px(pixels(24.0))
                    .pb(pixels(10.0))
                    .text_size(pixels(10.0))
                    .text_color(rgb(MUTED))
                    .child("BOARDS"),
            )
            .child(
                div()
                    .id("board-list")
                    .flex_1()
                    .min_h_0()
                    .px(pixels(12.0))
                    .overflow_y_scroll()
                    .children(self.library.boards.iter().map(|board| {
                        let id = board.id;
                        let active = id == self.library.active;
                        div()
                            .id(SharedString::from(id.to_string()))
                            .h(pixels(42.0))
                            .mb(pixels(4.0))
                            .px(pixels(12.0))
                            .flex()
                            .items_center()
                            .gap(pixels(10.0))
                            .rounded(pixels(7.0))
                            .cursor_pointer()
                            .hover(|s| s.bg(rgb(0x2b2e2c)))
                            .when(active, |s| s.bg(rgb(0x30372d)).text_color(rgb(ACCENT)))
                            .child(
                                div()
                                    .size(pixels(7.0))
                                    .rounded(pixels(2.0))
                                    .flex_shrink_0()
                                    .border_1()
                                    .border_color(rgb(if active { ACCENT } else { MUTED }))
                                    .when(active, |s| s.bg(rgb(ACCENT))),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_ellipsis()
                                    .text_size(pixels(12.0))
                                    .child(board.name.clone()),
                            )
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.switch_board(id, window, cx)
                            }))
                    })),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .border_t_1()
                    .border_color(rgb(BORDER))
                    .p(pixels(12.0))
                    .flex()
                    .flex_col()
                    .gap(pixels(2.0))
                    .child(
                        Self::button("rename", "Rename board")
                            .justify_start()
                            .text_color(rgb(MUTED))
                            .on_click(
                                cx.listener(|this, _, window, cx| this.rename_board(window, cx)),
                            ),
                    )
                    .child(
                        Self::button("delete-board", "Delete board")
                            .justify_start()
                            .text_color(rgb(0xbc9691))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.checkpoint();
                                this.motion = None;
                                this.library.delete_active();
                                this.selected.clear();
                                this.save(cx);
                                this.toast("Board deleted · Ctrl/Cmd+Z to undo", cx);
                            })),
                    ),
            )
    }

    fn toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .absolute()
            .bottom(pixels(24.0))
            .left(pixels(self.canvas_left()))
            .right_0()
            .flex()
            .justify_center()
            .child(
                Self::panel()
                    .id("toolbar")
                    .flex()
                    .items_center()
                    .p(pixels(5.0))
                    .gap(pixels(3.0))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        Self::button("select-tool", "↖")
                            .text_size(pixels(18.0))
                            .when(!self.hand, |s| s.bg(rgb(0x363b32)).text_color(rgb(ACCENT)))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.hand = false;
                                cx.notify();
                            })),
                    )
                    .child(
                        Self::button("pan-tool", "Pan")
                            .when(self.hand, |s| s.bg(rgb(0x363b32)).text_color(rgb(ACCENT)))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.hand = true;
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .w(pixels(1.0))
                            .h(pixels(20.0))
                            .mx(pixels(4.0))
                            .bg(rgb(BORDER)),
                    )
                    .child(
                        Self::button("zoom-out", "−")
                            .text_size(pixels(18.0))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.zoom_step(false, cx);
                                this.close_zoom(window, cx);
                            })),
                    )
                    .child(
                        Self::button(
                            "zoom-menu",
                            format!("{}  ⌄", format_zoom(self.library.board().camera.zoom)),
                        )
                        .min_w(pixels(82.0))
                        .px(pixels(8.0))
                        .when(self.zoom_input.is_some(), |s| {
                            s.bg(rgb(0x363b32)).text_color(rgb(ACCENT))
                        })
                        .on_click(cx.listener(|this, _, window, cx| this.toggle_zoom(window, cx))),
                    )
                    .child(
                        Self::button("zoom-in", "+")
                            .text_size(pixels(18.0))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.zoom_step(true, cx);
                                this.close_zoom(window, cx);
                            })),
                    )
                    .child(
                        div()
                            .w(pixels(1.0))
                            .h(pixels(20.0))
                            .mx(pixels(4.0))
                            .bg(rgb(BORDER)),
                    )
                    .child(Self::button("fit", "Fit").on_click(cx.listener(
                        |this, _, window, cx| {
                            this.fit(false, cx);
                            this.close_zoom(window, cx);
                        },
                    ))),
            )
    }

    fn zoom_menu(&self, input: Entity<TextInput>, cx: &mut Context<Self>) -> impl IntoElement {
        let row = |id, label, shortcut: &'static str| {
            Self::button(id, label).w_full().justify_between().child(
                div()
                    .text_size(pixels(11.0))
                    .text_color(rgb(MUTED))
                    .child(shortcut),
            )
        };
        div()
            .absolute()
            .bottom(pixels(80.0))
            .left(pixels(self.canvas_left()))
            .right_0()
            .flex()
            .justify_center()
            .child(
                Self::panel()
                    .w(pixels(286.0))
                    .p(pixels(8.0))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .px(pixels(8.0))
                            .pt(pixels(6.0))
                            .pb(pixels(10.0))
                            .child(
                                div()
                                    .text_size(pixels(11.0))
                                    .text_color(rgb(MUTED))
                                    .mb(pixels(8.0))
                                    .child("Zoom to"),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(pixels(6.0))
                                    .child(div().flex_1().min_w_0().child(input))
                                    .child(
                                        Self::button("apply-zoom", "↵")
                                            .bg(rgb(0x363b32))
                                            .text_color(rgb(ACCENT))
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.apply_zoom(window, cx)
                                            })),
                                    ),
                            )
                            .when(self.zoom_error, |s| {
                                s.child(
                                    div()
                                        .mt(pixels(6.0))
                                        .text_size(pixels(11.0))
                                        .text_color(rgb(0xe6afa4))
                                        .child("Enter a percentage from 1e-7 to 1e11."),
                                )
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(pixels(4.0))
                            .px(pixels(4.0))
                            .pb(pixels(8.0))
                            .children(
                                [
                                    ("preset-25", "25%", 0.25),
                                    ("preset-50", "50%", 0.5),
                                    ("preset-100", "100%", 1.0),
                                    ("preset-200", "200%", 2.0),
                                ]
                                .map(|(id, label, zoom)| {
                                    Self::button(id, label)
                                        .flex_1()
                                        .px_0()
                                        .bg(rgb(0x292c2e))
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.zoom_at(this.center(), zoom, true, cx);
                                            this.close_zoom(window, cx);
                                        }))
                                }),
                            ),
                    )
                    .child(div().h(pixels(1.0)).bg(rgb(BORDER)).my(pixels(4.0)))
                    .child(row("menu-zoom-in", "Zoom in", "+").on_click(cx.listener(
                        |this, _, window, cx| {
                            this.zoom_step(true, cx);
                            this.close_zoom(window, cx);
                        },
                    )))
                    .child(row("menu-zoom-out", "Zoom out", "−").on_click(cx.listener(
                        |this, _, window, cx| {
                            this.zoom_step(false, cx);
                            this.close_zoom(window, cx);
                        },
                    )))
                    .child(
                        row("menu-fit-all", "Fit all images", "Shift 1").on_click(cx.listener(
                            |this, _, window, cx| {
                                this.fit(false, cx);
                                this.close_zoom(window, cx);
                            },
                        )),
                    )
                    .child(
                        row("menu-fit-selection", "Fit selection", "Shift 2")
                            .when(self.selected.is_empty(), |s| s.text_color(rgb(MUTED)))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.fit(true, cx);
                                this.close_zoom(window, cx);
                            })),
                    )
                    .child(row("menu-actual-size", "Zoom to 100%", "Shift 0").on_click(
                        cx.listener(|this, _, window, cx| {
                            this.zoom_at(this.center(), 1.0, true, cx);
                            this.close_zoom(window, cx);
                        }),
                    )),
            )
    }

    fn empty(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .absolute()
            .left(pixels(self.canvas_left()))
            .right_0()
            .top_0()
            .bottom_0()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(pixels(18.0))
            .child(
                div()
                    .relative()
                    .w(pixels(108.0))
                    .h(pixels(80.0))
                    .mb(pixels(12.0))
                    .child(
                        div()
                            .absolute()
                            .left(pixels(3.0))
                            .top(pixels(13.0))
                            .w(pixels(52.0))
                            .h(pixels(60.0))
                            .bg(rgb(0x212528))
                            .border_1()
                            .border_color(rgb(0x3a4143))
                            .rounded(pixels(6.0)),
                    )
                    .child(
                        div()
                            .absolute()
                            .left(pixels(46.0))
                            .top_0()
                            .w(pixels(58.0))
                            .h(pixels(72.0))
                            .bg(rgb(0x2d332c))
                            .border_1()
                            .border_color(rgb(0x555f50))
                            .rounded(pixels(6.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_color(rgb(ACCENT))
                            .text_size(pixels(26.0))
                            .child("+"),
                    ),
            )
            .child(
                div()
                    .text_size(pixels(25.0))
                    .font_weight(FontWeight::MEDIUM)
                    .child("A little space for your ideas."),
            )
            .child(
                div()
                    .text_size(pixels(13.0))
                    .text_color(rgb(MUTED))
                    .child("Drop images anywhere. Make room for what inspires you."),
            )
            .child(
                Self::button("empty-add", "Choose images   ↗")
                    .mt(pixels(5.0))
                    .border_1()
                    .border_color(rgb(0x454b42))
                    .text_color(rgb(ACCENT))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(|this, _, _, cx| this.choose_images(cx))),
            )
            .child(
                div()
                    .text_size(pixels(10.0))
                    .text_color(rgb(0x666d6b))
                    .child("PNG, JPG, WEBP, GIF, BMP, TIFF  ·  or paste an image"),
            )
    }

    fn rename_modal(&self, input: Entity<TextInput>, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .absolute()
            .size_full()
            .bg(rgba(0x00000088))
            .flex()
            .items_center()
            .justify_center()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                Self::panel()
                    .w(pixels(380.0))
                    .p(pixels(24.0))
                    .flex()
                    .flex_col()
                    .gap(pixels(18.0))
                    .child(div().text_size(pixels(18.0)).child("Name your board"))
                    .child(
                        div()
                            .border_1()
                            .border_color(rgb(0x65715c))
                            .rounded(pixels(6.0))
                            .p(pixels(5.0))
                            .child(input),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(pixels(8.0))
                            .child(
                                Self::button("cancel-rename", "Cancel").on_click(cx.listener(
                                    |this, _, window, cx| {
                                        this.rename = None;
                                        this.focus.focus(window);
                                        cx.notify();
                                    },
                                )),
                            )
                            .child(
                                Self::button("save-name", "Save name")
                                    .bg(rgb(ACCENT))
                                    .text_color(rgb(0x20271e))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.finish_rename(window, cx)
                                    })),
                            ),
                    ),
            )
    }

    fn help(&self) -> impl IntoElement {
        let shortcuts = [
            ("Boards sidebar", "Ctrl / ⌘ B"),
            ("New board", "Ctrl / ⌘ N"),
            ("Add images", "Ctrl / ⌘ O"),
            ("Paste image", "Ctrl / ⌘ V"),
            ("Pan canvas", "Drag empty space / Space + drag"),
            ("Zoom at pointer", "Ctrl / ⌘ + scroll"),
            ("Zoom in / out", "+ / −"),
            ("Trackpad", "Scroll to pan · pinch to zoom on Mac"),
            ("Select several", "Shift + click / Shift + drag"),
            ("Resize image", "Drag the bottom-right handle"),
            ("Fit everything", "Shift 1 / F"),
            ("Fit selection", "Shift 2"),
            ("Actual size", "Shift 0 / 0"),
            ("Duplicate", "Ctrl / ⌘ D"),
            ("Delete selection", "Delete / Backspace"),
            ("Undo / redo", "Ctrl / ⌘ Z / Shift Z"),
            ("Rename board", "F2"),
        ];
        Self::panel()
            .absolute()
            .right(pixels(24.0))
            .bottom(pixels(76.0))
            .w(pixels(410.0))
            .p(pixels(22.0))
            .flex()
            .flex_col()
            .gap(pixels(14.0))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .text_size(pixels(16.0))
                    .mb(pixels(5.0))
                    .child("Make yourself at home"),
            )
            .children(shortcuts.map(|(name, key)| {
                div()
                    .flex()
                    .justify_between()
                    .text_size(pixels(11.0))
                    .child(div().text_color(rgb(MUTED)).child(name))
                    .child(key)
            }))
    }
}

impl Render for Magpie {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.viewport = [
            coord(window.viewport_size().width),
            coord(window.viewport_size().height),
        ];
        self.animate_frame(window, cx);
        let board = self.library.board();
        let camera = board.camera;
        let viewport = self.viewport;
        let panning = self.space || self.hand || matches!(self.gesture, Some(Gesture::Pan { .. }));
        let mut root = div()
            .id("magpie")
            .relative()
            .size_full()
            .overflow_hidden()
            .bg(rgb(BG))
            .text_color(rgb(TEXT))
            .font_family("Inter")
            .track_focus(&self.focus)
            .cursor(if self.gesture.is_some() {
                CursorStyle::ClosedHand
            } else if panning {
                CursorStyle::OpenHand
            } else {
                CursorStyle::Arrow
            })
            .on_key_down(cx.listener(Self::key_down))
            .on_key_up(cx.listener(|this, event: &KeyUpEvent, _, cx| {
                if event.keystroke.key == "space" {
                    this.space = false;
                    cx.notify();
                }
            }))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
            .on_mouse_down(MouseButton::Middle, cx.listener(Self::mouse_down))
            .on_mouse_down(MouseButton::Right, cx.listener(Self::mouse_down))
            .on_mouse_move(cx.listener(Self::mouse_move))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| this.finish_gesture(cx)),
            )
            .on_mouse_up(
                MouseButton::Middle,
                cx.listener(|this, _, _, cx| this.finish_gesture(cx)),
            )
            .on_mouse_up(
                MouseButton::Right,
                cx.listener(|this, _, _, cx| this.finish_gesture(cx)),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| this.finish_gesture(cx)),
            )
            .on_scroll_wheel(cx.listener(Self::scroll))
            .on_drop(cx.listener(|this, paths: &ExternalPaths, window, cx| {
                let p = window.mouse_position();
                this.import_paths(paths.paths().to_vec(), [coord(p.x), coord(p.y)], cx);
            }))
            .drag_over::<ExternalPaths>(|style, _, _, _| style.bg(rgb(0x20271f)))
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, _| {
                        let spacing = grid_spacing(camera.zoom);
                        let mut x = camera.x.rem_euclid(spacing);
                        while x < viewport[0] {
                            let mut y = camera.y.rem_euclid(spacing);
                            while y < viewport[1] {
                                window.paint_quad(fill(
                                    Bounds::new(
                                        point(bounds.left() + pixels(x), bounds.top() + pixels(y)),
                                        size(pixels(1.0), pixels(1.0)),
                                    ),
                                    rgb(0x303437),
                                ));
                                y += spacing;
                            }
                            x += spacing;
                        }
                    },
                )
                .absolute()
                .size_full(),
            );

        for item in &board.images {
            let p = camera.screen([item.x, item.y]);
            let w = item.width * camera.zoom;
            let h = item.height * camera.zoom;
            if p[0] + w < -20.0
                || p[1] + h < -20.0
                || p[0] > viewport[0] + 20.0
                || p[1] > viewport[1] + 20.0
            {
                continue;
            }
            let selected = self.selected.contains(&item.id);
            if w.max(h) > 1_000_000.0 {
                root = root.child(crate::canvas_image::magnified_image(
                    item,
                    camera,
                    viewport,
                    self.storage.asset(&item.asset),
                    window,
                    cx,
                ));
            } else {
                root = root.child(
                    div()
                        .absolute()
                        .left(pixels(p[0]))
                        .top(pixels(p[1]))
                        .w(pixels(w))
                        .h(pixels(h))
                        .bg(rgb(0x25282a))
                        .shadow_lg()
                        .child(
                            img(self.storage.asset(&item.asset))
                                .size_full()
                                .object_fit(ObjectFit::Contain),
                        ),
                );
            }
            if selected {
                root = root.child(crate::canvas_image::selection_outline(p, [w, h], viewport));
                if self.selected.len() == 1
                    && p[0] + w >= -8.0
                    && p[0] + w <= viewport[0] + 8.0
                    && p[1] + h >= -8.0
                    && p[1] + h <= viewport[1] + 8.0
                {
                    root = root.child(
                        div()
                            .absolute()
                            .left(pixels(p[0] + w - 4.0))
                            .top(pixels(p[1] + h - 4.0))
                            .size(pixels(8.0))
                            .bg(rgb(ACCENT))
                            .border_1()
                            .border_color(rgb(BG))
                            .cursor(CursorStyle::ResizeUpLeftDownRight),
                    );
                }
            }
        }
        if let Some(Gesture::Marquee { start, end }) = self.gesture {
            root = root.child(
                div()
                    .absolute()
                    .left(pixels(start[0].min(end[0])))
                    .top(pixels(start[1].min(end[1])))
                    .w(pixels((end[0] - start[0]).abs()))
                    .h(pixels((end[1] - start[1]).abs()))
                    .bg(rgba(0xc6d5b512))
                    .border_1()
                    .border_color(rgb(ACCENT)),
            );
        }
        if board.images.is_empty() {
            root = root.child(self.empty(cx));
        }
        root = root.child(self.header(cx)).child(self.toolbar(cx)).child(
            Self::button("help", "?")
                .absolute()
                .right(pixels(24.0))
                .bottom(pixels(26.0))
                .w(pixels(34.0))
                .border_1()
                .border_color(rgb(BORDER))
                .text_color(rgb(MUTED))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(|this, _, window, cx| {
                    this.zoom_input = None;
                    this.focus.focus(window);
                    this.help_open = !this.help_open;
                    cx.notify();
                })),
        );
        if self.boards_open {
            root = root.child(self.board_sidebar(cx));
        }
        if self.help_open {
            root = root.child(self.help());
        }
        if let Some(input) = &self.zoom_input {
            root = root.child(self.zoom_menu(input.clone(), cx));
        }
        if let Some(message) = &self.message {
            root = root.child(
                div()
                    .absolute()
                    .bottom(pixels(88.0))
                    .left(pixels(self.canvas_left()))
                    .right_0()
                    .flex()
                    .justify_center()
                    .child(
                        Self::panel()
                            .px(pixels(18.0))
                            .py(pixels(10.0))
                            .max_w(pixels(700.0))
                            .text_size(pixels(12.0))
                            .child(message.clone()),
                    ),
            );
        }
        if let Some(input) = &self.rename {
            root = root.child(self.rename_modal(input.clone(), cx));
        }
        root
    }
}
