use std::fmt::Debug;

use winit::{event::ElementState, keyboard::KeyCode};

use crate::draw_job::DrawJob;

pub trait Chip8Variant: Debug {
    fn instruction_cycle(&mut self);
    fn decrement_timers(&mut self);
    fn handle_input(&mut self, key_code: KeyCode, state: ElementState);
    fn sound_timer(&self) -> u8;
    fn waiting(&self) -> bool;
    fn poll_draw_queue(&mut self) -> Option<DrawJob>;
    fn set_collision(&mut self, value: bool);
}
