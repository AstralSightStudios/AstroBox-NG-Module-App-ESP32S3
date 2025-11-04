use std::{cell::RefCell, ops::Range, rc::Rc, time::Instant};

use anyhow::{anyhow, Result};
use embedded_graphics_core::{
    draw_target::DrawTarget,
    pixelcolor::{raw::RawU16, Rgb565},
    prelude::{Point, Size},
    primitives::Rectangle,
};
use slint::{
    platform::{
        self,
        software_renderer::{
            LineBufferProvider, MinimalSoftwareWindow, RepaintBufferType, Rgb565Pixel,
        },
        Platform, WindowAdapter,
    },
    PhysicalSize,
};

use super::display::DisplayType;

slint::include_modules!();

const DISPLAY_WIDTH: usize = 240;
const DISPLAY_HEIGHT: usize = 240;

thread_local! {
    static PLATFORM_WINDOW: RefCell<Option<Rc<MinimalSoftwareWindow>>> =
        const { RefCell::new(None) };
}

pub fn render_hello_world(display: &mut DisplayType<'static>) -> Result<()> {
    let window = ensure_platform_window()?;

    window.set_size(PhysicalSize::new(DISPLAY_WIDTH as _, DISPLAY_HEIGHT as _));
    window.request_redraw();

    let ui = App::new().map_err(|e| anyhow!("Failed to create Slint App: {:?}", e))?;
    ui.show()
        .map_err(|e| anyhow!("Failed to show Slint App: {:?}", e))?;

    platform::update_timers_and_animations();

    let mut line_buffer = [Rgb565Pixel(0); DISPLAY_WIDTH];
    let render_error = RefCell::<Option<anyhow::Error>>::new(None);

    while window.draw_if_needed(|renderer| {
        let provider = DisplayLineProvider {
            display,
            line_buffer: &mut line_buffer,
            error: &render_error,
        };
        renderer.render_by_line(provider);
    }) {
        platform::update_timers_and_animations();
    }

    if let Some(err) = render_error.into_inner() {
        return Err(err);
    }

    Ok(())
}

struct DisplayLineProvider<'a, 'b> {
    display: &'a mut DisplayType<'static>,
    line_buffer: &'b mut [Rgb565Pixel; DISPLAY_WIDTH],
    error: &'b RefCell<Option<anyhow::Error>>,
}

impl LineBufferProvider for DisplayLineProvider<'_, '_> {
    type TargetPixel = Rgb565Pixel;

    fn process_line(
        &mut self,
        line: usize,
        range: Range<usize>,
        render_fn: impl FnOnce(&mut [Self::TargetPixel]),
    ) {
        if self.error.borrow().is_some() {
            return;
        }

        let segment = &mut self.line_buffer[range.clone()];
        render_fn(segment);

        let rect = Rectangle::new(
            Point::new(range.start as i32, line as i32),
            Size::new(range.len() as u32, 1),
        );

        if let Err(e) = self.display.fill_contiguous(
            &rect,
            segment.iter().map(|p| Rgb565::from(RawU16::new(p.0))),
        ) {
            *self.error.borrow_mut() = Some(anyhow!("Failed to refresh line {line}: {e:?}"));
        }
    }
}

#[derive(Clone)]
struct SimplePlatform {
    window: Rc<MinimalSoftwareWindow>,
    start: Instant,
}

impl Platform for SimplePlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        Ok(self.window.clone())
    }

    fn run_event_loop(&self) -> Result<(), slint::PlatformError> {
        Ok(())
    }

    fn duration_since_start(&self) -> std::time::Duration {
        self.start.elapsed()
    }
}

fn ensure_platform_window() -> Result<Rc<MinimalSoftwareWindow>> {
    PLATFORM_WINDOW.with(|cell| {
        if let Some(existing) = cell.borrow().as_ref() {
            return Ok(existing.clone());
        }

        let window = MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer);
        let platform = SimplePlatform {
            window: window.clone(),
            start: Instant::now(),
        };
        platform::set_platform(Box::new(platform))
            .map_err(|e| anyhow!("Failed to set Slint platform: {e:?}"))?;
        *cell.borrow_mut() = Some(window.clone());
        Ok(window)
    })
}
