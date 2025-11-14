use core::convert::TryInto;

use anyhow::anyhow;
use corelib::{
    device::xiaomi::{components::network::NetworkComponent, XiaomiDevice},
    ecs::entity::EntityExt,
};
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::{gpio::Pins, modem::Modem, prelude::Peripherals},
    io::vfs::MountedEventfs,
    log::EspLogger,
    nvs::EspDefaultNvsPartition,
    sys::link_patches,
    wifi::{AuthMethod, BlockingWifi, ClientConfiguration, Configuration, EspWifi},
};
use std::time::Duration;

mod allocator;
pub mod gui;
pub mod miwear;
pub mod statlogger;
pub mod touch;

const WIFI_SSID: &str = "ASUS_AX86U";
const WIFI_PASSWORD: &str = "reveries2005";
const ECS_STACK_SIZE: usize = 32 * 1024;

fn main() -> anyhow::Result<()> {
    link_patches();
    EspLogger::initialize_default();

    let _mounted_eventfs = MountedEventfs::mount(5)?;

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    let local = tokio::task::LocalSet::new();

    local.block_on(&rt, run_app())
}

async fn run_app() -> anyhow::Result<()> {
    let Peripherals {
        pins,
        ledc,
        spi2,
        i2c0,
        modem,
        ..
    } = Peripherals::take()?;

    let _wifi = init_wifi(modem)?;

    corelib::ecs::init_runtime_default_with_stack(ECS_STACK_SIZE);
    tokio::task::spawn_local(async {
        let mut ticker = tokio::time::interval(Duration::from_secs(1));
        loop {
            ticker.tick().await;
            statlogger::log_heap_info();
            log_network_meter().await;
        }
    });

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
        gpio16,
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
            scl: gpio16,
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

async fn log_network_meter() {
    let speeds = corelib::ecs::with_rt_mut(|rt| {
        rt.entities
            .values_mut()
            .filter_map(|entity| {
                let dev = entity.as_any_mut().downcast_mut::<XiaomiDevice>()?;
                let name = dev.name().to_string();
                let addr = dev.addr().to_string();
                let speed = dev
                    .get_component_as_mut::<NetworkComponent>(NetworkComponent::ID)
                    .ok()
                    .map(|comp| comp.last_speed)?;
                Some((name, addr, speed))
            })
            .collect::<Vec<_>>()
    })
    .await;

    if speeds.is_empty() {
        log::info!("NET meter: no connected devices");
        return;
    }

    for (name, addr, speed) in speeds {
        log::info!(
            "NET meter {name}({addr}) ↑{:.1} KB/s ↓{:.1} KB/s",
            speed.write / 1024.0,
            speed.read / 1024.0
        );
    }
}

fn init_wifi(modem: Modem) -> anyhow::Result<BlockingWifi<EspWifi<'static>>> {
    let sys_loop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;

    let mut wifi = BlockingWifi::wrap(EspWifi::new(modem, sys_loop.clone(), Some(nvs))?, sys_loop)?;

    let wifi_configuration = Configuration::Client(ClientConfiguration {
        ssid: WIFI_SSID
            .try_into()
            .map_err(|_| anyhow!("Wi-Fi SSID is too long"))?,
        password: WIFI_PASSWORD
            .try_into()
            .map_err(|_| anyhow!("Wi-Fi password is too long"))?,
        auth_method: AuthMethod::WPA2Personal,
        ..Default::default()
    });

    wifi.set_configuration(&wifi_configuration)?;
    wifi.start()?;
    log::info!("Wi-Fi started");

    wifi.connect()?;
    log::info!("Wi-Fi connected to {}", WIFI_SSID);

    wifi.wait_netif_up()?;
    log::info!("Wi-Fi network interface is up");

    Ok(wifi)
}
