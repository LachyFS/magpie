#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

mod app;
mod input;

use gpui::{prelude::*, *};
use magpie::storage::Storage;

fn main() -> anyhow::Result<()> {
    env_logger::init();
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    match arguments.first().and_then(|arg| arg.to_str()) {
        Some("--version" | "-V") => {
            println!("Magpie {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Some("--help" | "-h") => {
            println!(
                "Magpie {}\n\nUsage: magpie [IMAGE ...]\n\n  --version, -V    Print the version\n  --help, -h       Show this help\n\nSet MAGPIE_DATA_DIR to use a separate local library.",
                env!("CARGO_PKG_VERSION")
            );
            return Ok(());
        }
        _ => {}
    }
    let paths = arguments
        .into_iter()
        .map(std::path::PathBuf::from)
        .collect::<Vec<_>>();
    let storage = Storage::open(None)?;
    let library = storage.load()?;
    Application::new().run(move |cx: &mut App| {
        input::init(cx);
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        let bounds = Bounds::centered(None, size(px(1280.0), px(820.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("Magpie".into()),
                    ..Default::default()
                }),
                window_min_size: Some(size(px(720.0), px(480.0))),
                app_id: Some("app.magpie.Magpie".into()),
                ..Default::default()
            },
            |window, cx| {
                let view = cx.new(|cx| app::Magpie::new(storage, library, window, cx));
                view.update(cx, |view, cx| {
                    if !paths.is_empty() {
                        view.import_paths(paths, [260.0, 180.0], cx);
                    }
                });
                view
            },
        )
        .expect("Couldn't open the Magpie window");
        cx.activate(true);
    });
    Ok(())
}
