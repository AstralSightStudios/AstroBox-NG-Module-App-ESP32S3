use std::{
    cell::RefCell,
    ops::Range,
    rc::Rc,
    time::{Duration, Instant},
};

use anyhow::{anyhow, Result};
use embedded_graphics_core::{
    draw_target::DrawTarget,
    pixelcolor::{raw::RawU16, Rgb565},
    prelude::{Point, Size},
    primitives::Rectangle,
};
use esp_idf_svc::sys::esp_get_free_heap_size;
use slint::{
    platform::{
        self,
        software_renderer::{
            LineBufferProvider, MinimalSoftwareWindow, RepaintBufferType, Rgb565Pixel,
        },
        Platform, PointerEventButton, WindowAdapter,
    },
    LogicalPosition, PhysicalSize, SharedString,
};

use super::display::DisplayType;

slint::include_modules!();

pub const DISPLAY_WIDTH: usize = 240;
pub const DISPLAY_HEIGHT: usize = 240;
thread_local! {
    static PLATFORM_WINDOW: RefCell<Option<Rc<MinimalSoftwareWindow>>> =
        const { RefCell::new(None) };
    static FRAME_STATS: RefCell<FrameStats> = RefCell::new(FrameStats::new());
    static APP_INSTANCE: RefCell<Option<App>> = const { RefCell::new(None) };
}

pub fn render_hello_world(display: &mut DisplayType<'static>) -> Result<()> {
    let window = ensure_platform_window()?;
    window.set_size(PhysicalSize::new(DISPLAY_WIDTH as _, DISPLAY_HEIGHT as _));
    window.request_redraw();

    ensure_app()?;

    let frame_start = Instant::now();
    let (displayed_fps, last_render_duration) =
        FRAME_STATS.with(|cell| cell.borrow().snapshot_for_display());

    let heap_bytes = unsafe { esp_get_free_heap_size() };
    let fps_display = if displayed_fps > f32::EPSILON {
        format!("{displayed_fps:.1}")
    } else {
        "--".to_string()
    };
    let render_display = if let Some(duration) = last_render_duration {
        format!("{:.2}", duration.as_secs_f32() * 1_000.0)
    } else {
        "--".to_string()
    };
    let heap_kb = heap_bytes as f32 / 1024.0;
    let stats_text = SharedString::from(format!(
        "FPS: {fps}\nRender: {render} ms\nHeap: {heap:.1} KB",
        fps = fps_display,
        render = render_display,
        heap = heap_kb
    ));
    set_stats_text(stats_text);

    platform::update_timers_and_animations();

    let render_error = RefCell::<Option<anyhow::Error>>::new(None);
    let display_ptr: *mut DisplayType<'static> = display;
    let mut line_buffer = [Rgb565Pixel(0); DISPLAY_WIDTH];

    while window.draw_if_needed(|renderer| {
        if render_error.borrow().is_some() {
            return;
        }

        // Safety: the draw loop is single-threaded and guarantees no aliasing with other uses.
        let display_ref = unsafe { &mut *display_ptr };
        let mut provider = DisplayLineProvider::new(display_ref, &mut line_buffer, &render_error);
        renderer.render_by_line(&mut provider);
        if let Err(err) = provider.finish() {
            *render_error.borrow_mut() = Some(err);
        }
    }) {
        platform::update_timers_and_animations();
    }

    if let Some(err) = render_error.into_inner() {
        return Err(err);
    }

    let render_duration = frame_start.elapsed();
    FRAME_STATS.with(|cell| {
        cell.borrow_mut()
            .update_after_frame(frame_start, render_duration);
    });

    Ok(())
}

const MAX_BATCH_LINES: usize = 16;

struct DisplayLineProvider<'a, 'b> {
    display: &'a mut DisplayType<'static>,
    line_buffer: &'b mut [Rgb565Pixel; DISPLAY_WIDTH],
    accumulator: LineAccumulator,
    error: &'b RefCell<Option<anyhow::Error>>,
}

impl<'a, 'b> DisplayLineProvider<'a, 'b> {
    fn new(
        display: &'a mut DisplayType<'static>,
        line_buffer: &'b mut [Rgb565Pixel; DISPLAY_WIDTH],
        error: &'b RefCell<Option<anyhow::Error>>,
    ) -> Self {
        Self {
            display,
            line_buffer,
            accumulator: LineAccumulator::new(),
            error,
        }
    }

    fn finish(&mut self) -> Result<()> {
        self.accumulator.flush(self.display)
    }
}

impl<'a, 'b, 'c> LineBufferProvider for &'c mut DisplayLineProvider<'a, 'b> {
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

        if let Err(err) = self
            .accumulator
            .push_line(line, range, segment, self.display)
        {
            *self.error.borrow_mut() = Some(err);
        }
    }
}

struct LineAccumulator {
    start_line: usize,
    range: Range<usize>,
    line_count: usize,
    buffer: Vec<Rgb565Pixel>,
}

impl LineAccumulator {
    fn new() -> Self {
        Self {
            start_line: 0,
            range: 0..0,
            line_count: 0,
            buffer: Vec::with_capacity(DISPLAY_WIDTH * MAX_BATCH_LINES),
        }
    }

    fn push_line(
        &mut self,
        line: usize,
        range: Range<usize>,
        pixels: &[Rgb565Pixel],
        display: &mut DisplayType<'static>,
    ) -> Result<()> {
        if pixels.is_empty() {
            return Ok(());
        }

        if self.line_count == 0 {
            self.start_line = line;
            self.range = range.clone();
        } else {
            let expected_line = self.start_line + self.line_count;
            if line != expected_line
                || range.start != self.range.start
                || range.end != self.range.end
            {
                self.flush(display)?;
                self.start_line = line;
                self.range = range.clone();
            }
        }

        self.buffer.extend_from_slice(pixels);
        self.line_count += 1;

        if self.line_count >= MAX_BATCH_LINES {
            self.flush(display)?;
        }
        Ok(())
    }

    fn flush(&mut self, display: &mut DisplayType<'static>) -> Result<()> {
        if self.line_count == 0 {
            return Ok(());
        }

        let rect = Rectangle::new(
            Point::new(self.range.start as i32, self.start_line as i32),
            Size::new(self.range.len() as u32, self.line_count as u32),
        );

        let colors = self
            .buffer
            .iter()
            .take(self.range.len() * self.line_count)
            .map(|Rgb565Pixel(pixel)| Rgb565::from(RawU16::new(*pixel)));

        display
            .fill_contiguous(&rect, colors)
            .map_err(|e| anyhow!("Failed to refresh region {:?}: {e:?}", rect))?;

        self.buffer.clear();
        self.line_count = 0;
        Ok(())
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

fn ensure_app() -> Result<()> {
    APP_INSTANCE.with(|cell| {
        if cell.borrow().is_none() {
            let app = App::new().map_err(|e| anyhow!("Failed to create Slint App: {:?}", e))?;
            app.show()
                .map_err(|e| anyhow!("Failed to show Slint App: {:?}", e))?;
            cell.replace(Some(app));
        }
        Ok(())
    })
}

fn set_stats_text(stats: SharedString) {
    APP_INSTANCE.with(|cell| {
        if let Some(app) = cell.borrow().as_ref() {
            app.set_stats_text(stats.clone());
            PLATFORM_WINDOW.with(|window_cell| {
                if let Some(window) = window_cell.borrow().as_ref() {
                    window.request_redraw();
                }
            });
        }
    });
}

struct FrameStats {
    last_frame_start: Option<Instant>,
    last_render_time: Option<Duration>,
    last_fps: f32,
}

impl FrameStats {
    const fn new() -> Self {
        Self {
            last_frame_start: None,
            last_render_time: None,
            last_fps: 0.0,
        }
    }

    fn snapshot_for_display(&self) -> (f32, Option<Duration>) {
        (self.last_fps, self.last_render_time)
    }

    fn update_after_frame(&mut self, frame_start: Instant, render_time: Duration) {
        if let Some(previous_start) = self.last_frame_start {
            if let Some(frame_interval) = frame_start.checked_duration_since(previous_start) {
                let frame_time = frame_interval.as_secs_f32();
                if frame_time > f32::EPSILON {
                    self.last_fps = 1.0 / frame_time;
                }
            }
        }
        self.last_frame_start = Some(frame_start);
        self.last_render_time = Some(render_time);
    }
}

#[derive(Clone, Copy, Debug)]
pub enum PointerAction {
    Press,
    Move,
    Release,
}

pub fn dispatch_pointer_action(action: PointerAction, position: (f32, f32)) -> Result<()> {
    let window = ensure_platform_window()?;
    let logical_position = LogicalPosition::new(position.0, position.1);
    let event = match action {
        PointerAction::Press => slint::platform::WindowEvent::PointerPressed {
            position: logical_position,
            button: PointerEventButton::Left,
        },
        PointerAction::Move => slint::platform::WindowEvent::PointerMoved {
            position: logical_position,
        },
        PointerAction::Release => slint::platform::WindowEvent::PointerReleased {
            position: logical_position,
            button: PointerEventButton::Left,
        },
    };
    window.dispatch_event(event);
    window.request_redraw();
    Ok(())
}

pub fn set_touch_text(stats: SharedString) {
    APP_INSTANCE.with(|cell| {
        if let Some(app) = cell.borrow().as_ref() {
            app.set_touch_text(stats.clone());
            PLATFORM_WINDOW.with(|window_cell| {
                if let Some(window) = window_cell.borrow().as_ref() {
                    window.request_redraw();
                }
            });
        }
    });
}
