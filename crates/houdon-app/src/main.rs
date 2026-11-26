//! HoudonRef entry points for desktop and web.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod atlas;
mod masks;
mod matcaps;
mod studio;

use app::HoudonApp;

/// Depth buffer bits requested from eframe; the 3D viewport needs one.
const DEPTH_BITS: u8 = 24;

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("HoudonRef")
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([640.0, 400.0])
            .with_drag_and_drop(true),
        depth_buffer: DEPTH_BITS,
        ..Default::default()
    };
    eframe::run_native(
        "HoudonRef",
        options,
        Box::new(|cc| Ok(Box::new(HoudonApp::new(cc, DEPTH_BITS)))),
    )
}

#[cfg(target_arch = "wasm32")]
fn main() {
    use eframe::wasm_bindgen::JsCast as _;

    eframe::WebLogger::init(log::LevelFilter::Warn).ok();
    let options = eframe::WebOptions {
        depth_buffer: DEPTH_BITS,
        ..Default::default()
    };
    wasm_bindgen_futures::spawn_local(async move {
        let canvas = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id("houdon_canvas"))
            .and_then(|e| e.dyn_into::<web_sys::HtmlCanvasElement>().ok())
            .expect("the page must contain <canvas id=\"houdon_canvas\">");
        let result = eframe::WebRunner::new()
            .start(
                canvas,
                options,
                Box::new(|cc| Ok(Box::new(HoudonApp::new(cc, DEPTH_BITS)))),
            )
            .await;
        if let Err(error) = result {
            log::error!("failed to start HoudonRef: {error:?}");
        }
    });
}
