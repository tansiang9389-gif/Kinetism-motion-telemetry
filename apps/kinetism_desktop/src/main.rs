//! Kinetism Desktop: shoulder abduction, Fruit-Ninja PNF game and muscle
//! progress for the `Physio_A` paired-arm sensors.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod clinical;
mod demo;
mod game;
mod kinematics;
mod math;
mod model;
mod sensor;
mod sim;
mod store;
mod theme;
mod tracker;
mod ui;
mod voice;

use eframe::egui;
use eframe::egui_wgpu::{self, wgpu};

/// wgpu backend choice. On Windows use DX12 only: the OpenGL probe thread can
/// overflow its stack and old hybrid-laptop Vulkan drivers can stall startup,
/// while DX12 always has an adapter (WARP fallback). `WGPU_BACKEND` overrides.
fn wgpu_setup() -> egui_wgpu::WgpuSetup {
    let default_backends = if cfg!(windows) {
        wgpu::Backends::DX12
    } else {
        wgpu::Backends::PRIMARY
    };
    let mut setup = egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    setup.instance_descriptor.backends = wgpu::Backends::from_env().unwrap_or(default_backends);
    setup.power_preference =
        wgpu::PowerPreference::from_env().unwrap_or(wgpu::PowerPreference::LowPower);
    egui_wgpu::WgpuSetup::CreateNew(setup)
}

fn main() -> eframe::Result {
    env_logger::init();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("failed to start the async runtime");
    let mut options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Kinetism Desktop")
            .with_inner_size([1360.0, 880.0])
            .with_min_inner_size([1024.0, 680.0]),
        ..Default::default()
    };
    options.wgpu_options.wgpu_setup = wgpu_setup();
    eframe::run_native(
        "Kinetism Desktop",
        options,
        Box::new(move |cc| Ok(Box::new(app::KinetismApp::new(cc, runtime)))),
    )
}
