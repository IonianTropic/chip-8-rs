#![feature(random)]
#![allow(clippy::precedence)]

use app::App;
use winit::event_loop::{ControlFlow, EventLoop};

mod app;

fn main() {
    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = App::new();
    event_loop.run_app(&mut app).unwrap();
}
