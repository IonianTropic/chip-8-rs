use pixels::{Pixels, SurfaceTexture};
use rodio::{source::SignalGenerator, OutputStream, Sink};
use std::{
    path::Path,
    time::{Duration, Instant},
};
use winit::{
    application::ApplicationHandler,
    event::{KeyEvent, WindowEvent},
    event_loop::ActiveEventLoop,
    keyboard::PhysicalKey,
    window::{Window, WindowId},
};

use crate::{
    chip_8::Chip8,
    chip_8_variant::Chip8Variant,
    draw_job::{DrawJob, Sprite},
};

const WIDTH: usize = 64;
const HEIGHT: usize = 32;
const REFRESH_DURATION: Duration = Duration::from_micros(16667); // 16667
const SYSTEM_DURATION: Duration = Duration::from_micros(16667); // 16667
const CYCLE_DURATION: Duration = Duration::from_micros(2000); // 1429

pub struct App {
    window: Option<Window>,
    pixels: Option<Pixels>,
    redraw: bool,
    _stream: OutputStream,
    sink: Sink,
    refresh_timer: Instant,
    cycle_timer: Instant,
    system_timer: Instant,
    chip_8: Box<dyn Chip8Variant>,
}

// public
impl App {
    pub fn new<P>(path: P) -> Self
    where
        P: AsRef<Path>,
    {
        let chip_8 = Box::new(Chip8::new(path));

        let (_stream, stream_handle) = OutputStream::try_default().unwrap();
        let sink = Sink::try_new(&stream_handle).unwrap();

        let init_time = Instant::now();

        Self {
            window: None,
            pixels: None,
            redraw: false,
            _stream,
            sink,
            refresh_timer: init_time,
            cycle_timer: init_time,
            system_timer: init_time,
            chip_8,
        }
    }
}

// private
impl App {
    fn main_loop(&mut self) {
        if self.chip_8.sound_timer() != 0 {
            self.sink.play();
        }

        if self.system_timer.elapsed() >= SYSTEM_DURATION {
            self.system_timer = Instant::now();
            self.chip_8.decrement_timers();
            if self.chip_8.sound_timer() == 0 {
                self.sink.pause();
            }
        }

        if self.cycle_timer.elapsed() >= CYCLE_DURATION {
            self.cycle_timer = Instant::now();
            if !self.chip_8.waiting() {
                self.chip_8.instruction_cycle();
            }
            self.render();
        }

        if self.refresh_timer.elapsed() >= REFRESH_DURATION {
            self.refresh_timer = Instant::now();
            if self.redraw {
                self.pixels.as_ref().unwrap().render().unwrap();
                self.redraw = false;
            }
        }

        self.window.as_ref().unwrap().request_redraw();
    }

    fn render(&mut self) {
        while let Some(job) = self.chip_8.poll_draw_queue() {
            match job {
                DrawJob::Draw(sprite) => {
                    self.draw_sprite(sprite);
                }
                DrawJob::Clear => self.clear_screen(),
            }
            self.redraw = true;
        }
    }

    fn clear_screen(&mut self) {
        let frame = self.pixels.as_mut().unwrap().frame_mut();
        for pixel in frame.chunks_exact_mut(4) {
            pixel[0] = 0x00;
            pixel[1] = 0x00;
            pixel[2] = 0x00;
            pixel[3] = 0xff;
        }
    }

    fn draw_sprite(&mut self, sprite: Sprite) {
        let n_x = sprite.v_x & 0x3F;
        let n_y = sprite.v_y & 0x1F;
        let mut collision = false;

        let frame = self.pixels.as_mut().unwrap().frame_mut();

        for (i, row) in sprite.buf.iter().enumerate() {
            for j in 0..8 {
                if (row & (1 << (7 - j))) >> (7 - j) == 1 {
                    // flip (x + j, y + i) -> 4 * (x + j + width * (y + i))
                    if (n_x + j) >= WIDTH {
                        continue;
                    }
                    if (n_y + i) >= HEIGHT {
                        continue;
                    }
                    let index = 4 * (n_x + j + WIDTH * (n_y + i));
                    if !collision {
                        let check = frame[index] | frame[index + 1] | frame[index + 2];
                        if check > 0 {
                            collision = true;
                        }
                    }
                    frame[index] ^= 0xff;
                    frame[index + 1] ^= 0xff;
                    frame[index + 2] ^= 0xff;
                    frame[index + 3] = 0xff;
                }
            }
        }
        self.chip_8.set_collision(collision);
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window_attributes = Window::default_attributes().with_title("CHIP-8");
        let window = event_loop.create_window(window_attributes).unwrap();
        let size = window.inner_size();
        let surface_texture = SurfaceTexture::new(size.width, size.height, &window);
        let pixels = Pixels::new(WIDTH as u32, HEIGHT as u32, surface_texture).unwrap();
        self.window = Some(window);
        self.pixels = Some(pixels);
        let source = SignalGenerator::new(
            cpal::SampleRate(48000),
            220.0,
            rodio::source::Function::Triangle,
        );
        self.sink.append(source);
        self.sink.pause();
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::Resized(size) => {
                self.pixels
                    .as_mut()
                    .unwrap()
                    .resize_surface(size.width, size.height)
                    .unwrap();
            }
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(key_code),
                        state,
                        repeat: false,
                        ..
                    },
                is_synthetic: false,
                ..
            } => self.chip_8.handle_input(key_code, state),
            WindowEvent::RedrawRequested => self.main_loop(),
            _ => (),
        }
    }
}
