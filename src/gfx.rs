use crate::{Chip, SCREEN_WIDTH, SCREEN_HEIGHT, ChipException};

use sdl2::pixels::Color;
use sdl2::render::Canvas;
use sdl2::video::Window;
use sdl2::event::Event;
use sdl2::EventPump;
use sdl2::keyboard::Keycode;
use sdl2::rect::Rect;

use std::time::Duration;
use std::thread;

const WINDOW_WIDTH: u32 = 1024;
const WINDOW_HEIGHT: u32 = 512;

const CELL_HEIGHT: u32 = WINDOW_HEIGHT / SCREEN_HEIGHT;
const CELL_WIDTH: u32 = WINDOW_WIDTH / SCREEN_WIDTH;

const CYCLES_PER_FRAME: u32 = 20;

fn draw_grid(canvas: &mut Canvas<Window>, chip: &Chip) {
    for row in 0..SCREEN_HEIGHT {
        for col in 0..SCREEN_WIDTH {
            let idx = (row * SCREEN_WIDTH + col) as usize;

            // screen cell is active, color white
            if chip.video_memory[idx] == 1 {
                let _ = canvas.fill_rect(Rect::new(
                    (col * CELL_WIDTH) as i32,
                    (row * CELL_HEIGHT) as i32, 
                    CELL_WIDTH, CELL_HEIGHT));
            } 
        }
    }
}

fn freeze(mut events: EventPump) -> ! {
    loop {
        match events.wait_event() {
            Event::Quit { .. } |
            Event::KeyDown { keycode: Some(Keycode::Q), .. } => {
                std::process::exit(0);
            }
            _ => {}
        }
    }
}

fn pause(events: &mut EventPump) {
    loop {
        match events.wait_event() {
            Event::Quit { .. } |
            Event::KeyDown { keycode: Some(Keycode::Escape), .. } => {
                std::process::exit(0);
            }
            Event::KeyDown { keycode: Some(Keycode::P), .. } => {
                return
            }
            _ => {}
        }
    }
}

fn wait_for_key(chip: &mut Chip, register: u8, events: &mut EventPump) {
    loop {
        match events.wait_event() {
            Event::Quit { .. } |
            Event::KeyDown { keycode: Some(Keycode::Escape), .. } => {
                std::process::exit(0);
            }
            Event::KeyDown { keycode, .. }  => {
                if let Some(key) = keycode.and_then(|key| u8::from_str_radix(&key.to_string(), 16).ok()) {
                    chip.data_regs[register as usize] = key;
                    chip.ip += 2;
                    return;
                }
            }
            _ => {}
        }
    }
}

const KEY_MAP: [&str; 16] = [
    "X", "1", "2", "3",
    "Q", "W", "E", "A",
    "S", "D", "Z", "C",
    "4", "R", "F", "V",
];
 
pub fn spawn_window(mut chip: Chip) {
    let sdl_context = sdl2::init().unwrap();
    let video_subsystem = sdl_context.video().unwrap();
 
    let window = video_subsystem.window("Chip-8 Emulator", WINDOW_WIDTH, WINDOW_HEIGHT)
        .position_centered()
        .build()
        .unwrap();

    let mut key_matrix: [bool; 16] = [false; 16];
 
    let mut canvas = window.into_canvas().build().unwrap();
    let mut event_pump = sdl_context.event_pump().unwrap();
    'running: loop {
        let mut p = false;

        for event in event_pump.poll_iter() {
            match event {
                Event::Quit {..} |
                Event::KeyDown { keycode: Some(Keycode::Escape), .. } => {
                    break 'running
                },
                Event::KeyDown { keycode: Some(Keycode::P), .. } => {
                    p = true; 
                },
                Event::KeyDown { keycode, .. } => {
                    if let Some(key) = keycode.map(|key| key.to_string()) {
                        println!("press: {key}");
                        if let Some(idx) = KEY_MAP.iter().position(|x| key.eq(x)) {
                            key_matrix[idx] = true;
                        }
                    }
                    //if let Some(key) = keycode.and_then(|key| u8::from_str_radix(&key.to_string(), 16).ok()) {
                    //    key_matrix[key as usize] = true;
                    //}
                },
                Event::KeyUp { keycode, .. } => {
                    if let Some(key) = keycode.map(|key| key.to_string()) {
                        if let Some(idx) = KEY_MAP.iter().position(|x| key.eq(x)) {
                            key_matrix[idx] = false;
                        }
                    }
                },
                _ => {}
            }
        }
        if p {
            pause(&mut event_pump);
        }

        canvas.set_draw_color(Color::RGB(18, 18, 18));
        canvas.clear();

        // println!("{key_matrix:#?}");

        for _ in 0..CYCLES_PER_FRAME {
            match chip.cycle() {
                Err(ChipException::WaitForKey { register }) => wait_for_key(&mut chip, register, &mut event_pump),
                Err(ChipException::SkipIfPressed { register }) => {
                    if key_matrix[chip.data_regs[register as usize] as usize] {
                        chip.ip += 2;
                    }
                }
                Err(ChipException::SkipIfNotPressed { register }) => {
                    if !key_matrix[chip.data_regs[register as usize] as usize] {
                        chip.ip += 2;
                    }
                }
                Err(e) => {
                    println!("chip8 runtime exception: {e:?}");
                    freeze(event_pump);
                }
                Ok(()) => {},
            }
        }

        canvas.set_draw_color(Color::RGB(255,255,255));
        draw_grid(&mut canvas, &chip);

        canvas.present();
        chip.delay_timer = chip.delay_timer.saturating_sub(1);
        chip.sound_timer = chip.delay_timer.saturating_sub(1);
        thread::sleep(Duration::from_millis(1000 / 60));
    }
}
