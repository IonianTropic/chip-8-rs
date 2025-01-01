use std::{collections::VecDeque, fs::File, io::prelude::*, path::Path, random::random};

use winit::{event::ElementState, keyboard::KeyCode};

use crate::{
    chip_8_variant::Chip8Variant,
    draw_job::{DrawJob, Sprite},
};

const MEMORY_LENGTH: usize = 4096;
const VRAM_LENGTH: usize = 256;
const ENTRY: usize = 0x200;
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

#[derive(Debug)]
pub struct Chip8 {
    draw_queue: VecDeque<DrawJob>,
    stack: Vec<u16>,
    register_file: [u8; 16],
    ir: u16,
    pc: u16,
    indirect: u16,
    delay_timer: u8,
    sound_timer: u8,
    memory: [u8; 4096],
    video_memory: [u8; 256],
    keyboard: [ElementState; 16],
    key_latch: Option<u8>,
    awaiting_key: bool,
    instr: InstructionDecode,
}

impl Chip8 {
    pub fn new<P>(path: P) -> Self
    where
        P: AsRef<Path>,
    {
        let mut memory = [0; MEMORY_LENGTH];
        let mut file = File::open(path).unwrap();
        memory[..80].copy_from_slice(&FONT);
        let _ = file.read(&mut memory[ENTRY..]).unwrap();
        memory[0x1FF] = 0; // quirk test specific
        Self {
            draw_queue: VecDeque::new(),
            stack: Vec::new(),
            register_file: [0; 16],
            ir: 0,
            pc: ENTRY as u16,
            indirect: 0,
            delay_timer: 0,
            sound_timer: 0,
            memory,
            video_memory: [0; VRAM_LENGTH],
            keyboard: [ElementState::Released; 16],
            key_latch: None,
            awaiting_key: false,
            instr: InstructionDecode::decode(0),
        }
    }
}

impl Chip8Variant for Chip8 {
    fn instruction_cycle(&mut self) {
        self.fetch();
        self.decode();
        self.execute();
    }

    fn decrement_timers(&mut self) {
        self.delay_timer = self.delay_timer.saturating_sub(1);
        self.sound_timer = self.sound_timer.saturating_sub(1);
    }

    fn handle_input(&mut self, key_code: KeyCode, state: ElementState) {
        if let Some(key) = match key_code {
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
        } {
            self.keyboard[key] = state;
            if self.awaiting_key {
                match self.key_latch {
                    Some(key_latch) => {
                        if key_latch == key as u8 {
                            self.register_file[self.instr.x] = key_latch;
                            self.awaiting_key = false;
                            self.key_latch = None;
                        }
                    }
                    None => self.key_latch = Some(key as u8),
                }
            }
        }
    }

    fn sound_timer(&self) -> u8 {
        self.sound_timer
    }

    fn waiting(&self) -> bool {
        self.awaiting_key
    }

    fn poll_draw_queue(&mut self) -> Option<DrawJob> {
        self.draw_queue.pop_front()
    }

    fn set_collision(&mut self, collides: bool) {
        self.register_file[0xF] = if collides { 1 } else { 0 }
    }
}

impl Chip8 {
    fn fetch(&mut self) {
        self.ir = u16::from_be_bytes(
            self.memory[self.pc as usize..self.pc as usize + 2]
                .try_into()
                .unwrap(),
        );
        self.pc += 2;
    }

    fn decode(&mut self) {
        self.instr = InstructionDecode::decode(self.ir);
    }

    fn execute(&mut self) {
        match self.instr.opcode {
            0x0 => match self.instr.address {
                0x0E0 => self.clear_screen(),
                0x0EE => self.ret(),
                _ => log::error!("Unknown instruction {:#06x}", self.ir),
            },
            0x1 => self.jump(self.instr.address),
            0x2 => self.call(self.instr.address),
            0x3 => self.skip_vx_e_imm(self.instr.x, self.instr.immediate),
            0x4 => self.skip_vx_ne_imm(self.instr.x, self.instr.immediate),
            0x5 => self.skip_vx_e_vy(self.instr.x, self.instr.y),
            0x6 => self.load_imm(self.instr.x, self.instr.immediate),
            0x7 => self.add_imm(self.instr.x, self.instr.immediate),
            0x8 => match self.instr.funct {
                0x0 => self.register_file[self.instr.x] = self.register_file[self.instr.y],
                0x1 => self.or_reg(self.instr.x, self.instr.y), // self.v_file[x] |= self.v_file[y],
                0x2 => self.and_reg(self.instr.x, self.instr.y), // self.v_file[x] &= self.v_file[y],
                0x3 => self.xor_reg(self.instr.x, self.instr.y), // self.v_file[x] ^= self.v_file[y],
                0x4 => self.add_reg(self.instr.x, self.instr.y),
                0x5 => self.sub_reg(self.instr.x, self.instr.y),
                0x6 => self.shr_reg(self.instr.x, self.instr.y),
                0x7 => self.subn_reg(self.instr.x, self.instr.y),
                0xE => self.shl_reg(self.instr.x, self.instr.y),
                _ => log::error!("Unknown instruction {:#06x}", self.ir),
            },
            0x9 => self.skip_vx_ne_vy(self.instr.x, self.instr.y),
            0xA => self.load_addr(self.instr.address),
            0xB => self.pc = self.instr.address + self.register_file[0] as u16,
            0xC => self.register_file[self.instr.x] = random::<u8>() & self.instr.immediate,
            0xD => self.draw_sprite(self.instr.x, self.instr.y, self.instr.funct),
            0xE => match self.instr.immediate {
                0x9E => self.skip_pressed(self.instr.x),
                0xA1 => self.skip_not_pressed(self.instr.x),
                _ => log::error!("Unknown instruction {:#06x}", self.ir),
            },
            0xF => match self.instr.immediate {
                0x07 => self.register_file[self.instr.x] = self.delay_timer,
                0x0A => self.get_key(self.instr.x),
                0x15 => self.delay_timer = self.register_file[self.instr.x],
                0x18 => self.load_sound_timer(self.instr.x),
                0x1E => self.indirect += self.register_file[self.instr.x] as u16,
                0x29 => self.load_hex_sprite(self.instr.x),
                0x33 => self.store_bcd(self.instr.x),
                0x55 => self.store_block(self.instr.x),
                0x65 => self.load_block(self.instr.x),
                _ => log::error!("Unknown instruction {:#06x}", self.ir),
            },
            _ => log::error!("Unknown instruction {:#06x}", self.ir),
        }
    }
}

impl Chip8 {
    fn clear_screen(&mut self) {
        self.draw_queue.push_back(DrawJob::Clear);
    }

    fn ret(&mut self) {
        self.pc = self.stack.pop().unwrap();
    }

    fn jump(&mut self, addr: u16) {
        self.pc = addr;
    }

    fn call(&mut self, addr: u16) {
        self.stack.push(self.pc);
        self.pc = addr;
    }

    fn skip_vx_e_imm(&mut self, x: usize, imm: u8) {
        if self.register_file[x] == imm {
            self.pc += 2;
        }
    }

    fn skip_vx_ne_imm(&mut self, x: usize, imm: u8) {
        if self.register_file[x] != imm {
            self.pc += 2;
        }
    }

    fn skip_vx_e_vy(&mut self, x: usize, y: usize) {
        if self.register_file[x] == self.register_file[y] {
            self.pc += 2;
        }
    }

    fn load_imm(&mut self, x: usize, imm: u8) {
        self.register_file[x] = imm;
    }

    fn add_imm(&mut self, x: usize, imm: u8) {
        self.register_file[x] = self.register_file[x].wrapping_add(imm);
    }

    fn or_reg(&mut self, x: usize, y: usize) {
        self.register_file[x] |= self.register_file[y];
        self.register_file[0xF] = 0;
    }

    fn and_reg(&mut self, x: usize, y: usize) {
        self.register_file[x] &= self.register_file[y];
        self.register_file[0xF] = 0;
    }

    fn xor_reg(&mut self, x: usize, y: usize) {
        self.register_file[x] ^= self.register_file[y];
        self.register_file[0xF] = 0;
    }

    fn add_reg(&mut self, x: usize, y: usize) {
        let (result, carry) = self.register_file[x].overflowing_add(self.register_file[y]);
        self.register_file[x] = result;
        self.register_file[0xF] = if carry { 1 } else { 0 };
    }

    fn sub_reg(&mut self, x: usize, y: usize) {
        let (result, borrow) = self.register_file[x].overflowing_sub(self.register_file[y]);
        self.register_file[x] = result;
        self.register_file[0xF] = if borrow { 0 } else { 1 };
    }

    fn shr_reg(&mut self, x: usize, y: usize) {
        let v_y = self.register_file[y];
        self.register_file[x] = self.register_file[y].wrapping_shr(1);
        self.register_file[0xF] = v_y & 1;
    }

    fn subn_reg(&mut self, x: usize, y: usize) {
        let (result, carry) = self.register_file[y].overflowing_sub(self.register_file[x]);
        self.register_file[x] = result;
        self.register_file[0xF] = if carry { 0 } else { 1 };
    }

    fn shl_reg(&mut self, x: usize, y: usize) {
        let v_y = self.register_file[y];
        self.register_file[x] = self.register_file[y].wrapping_shl(1);
        self.register_file[0xF] = v_y >> 7;
    }

    fn load_addr(&mut self, addr: u16) {
        self.indirect = addr;
    }

    fn skip_vx_ne_vy(&mut self, x: usize, y: usize) {
        if self.register_file[x] != self.register_file[y] {
            self.pc += 2;
        }
    }

    fn draw_sprite(&mut self, x: usize, y: usize, n: usize) {
        let slice = &self.memory[self.indirect as usize..self.indirect as usize + n];
        let buf = slice.to_vec();
        let v_x = self.register_file[x] as usize;
        let v_y = self.register_file[y] as usize;
        let job = DrawJob::Draw(Sprite { v_x, v_y, buf });
        self.draw_queue.push_back(job);
    }

    fn skip_pressed(&mut self, x: usize) {
        if self.keyboard[self.register_file[x] as usize & 0xF].is_pressed() {
            self.pc += 2;
        }
    }

    fn skip_not_pressed(&mut self, x: usize) {
        if !self.keyboard[self.register_file[x] as usize & 0xF].is_pressed() {
            self.pc += 2;
        }
    }

    fn get_key(&mut self, _x: usize) {
        self.awaiting_key = true;
    }

    fn load_sound_timer(&mut self, x: usize) {
        self.sound_timer = self.register_file[x];
    }

    fn load_hex_sprite(&mut self, x: usize) {
        self.indirect = 5 * (self.register_file[self.instr.x] as u16 & 0x00FF);
    }

    fn store_bcd(&mut self, x: usize) {
        let mut num = self.register_file[x];
        for j in (0..3).rev() {
            self.memory[self.indirect as usize + j] = num % 10;
            num /= 10;
        }
    }

    fn store_block(&mut self, x: usize) {
        self.memory[self.indirect as usize..self.indirect as usize + x + 1]
            .copy_from_slice(&self.register_file[..x + 1]);
        self.indirect += x as u16 + 1;
    }

    fn load_block(&mut self, x: usize) {
        self.register_file[..x + 1]
            .copy_from_slice(&self.memory[self.indirect as usize..self.indirect as usize + x + 1]);
        self.indirect += x as u16 + 1;
    }
}

#[derive(Debug)]
struct InstructionDecode {
    pub opcode: u8,
    pub x: usize, // usize clarfies that this value is only used to write to regfile
    pub y: usize,
    pub funct: usize, // usize for draw jobs
    pub immediate: u8,
    pub address: u16,
}

impl InstructionDecode {
    pub fn decode(instruction: u16) -> Self {
        let opcode = (instruction >> 12) as u8;
        let x = ((instruction & 0x0F00) >> 8) as usize;
        let y = ((instruction & 0x00F0) >> 4) as usize;
        let funct = (instruction & 0x000F) as usize;
        let immediate = (instruction & 0x00FF) as u8;
        let address = instruction & 0x0FFF;
        Self {
            opcode,
            x,
            y,
            funct,
            immediate,
            address,
        }
    }
}
