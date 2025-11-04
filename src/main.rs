use esp_idf_svc::{hal::prelude::Peripherals, log::EspLogger, sys::link_patches};
use std::time::Duration;

pub mod gui;
pub mod miwear;
pub mod statlogger;

fn main() -> anyhow::Result<()> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let local = tokio::task::LocalSet::new();
    local.block_on(&rt, run_app())
}

async fn run_app() -> anyhow::Result<()> {
    link_patches();
    EspLogger::initialize_default();
    corelib::ecs::init_runtime_default_with_stack(64 * 1024);
    tokio::task::spawn_local(async {
        let mut ticker = tokio::time::interval(Duration::from_secs(10));
        loop {
            ticker.tick().await;
            statlogger::log_heap_info();
        }
    });

    let peripherals = Peripherals::take()?;
    let (mut display, mut backlight) = gui::display::init_display_gc9a01(peripherals)?;

    let _ = &mut backlight;

    miwear::connect().await?;

    loop {
        gui::slint_ui::render_hello_world(&mut display)?;
    }
}
