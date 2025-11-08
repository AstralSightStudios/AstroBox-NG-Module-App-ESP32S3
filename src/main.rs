use esp_idf_svc::{
    hal::{gpio::Pins, prelude::Peripherals},
    log::EspLogger,
    sys::link_patches,
};
use std::time::Duration;

pub mod gui;
pub mod miwear;
pub mod statlogger;
pub mod touch;

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

    let Peripherals {
        pins,
        ledc,
        spi2,
        i2c0,
        ..
    } = Peripherals::take()?;
    let Pins {
        gpio0,
        gpio1,
        gpio2,
        gpio3,
        gpio4,
        gpio5,
        gpio6,
        gpio7,
        gpio18,
        gpio19,
        ..
    } = pins;

    let (mut display, mut backlight) = gui::display::init_display_gc9a01(
        spi2,
        ledc,
        gui::display::DisplayPins {
            backlight: gpio2,
            rst: gpio3,
            dc: gpio4,
            cs: gpio5,
            mosi: gpio6,
            sclk: gpio7,
        },
    )?;

    let _ = &mut backlight;

    touch::spawn_touch_task(
        i2c0,
        touch::TouchPins {
            sda: gpio18,
            scl: gpio19,
            interrupt: gpio1,
            reset: gpio0,
        },
    )?;

    tokio::task::spawn_local(async {
        if let Err(err) = miwear::connect().await {
            log::error!("miwear connect failed: {err:?}");
        }
    });

    tokio::task::spawn_local(async move {
        loop {
            if let Err(err) = gui::slint_ui::render_hello_world(&mut display) {
                log::error!("render loop exited: {err:?}");
                break;
            }
            tokio::time::sleep(Duration::from_millis(16)).await;
        }
    })
    .await?;

    Ok(())
}
