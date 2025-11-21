mod cpu;

use pixels::{Pixels, SurfaceTexture};
use winit::{
    dpi::LogicalSize,
    event::{Event, VirtualKeyCode, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

const WIDTH: u32 = 720;
const HEIGHT: u32 = 576;

fn draw(pixels: &mut Pixels) {
    let frame = pixels.frame_mut();

    // Fill entire buffer to black
    for pixel in frame.chunks_exact_mut(4) {
        pixel[0] = 0x00; // R
        pixel[1] = 0x00; // G
        pixel[2] = 0x00; // B
        pixel[3] = 0xff; // A
    }

    // Draw white rectangle
    let rect_x = 50;
    let rect_y = 50;
    let rect_width = 100;
    let rect_height = 80;

    for y in rect_y..(rect_y + rect_height) {
        for x in rect_x..(rect_x + rect_width) {
            if x < WIDTH && y < HEIGHT {
                let i = ((y * WIDTH + x) * 4) as usize;
                frame[i] = 0xff; // R
                frame[i + 1] = 0xff; // G
                frame[i + 2] = 0xff; // B
                frame[i + 3] = 0xff; // A
            }
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut cpu = cpu::Cpu::new();
    let program = vec![cpu::LDA_IMM, 0x42, cpu::BRK];
    cpu.load_and_run(program);
    let event_loop = EventLoop::new();

    let window = WindowBuilder::new()
        .with_title("NESER - NES Emulator in Rust")
        .with_inner_size(LogicalSize::new(WIDTH as f64, HEIGHT as f64))
        .build(&event_loop)
        .unwrap();

    let mut pixels = {
        let window_size = window.inner_size();
        let surface_texture = SurfaceTexture::new(window_size.width, window_size.height, &window);
        Pixels::new(WIDTH, HEIGHT, surface_texture)?
    };

    event_loop.run(move |event, _, control_flow| match event {
        Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        }
        | Event::WindowEvent {
            event:
                WindowEvent::KeyboardInput {
                    input:
                        winit::event::KeyboardInput {
                            virtual_keycode: Some(VirtualKeyCode::Escape),
                            ..
                        },
                    ..
                },
            ..
        } => {
            *control_flow = ControlFlow::Exit;
        }
        Event::WindowEvent {
            event: WindowEvent::Resized(size),
            ..
        } => {
            if let Err(_err) = pixels.resize_surface(size.width, size.height) {
                *control_flow = ControlFlow::Exit;
            }
        }
        Event::RedrawRequested(_) => {
            draw(&mut pixels);
            if let Err(_err) = pixels.render() {
                *control_flow = ControlFlow::Exit;
            }
        }
        Event::MainEventsCleared => {
            window.request_redraw();
        }
        _ => (),
    });
}
