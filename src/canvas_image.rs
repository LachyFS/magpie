//! At very high magnification, paint visible source pixels in viewport space.
//! Sending trillion-pixel image bounds to a float GPU loses the whole viewport.
use gpui::{prelude::*, *};
use magpie::{
    model::{Camera, ImageItem},
    navigation::clip_rect,
};
use std::path::PathBuf;

pub fn magnified_image(
    item: &ImageItem,
    camera: Camera,
    viewport: [f64; 2],
    path: PathBuf,
    window: &mut Window,
    cx: &mut App,
) -> impl IntoElement {
    let origin = camera.screen([item.x, item.y]);
    let dimensions = [item.width * camera.zoom, item.height * camera.zoom];
    let data = window.use_asset::<ImageAssetLoader>(&Resource::Path(path.into()), cx);
    canvas(
        move |_, _, _| data,
        move |_, data, window, _| {
            paint_rect(origin, dimensions, viewport, rgb(0x25282a).into(), window);
            let Some(Ok(data)) = data else {
                return;
            };
            let size = data.size(0);
            let (width, height) = (size.width.0 as usize, size.height.0 as usize);
            let Some(bytes) = data.as_bytes(0) else {
                return;
            };
            let pixel = [dimensions[0] / width as f64, dimensions[1] / height as f64];
            let left = (-origin[0] / pixel[0]).floor().max(0.0) as usize;
            let top = (-origin[1] / pixel[1]).floor().max(0.0) as usize;
            let right =
                (((viewport[0] - origin[0]) / pixel[0]).ceil().max(0.0) as usize).min(width);
            let bottom =
                (((viewport[1] - origin[1]) / pixel[1]).ceil().max(0.0) as usize).min(height);
            for y in top..bottom {
                for x in left..right {
                    let offset = (y * width + x) * 4;
                    // GPUI's image loader supplies BGRA pixels.
                    let color = rgba(u32::from_be_bytes([
                        bytes[offset + 2],
                        bytes[offset + 1],
                        bytes[offset],
                        bytes[offset + 3],
                    ]));
                    paint_rect(
                        [
                            origin[0] + x as f64 * pixel[0],
                            origin[1] + y as f64 * pixel[1],
                        ],
                        pixel,
                        viewport,
                        color.into(),
                        window,
                    );
                }
            }
        },
    )
    .absolute()
    .size_full()
}

pub fn selection_outline(
    origin: [f64; 2],
    dimensions: [f64; 2],
    viewport: [f64; 2],
) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |_, _, window, _| {
            let [x, y] = [origin[0] - 3.0, origin[1] - 3.0];
            let [w, h] = [dimensions[0] + 6.0, dimensions[1] + 6.0];
            for (origin, size) in [
                ([x, y], [w, 1.0]),
                ([x, y + h - 1.0], [w, 1.0]),
                ([x, y], [1.0, h]),
                ([x + w - 1.0, y], [1.0, h]),
            ] {
                paint_rect(origin, size, viewport, rgb(0xc6d5b5).into(), window);
            }
        },
    )
    .absolute()
    .size_full()
}

fn paint_rect(
    origin: [f64; 2],
    dimensions: [f64; 2],
    viewport: [f64; 2],
    color: Hsla,
    window: &mut Window,
) {
    if let Some([x, y, w, h]) = clip_rect(origin, dimensions, viewport) {
        window.paint_quad(fill(
            Bounds::new(
                point(px(x as f32), px(y as f32)),
                size(px(w as f32), px(h as f32)),
            ),
            color,
        ));
    }
}
