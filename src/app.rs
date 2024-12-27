use pixels::{Pixels, SurfaceTexture};
use rodio::{source::SignalGenerator, OutputStream, Sink};
use std::{
    fs::File,
    io::prelude::*,
    random::random,
    time::{Duration, Instant},
};
use winit::{
    application::ApplicationHandler,
    event::{ElementState, KeyEvent, WindowEvent},
    event_loop::ActiveEventLoop,
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};

const MLEN: usize = 4096;
const START: usize = 0x200;
const WIDTH: usize = 64;
const HEIGHT: usize = 32;
const REFRESH_DURATION: Duration = Duration::from_micros(16667); // 16667
const CYCLE_DURATION: Duration = Duration::from_micros(1429); // 1429
const FONT: [u8; 80] = [
    0xF0, 0x90, 0x90, 0x90, 0xF0, // 0
    0x20, 0x60, 0x20, 0x20, 0x70, // 1
    0xF0, 0x10, 0xF0, 0x80, 0xF0, // 2
    0xF0, 0x10, 0xF0, 0x10, 0xF0, // 3
    0x90, 0x90, 0xF0, 0x10, 0x10, // 4
    0xF0, 0x80, 0xF0, 0x10, 0xF0, // 5
    0xF0, 0x80, 0xF0, 0x90, 0xF0, // 6
    0xF0, 0x10, 0x20, 0x40, 0x40, // 7
    0xF0, 0x90, 0xF0, 0x90, 0xF0, // 8
    0xF0, 0x90, 0xF0, 0x10, 0xF0, // 9
    0xF0, 0x90, 0xF0, 0x90, 0x90, // A
    0xE0, 0x90, 0xE0, 0x90, 0xE0, // B
    0xF0, 0x80, 0x80, 0x80, 0xF0, // C
    0xE0, 0x90, 0x90, 0x90, 0xE0, // D
    0xF0, 0x80, 0xF0, 0x80, 0xF0, // E
    0xF0, 0x80, 0xF0, 0x80, 0x80, // F
];

pub struct App {
    window: Option<Window>,
    pixels: Option<Pixels>,
    _stream: OutputStream,
    sink: Sink,
    refresh_timer: Instant,
    cycle_timer: Instant,
    key_wait: KeyWaitState,
    memory: [u8; MLEN],
    pc: u16,
    sp: u8,
    i: u16,
    v_file: [u8; 16],
    keypad: [ElementState; 16],
    delay_timer: u8,
    sound_timer: u8,
}

#[derive(Debug)]
enum KeyWaitState {
    Idle,
    Waiting,
    Pressed(u8),
    Released(u8),
}

impl App {
    pub fn new() -> Self {
        let (_stream, stream_handle) = OutputStream::try_default().unwrap();
        let sink = Sink::try_new(&stream_handle).unwrap();
        let mut memory = [0; MLEN];
        let mut file = File::open("./roms/demos/octojam1title.ch8").unwrap();
        memory[..80].copy_from_slice(&FONT);
        let _ = file.read(&mut memory[START..MLEN]).unwrap();
        // quirk test specific
        memory[0x1FF] = 0;
        Self {
            window: None,
            pixels: None,
            _stream,
            sink,
            refresh_timer: Instant::now(),
            cycle_timer: Instant::now(),
            key_wait: KeyWaitState::Idle,
            memory,
            pc: 0x200,
            i: 0,
            sp: 0,
            v_file: [0; 16],
            keypad: [ElementState::Released; 16],
            delay_timer: 0,
            sound_timer: 0,
        }
    }
}

// private
impl App {
    fn instruction_cycle(&mut self) {
        let instr = u16::from_be_bytes(
            self.memory[self.pc as usize..self.pc as usize + 2]
                .try_into()
                .unwrap(),
        );
        self.pc += 2;
        let opcode = instr >> 12;
        let x = ((instr & 0x0F00) >> 8) as usize;
        let y = ((instr & 0x00F0) >> 4) as usize;
        let funct = instr & 0x000F;
        let imm = (instr & 0x00FF) as u8;
        let addr = instr & 0x0FFF;
        match opcode {
            0x0 => match addr {
                0x0E0 => self.clear_screen(),
                0x0EE => self.ret(),
                _ => println!("Unknown machine subroutine {:#5x}", addr),
            },
            0x1 => self.jump(addr),
            0x2 => self.call(addr),
            0x3 => self.skip_vx_e_imm(x, imm),
            0x4 => self.skip_vx_ne_imm(x, imm),
            0x5 => self.skip_vx_e_vy(x, y),
            0x6 => self.load_imm(x, imm),
            0x7 => self.add_imm(x, imm),
            0x8 => match funct {
                0x0 => self.v_file[x] = self.v_file[y],
                0x1 => self.or_reg(x, y), // self.v_file[x] |= self.v_file[y],
                0x2 => self.and_reg(x, y), // self.v_file[x] &= self.v_file[y],
                0x3 => self.xor_reg(x, y), // self.v_file[x] ^= self.v_file[y],
                0x4 => self.add_reg(x, y),
                0x5 => self.sub_reg(x, y),
                0x6 => self.shr_reg(x, y),
                0x7 => self.subn_reg(x, y),
                0xE => self.shl_reg(x, y),
                _ => println!("Unknown ALU funct {:#3x}", funct),
            },
            0x9 => self.skip_vx_ne_vy(x, y),
            0xA => self.load_addr(addr),
            0xB => self.pc = addr + self.v_file[0] as u16,
            0xC => self.v_file[x] = random::<u8>() & imm,
            0xD => self.draw_sprite(x, y, funct),
            0xE => match imm {
                0x9E => self.skip_pressed(x),
                0xA1 => self.skip_not_pressed(x),
                _ => println!("Unknown keypad subroutine {:#4x}", imm),
            },
            0xF => match imm {
                0x07 => self.v_file[x] = self.delay_timer,
                0x0A => self.await_key(x),
                0x15 => self.delay_timer = self.v_file[x],
                0x18 => self.load_sound_timer(x),
                0x1E => self.i += self.v_file[x] as u16,
                0x29 => self.i = 5 * (self.v_file[x] as u16 & 0x00FF),
                0x33 => self.store_bcd(x),
                0x55 => self.store_block(x),
                0x65 => self.load_block(x),
                _ => println!("Unknown control subroutine {:#4x}", imm),
            },
            _ => println!("Unknown opcode {:#3x}", opcode),
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

    fn ret(&mut self) {
        self.sp -= 2;
        let addr = u16::from_be_bytes(
            self.memory[self.sp as usize..self.sp as usize + 2]
                .try_into()
                .unwrap(),
        );
        self.pc = addr;
    }

    fn jump(&mut self, addr: u16) {
        self.pc = addr;
    }

    fn call(&mut self, addr: u16) {
        self.memory[self.sp as usize] = (self.pc >> 8) as u8;
        self.memory[self.sp as usize + 1] = (self.pc & 0x00FF) as u8;
        self.sp += 2;
        self.pc = addr;
    }

    fn skip_vx_e_imm(&mut self, x: usize, imm: u8) {
        if self.v_file[x] == imm {
            self.pc += 2;
        }
    }

    fn skip_vx_ne_imm(&mut self, x: usize, imm: u8) {
        if self.v_file[x] != imm {
            self.pc += 2;
        }
    }

    fn skip_vx_e_vy(&mut self, x: usize, y: usize) {
        if self.v_file[x] == self.v_file[y] {
            self.pc += 2;
        }
    }

    fn load_imm(&mut self, x: usize, imm: u8) {
        self.v_file[x] = imm;
    }

    fn add_imm(&mut self, x: usize, imm: u8) {
        self.v_file[x] = self.v_file[x].wrapping_add(imm);
    }

    fn or_reg(&mut self, x: usize, y: usize) {
        self.v_file[x] |= self.v_file[y];
        self.v_file[0xF] = 0;
    }

    fn and_reg(&mut self, x: usize, y: usize) {
        self.v_file[x] &= self.v_file[y];
        self.v_file[0xF] = 0;
    }

    fn xor_reg(&mut self, x: usize, y: usize) {
        self.v_file[x] ^= self.v_file[y];
        self.v_file[0xF] = 0;
    }

    fn add_reg(&mut self, x: usize, y: usize) {
        let (result, carry) = self.v_file[x].overflowing_add(self.v_file[y]);
        self.v_file[x] = result;
        self.v_file[0xF] = if carry { 1 } else { 0 };
    }

    fn sub_reg(&mut self, x: usize, y: usize) {
        let (result, borrow) = self.v_file[x].overflowing_sub(self.v_file[y]);
        self.v_file[x] = result;
        self.v_file[0xF] = if borrow { 0 } else { 1 };
    }

    fn shr_reg(&mut self, x: usize, y: usize) {
        let v_y = self.v_file[y];
        self.v_file[x] = self.v_file[y].wrapping_shr(1);
        self.v_file[0xF] = v_y & 1;
    }

    fn subn_reg(&mut self, x: usize, y: usize) {
        let (result, carry) = self.v_file[y].overflowing_sub(self.v_file[x]);
        self.v_file[x] = result;
        self.v_file[0xF] = if carry { 0 } else { 1 };
    }

    fn shl_reg(&mut self, x: usize, y: usize) {
        let v_y = self.v_file[y];
        self.v_file[x] = self.v_file[y].wrapping_shl(1);
        self.v_file[0xF] = v_y >> 7;
    }

    fn load_addr(&mut self, addr: u16) {
        self.i = addr;
    }

    fn skip_vx_ne_vy(&mut self, x: usize, y: usize) {
        if self.v_file[x] != self.v_file[y] {
            self.pc += 2;
        }
    }

    fn draw_sprite(&mut self, x: usize, y: usize, funct: u16) {
        let v_x = self.v_file[x];
        let v_y = self.v_file[y];
        let n_x = v_x as usize % WIDTH;
        let n_y = v_y as usize % HEIGHT;
        let mut flipped = false;

        let frame = self.pixels.as_mut().unwrap().frame_mut();

        for (i, row) in self.memory[self.i as usize..(self.i + funct) as usize]
            .iter()
            .enumerate()
        {
            for j in 0..8 {
                if row & (1 << (7 - j) >> (7 - j)) == 1 {
                    // flip (x+j, y+i) -> 4*(x+j+width*(y+i))
                    if (n_x + j) >= WIDTH {
                        continue;
                    }
                    if (n_y + i) >= HEIGHT {
                        continue;
                    }
                    let index = 4 * (n_x + j + WIDTH * (n_y + i));
                    if !flipped {
                        let check = frame[index] & 0xFF | frame[index + 1] & 0xFF | frame[index + 2] & 0xFF;
                        if check > 0 {
                            flipped = true;
                        }
                    }
                    frame[index] ^= 0xff;
                    frame[index + 1] ^= 0xff;
                    frame[index + 2] ^= 0xff;
                    frame[index + 3] = 0xff;
                }
            }
        }
        self.v_file[0xF] = if flipped { 1 } else { 0 };
    }

    fn skip_pressed(&mut self, x: usize) {
        if self.keypad[self.v_file[x] as usize & 0xF].is_pressed() {
            self.pc += 2;
        }
    }

    fn skip_not_pressed(&mut self, x: usize) {
        if !self.keypad[self.v_file[x] as usize & 0xF].is_pressed() {
            self.pc += 2;
        }
    }

    fn await_key(&mut self, x: usize) {
        self.pc -= 2;
        match self.key_wait {
            KeyWaitState::Idle => {
                self.key_wait = KeyWaitState::Waiting;
            }
            KeyWaitState::Released(key) => {
                self.pc += 2;
                self.v_file[x] = key;
                self.key_wait = KeyWaitState::Idle;
            }
            _ => (),
        }
    }

    fn load_sound_timer(&mut self, x: usize) {
        self.sound_timer = self.v_file[x];
        if self.sound_timer > 0 {
            self.sink.play();
        }
    }

    fn store_bcd(&mut self, x: usize) {
        let mut num = self.v_file[x];
        for j in (0..3).rev() {
            self.memory[self.i as usize + j] = num % 10;
            num /= 10;
        }
    }

    fn store_block(&mut self, x: usize) {
        self.memory[self.i as usize..self.i as usize + x + 1]
            .copy_from_slice(&self.v_file[..x + 1]);
        self.i += x as u16 + 1;
    }

    fn load_block(&mut self, x: usize) {
        self.v_file[..x + 1]
            .copy_from_slice(&self.memory[self.i as usize..self.i as usize + x + 1]);
        self.i += x as u16 + 1;
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
                        physical_key: PhysicalKey::Code(code),
                        state,
                        repeat: false,
                        ..
                    },
                is_synthetic: false,
                ..
            } => {
                let key = match code {
                    KeyCode::KeyX => Some(0),
                    KeyCode::Digit1 => Some(1),
                    KeyCode::Digit2 => Some(2),
                    KeyCode::Digit3 => Some(3),
                    KeyCode::KeyQ => Some(4),
                    KeyCode::KeyW => Some(5),
                    KeyCode::KeyE => Some(6),
                    KeyCode::KeyA => Some(7),
                    KeyCode::KeyS => Some(8),
                    KeyCode::KeyD => Some(9),
                    KeyCode::KeyZ => Some(0xA),
                    KeyCode::KeyC => Some(0xB),
                    KeyCode::Digit4 => Some(0xC),
                    KeyCode::KeyR => Some(0xD),
                    KeyCode::KeyF => Some(0xE),
                    KeyCode::KeyV => Some(0xF),
                    _ => None,
                };
                if let Some(key_code) = key {
                    self.keypad[key_code] = state;
                    match self.key_wait {
                        KeyWaitState::Waiting => {
                            if state.is_pressed() {
                                self.key_wait = KeyWaitState::Pressed(key_code as u8);
                            }
                        }
                        KeyWaitState::Pressed(pcode) => {
                            if !state.is_pressed() && key_code as u8 == pcode {
                                self.key_wait = KeyWaitState::Released(pcode);
                            }
                        }
                        _ => (),
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                if self.cycle_timer.elapsed() >= CYCLE_DURATION {
                    self.cycle_timer = Instant::now();
                    self.instruction_cycle();
                }

                if self.refresh_timer.elapsed() >= REFRESH_DURATION {
                    self.refresh_timer = Instant::now();
                    self.delay_timer = self.delay_timer.saturating_sub(1);
                    self.sound_timer = self.sound_timer.saturating_sub(1);
                    if self.sound_timer == 0 {
                        self.sink.pause();
                    }
                    self.pixels.as_ref().unwrap().render().unwrap();
                }

                self.window.as_ref().unwrap().request_redraw();
            }
            _ => (),
        }
    }
}
