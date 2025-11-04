use anyhow::{anyhow, Result};
use esp_idf_svc::hal::{
    delay::Delay,
    gpio::{Gpio3, Gpio4, PinDriver, Pins},
    ledc::{config::TimerConfig, LedcDriver, LedcTimerDriver, LEDC},
    prelude::Peripherals,
    spi::{config::DriverConfig, Dma, SpiConfig, SpiDeviceDriver, SpiDriver},
};
use mipidsi::{
    interface::SpiInterface,
    models::GC9A01,
    options::{ColorInversion, ColorOrder, Orientation, RefreshOrder},
    Builder,
};

type DisplayDcPin<'d> = PinDriver<'d, Gpio4, esp_idf_svc::hal::gpio::Output>;
type DisplayRstPin<'d> = PinDriver<'d, Gpio3, esp_idf_svc::hal::gpio::Output>;
type DisplayInterface<'d> = SpiInterface<'d, SpiDeviceDriver<'d, SpiDriver<'d>>, DisplayDcPin<'d>>;
type DisplayType<'d> = mipidsi::Display<DisplayInterface<'d>, GC9A01, DisplayRstPin<'d>>;

const DISPLAY_SPI_BUFFER_SIZE: usize = 1024;
static mut DISPLAY_SPI_BUFFER: [u8; DISPLAY_SPI_BUFFER_SIZE] = [0; DISPLAY_SPI_BUFFER_SIZE];

pub fn init_display_gc9a01(p: Peripherals) -> Result<(DisplayType<'static>, LedcDriver<'static>)> {
    let Peripherals {
        pins, ledc, spi2, ..
    } = p;
    let Pins {
        gpio2,
        gpio3,
        gpio4,
        gpio5,
        gpio6,
        gpio7,
        ..
    } = pins;
    let LEDC {
        timer0, channel0, ..
    } = ledc;

    let dc = PinDriver::output(gpio4)?; // D/C
    let rst = PinDriver::output(gpio3)?; // RST

    let ledc_timer = LedcTimerDriver::new(timer0, &TimerConfig::new().frequency(25_000.into()))?;
    let mut backlight = LedcDriver::new(channel0, ledc_timer, gpio2)?;
    backlight.set_duty(backlight.get_max_duty() / 2)?;

    let spi_driver = SpiDriver::new(
        spi2,
        gpio7, // SCLK
        gpio6, // MOSI (SDO)
        Option::<esp_idf_svc::hal::gpio::Gpio8>::None,
        &DriverConfig {
            dma: Dma::Auto(DISPLAY_SPI_BUFFER_SIZE),
            ..Default::default()
        },
    )?;
    let spi_dev = SpiDeviceDriver::new(
        spi_driver,
        Some(gpio5), // CS
        &SpiConfig::new().baudrate(40_000_000.into()),
    )?;

    #[allow(static_mut_refs)]
    let buffer: &'static mut [u8] = unsafe { &mut DISPLAY_SPI_BUFFER };
    let di = SpiInterface::new(spi_dev, dc, buffer);

    let mut delay = Delay::new_default();
    let display = Builder::new(GC9A01, di)
        .reset_pin(rst)
        .invert_colors(ColorInversion::Inverted)
        .color_order(ColorOrder::Rgb)
        .orientation(Orientation::new().rotate(mipidsi::options::Rotation::Deg0))
        .refresh_order(RefreshOrder::new(
            mipidsi::options::VerticalRefreshOrder::TopToBottom,
            mipidsi::options::HorizontalRefreshOrder::LeftToRight,
        ))
        .display_size(240, 240)
        .display_offset(0, 0)
        .init(&mut delay)
        .map_err(|e| anyhow!("display init failed: {:?}", e))?;

    Ok((display, backlight))
}
