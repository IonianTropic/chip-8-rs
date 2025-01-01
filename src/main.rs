#![feature(random)]
#![allow(clippy::precedence)]

use std::{fs::File, time::UNIX_EPOCH};

use app::App;
use env_logger::Target;
use winit::event_loop::{ControlFlow, EventLoop};

mod app;
mod chip_8;
mod chip_8_variant;
mod draw_job;

fn main() {
    init_logger();

    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Poll);

    let rom_path = std::env::args()
        .nth(1)
        .expect("rom path should be specified");

    let mut app = App::new(rom_path);
    event_loop.run_app(&mut app).unwrap();
}

fn init_logger() {
    let log_id = UNIX_EPOCH.elapsed().expect("time travel").as_secs();
    let target_path = format!("logs/log-{}.txt", log_id);
    let target = Target::Pipe(Box::new(File::create(target_path).unwrap()));
    env_logger::builder().target(target).init();
}
