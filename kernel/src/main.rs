#![feature(sync_unsafe_cell)]
#![feature(abi_x86_interrupt)]
#![no_std] // don't link the Rust standard library
#![no_main] // disable all Rust-level entry points

extern crate alloc;

mod screen;
mod allocator;
mod frame_allocator;
mod interrupts;
mod gdt;

use core::fmt::Write;
use core::slice;
use bootloader_api::{entry_point, BootInfo, BootloaderConfig};
use bootloader_api::config::Mapping::Dynamic;
use bootloader_api::info::MemoryRegionKind;
use kernel::{HandlerTable, serial};
use pc_keyboard::{DecodedKey, KeyCode};
use x86_64::registers::control::Cr3;
use x86_64::VirtAddr;
use crate::frame_allocator::BootInfoFrameAllocator;
use crate::screen::{ScreenWriter, screenwriter, draw_paddle, draw_ball, draw_center_line, draw_score};

// Game Variables
static mut SCREEN_WIDTH: usize = 0;
static mut SCREEN_HEIGHT: usize = 0;

const PADDLE_WIDTH: usize = 15;  
const PADDLE_HEIGHT: usize = 100; 
const BALL_SIZE: usize = 12;       
const PADDLE_SPEED: usize = 50; 

// Player 1 
static mut PLAYER1_PADDLE_Y: usize = 0; 
const PLAYER1_PADDLE_X: usize = 30; 

// Player 2 
static mut PLAYER2_PADDLE_Y: usize = 0; 
static mut PLAYER2_PADDLE_X: usize = 0; 

// Ball Position and Velocity
static mut BALL_X: usize = 0; 
static mut BALL_Y: usize = 0; 
static mut BALL_VEL_X: isize = 10;  
static mut BALL_VEL_Y: isize = 10;

// Player Scores
static mut PLAYER1_SCORE: usize = 0; 
static mut PLAYER2_SCORE: usize = 0;

// Game State
#[derive(PartialEq)]
enum GameState {
    StartScreen,
    Playing,
    GameOver
}
static mut GAME_STATE: GameState = GameState::StartScreen;
const WINNING_SCORE: usize = 5;


const BOOTLOADER_CONFIG: BootloaderConfig = {
    let mut config = BootloaderConfig::new_default();
    config.mappings.physical_memory = Some(Dynamic); // obtain physical memory offset
    config.kernel_stack_size = 256 * 1024; // 256 KiB kernel stack size
    config
};
entry_point!(kernel_main, config = &BOOTLOADER_CONFIG);


fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    writeln!(serial(), "Entered kernel with boot info: {boot_info:?}").unwrap();
    writeln!(serial(), "Frame Buffer: {:p}", boot_info.framebuffer.as_ref().unwrap().buffer()).unwrap();

    
    let frame_info = boot_info.framebuffer.as_ref().unwrap().info();
    let framebuffer = boot_info.framebuffer.as_mut().unwrap();
    screen::init(framebuffer);

    for r in boot_info.memory_regions.iter() {
        writeln!(serial(), "{:?} {:?} {:?} {}", r, r.start as *mut u8, r.end as *mut usize, r.end-r.start).unwrap();
    }

    // Initialize screen dimensions
    unsafe {
        SCREEN_WIDTH = frame_info.width as usize;
        SCREEN_HEIGHT = frame_info.height as usize;
        
        PLAYER1_PADDLE_Y = SCREEN_HEIGHT / 2 - PADDLE_HEIGHT / 2;
        PLAYER2_PADDLE_X = SCREEN_WIDTH - PADDLE_WIDTH - 30;
        PLAYER2_PADDLE_Y = SCREEN_HEIGHT / 2 - PADDLE_HEIGHT / 2;
        
        BALL_X = SCREEN_WIDTH / 2;
        BALL_Y = SCREEN_HEIGHT / 2;
    }

    let usable_region = boot_info.memory_regions.iter().filter(|x|x.kind == MemoryRegionKind::Usable).last().unwrap();
    writeln!(serial(), "{usable_region:?}").unwrap();

    let physical_offset = boot_info.physical_memory_offset.take().expect("Failed to find physical memory offset");
    let ptr = (physical_offset + usable_region.start) as *mut u8;
    writeln!(serial(), "Physical memory offset: {:X}; usable range: {:p}", physical_offset, ptr).unwrap();

    // print out values stored in specific memory address
    let vault = unsafe { slice::from_raw_parts_mut(ptr, 100) };
    vault[0] = 65;
    vault[1] = 66;
    //writeln!(Writer, "{} {}", vault[0] as char, vault[1] as char).unwrap();

    //read CR3 for current page table
    let cr3 = Cr3::read().0.start_address().as_u64();
    writeln!(serial(), "CR3 read: {:#x}", cr3).unwrap();

    let cr3_page = unsafe { slice::from_raw_parts_mut((cr3 + physical_offset) as *mut usize, 6) };
    writeln!(serial(), "CR3 Page table virtual address {cr3_page:#p}").unwrap();

    allocator::init_heap((physical_offset + usable_region.start) as usize);

    let rsdp = boot_info.rsdp_addr.take();
    let mut mapper = frame_allocator::init(VirtAddr::new(physical_offset));
    let mut frame_allocator = BootInfoFrameAllocator::new(&boot_info.memory_regions);
    
    gdt::init();

    let lapic_ptr = interrupts::init_apic(rsdp.expect("Failed to get RSDP address") as usize, physical_offset, &mut mapper, &mut frame_allocator);
    HandlerTable::new()
        .keyboard(key)
        .timer(tick)
        .startup(start)
        .start(lapic_ptr)
}

fn start() {

    unsafe {
        let writer = screenwriter();
        writer.clear();
        GAME_STATE = GameState::StartScreen;
        draw_start_screen(writer);
    }
}

fn draw_start_screen(writer: &mut ScreenWriter) {
    let screen_width = unsafe { SCREEN_WIDTH };
    let screen_height = unsafe { SCREEN_HEIGHT };

    writer.write_large_text("PONG", screen_width / 2 - 60, screen_height / 3, 255, 255, 255);
    writer.write_large_text("Press SPACE to Start", screen_width / 2 - 200, screen_height / 2, 255, 255, 255);
}

fn draw_game_over_screen(writer: &mut ScreenWriter) {
    let screen_width = unsafe { SCREEN_WIDTH };
    let screen_height = unsafe { SCREEN_HEIGHT };
    writer.write_large_text("GAME OVER", screen_width / 2 - 120, screen_height / 3, 255, 255, 255);
    let winner_text = if unsafe { PLAYER1_SCORE >= WINNING_SCORE } {
        "Player 1 Wins"
    } else {
        "Player 2 Wins"
    };
    writer.write_large_text(winner_text, screen_width / 2 - 150, screen_height / 2, 255, 255, 255);
    writer.write_large_text("Press SPACE to Restart", screen_width / 2 - 200, screen_height / 2 + 100, 255, 255, 255);
}

fn init_game() {
    unsafe {
        let writer = screenwriter();

        let screen_width = SCREEN_WIDTH;
        let screen_height = SCREEN_HEIGHT;

        BALL_X = screen_width / 2;
        BALL_Y = screen_height / 2;

        PLAYER1_PADDLE_Y = screen_height / 2 - PADDLE_HEIGHT / 2;
        PLAYER2_PADDLE_Y = screen_height / 2 - PADDLE_HEIGHT / 2;

        PLAYER1_SCORE = 0;
        PLAYER2_SCORE = 0;

        BALL_VEL_X = 10;  
        BALL_VEL_Y = 10;

        GAME_STATE = GameState::Playing;

        writer.clear();
        draw_paddle(writer, PLAYER1_PADDLE_X, PLAYER1_PADDLE_Y, 255, 255, 255);
        draw_paddle(writer, PLAYER2_PADDLE_X, PLAYER2_PADDLE_Y, 255, 255, 255);
    }
}

fn reset_ball() {
    unsafe {
        BALL_X = SCREEN_WIDTH / 2;
        BALL_Y = SCREEN_HEIGHT / 2;

        BALL_VEL_X = if BALL_VEL_X > 0 { -10 } else { 10 };
        BALL_VEL_Y = if BALL_VEL_Y > 0 { -10 } else { 10 };
    }
}


fn tick() {
    unsafe {
        let writer = screenwriter();

        match GAME_STATE {
            GameState::StartScreen => {
                draw_start_screen(writer);
            }
            GameState::Playing => {
                let ball_x = BALL_X;
                let ball_y = BALL_Y;
                draw_ball(writer, ball_x, ball_y, 0, 0, 0);
                BALL_X = (BALL_X as isize + BALL_VEL_X) as usize;
                BALL_Y = (BALL_Y as isize + BALL_VEL_Y) as usize;

                const PADDLE_BUFFER: usize = 8; 

                
                // Ball collision with walls (top/bottom)
                if BALL_Y <= 0 || BALL_Y + BALL_SIZE >= SCREEN_HEIGHT {
                    BALL_VEL_Y = -BALL_VEL_Y;
                }
            
                // Ball collision with Player 1 
                if BALL_X <= PLAYER1_PADDLE_X + PADDLE_WIDTH + PADDLE_BUFFER && 
                    BALL_Y + BALL_SIZE >= PLAYER1_PADDLE_Y - PADDLE_BUFFER &&    
                    BALL_Y <= PLAYER1_PADDLE_Y + PADDLE_HEIGHT + PADDLE_BUFFER {
                    BALL_VEL_X = BALL_VEL_X.abs(); 
                }
                
                // Ball collision with Player 2 
                if BALL_X + BALL_SIZE >= PLAYER2_PADDLE_X - PADDLE_BUFFER && 
                    BALL_Y + BALL_SIZE >= PLAYER2_PADDLE_Y - PADDLE_BUFFER &&  
                    BALL_Y <= PLAYER2_PADDLE_Y + PADDLE_HEIGHT + PADDLE_BUFFER {
                    BALL_VEL_X = -BALL_VEL_X.abs(); 
                }
                
                
                if BALL_X <= PADDLE_WIDTH {
                    PLAYER2_SCORE += 1;
                    if PLAYER2_SCORE >= WINNING_SCORE {
                        GAME_STATE = GameState::GameOver; 
                    } else {
                        reset_ball();
                    }
                } else if BALL_X >= SCREEN_WIDTH {
                    PLAYER1_SCORE += 1;
                    if PLAYER1_SCORE >= WINNING_SCORE {
                        GAME_STATE = GameState::GameOver; 
                    } else {
                        reset_ball();
                    }
                }

                draw_center_line(writer);
                draw_score(writer, PLAYER1_SCORE, PLAYER2_SCORE);
                draw_ball(writer, BALL_X, BALL_Y, 255, 255, 255);
            }
            GameState::GameOver => {
                BALL_VEL_X = 0;
                BALL_VEL_Y = 0;
                draw_game_over_screen(writer);
            }
        }
        
    }
}

fn key(key: DecodedKey) {
    unsafe {
        let writer = screenwriter();

        match GAME_STATE {
            GameState::StartScreen => {
                match key {
                    DecodedKey::Unicode(' ') => {
                        init_game();
                    },
                    _ => {}
                }
            }
            GameState::Playing => {
                match key {
                    // Player 1 controls (W/S)
                    DecodedKey::Unicode('w') => {
                        if PLAYER1_PADDLE_Y > PADDLE_SPEED {
                            draw_paddle(writer, PLAYER1_PADDLE_X, PLAYER1_PADDLE_Y, 0, 0, 0); // Erase old paddle
                            PLAYER1_PADDLE_Y -= PADDLE_SPEED;
                            draw_paddle(writer, PLAYER1_PADDLE_X, PLAYER1_PADDLE_Y, 255, 255, 255); // Draw new paddle
                        }
                    }
                    
                    DecodedKey::Unicode('s') => {
                        if PLAYER1_PADDLE_Y + PADDLE_HEIGHT + PADDLE_SPEED < SCREEN_HEIGHT {
                            draw_paddle(writer, PLAYER1_PADDLE_X, PLAYER1_PADDLE_Y, 0, 0, 0); // Erase old paddle
                            PLAYER1_PADDLE_Y += PADDLE_SPEED;
                            draw_paddle(writer, PLAYER1_PADDLE_X, PLAYER1_PADDLE_Y, 255, 255, 255); // Draw new paddle
                        }
                    }
        
                    // Player 2 controls (Arrow Up/Down)
                    DecodedKey::RawKey(KeyCode::ArrowUp) => {
                        if PLAYER2_PADDLE_Y > PADDLE_SPEED {
                            draw_paddle(writer, PLAYER2_PADDLE_X, PLAYER2_PADDLE_Y, 0, 0, 0); // Erase old paddle
                            PLAYER2_PADDLE_Y -= PADDLE_SPEED;
                            draw_paddle(writer, PLAYER2_PADDLE_X, PLAYER2_PADDLE_Y, 255, 255, 255); // Draw new paddle
                        }
                    }
                    DecodedKey::RawKey(KeyCode::ArrowDown) => {
                        if PLAYER2_PADDLE_Y + PADDLE_HEIGHT + PADDLE_SPEED < SCREEN_HEIGHT {
                            draw_paddle(writer, PLAYER2_PADDLE_X, PLAYER2_PADDLE_Y, 0, 0, 0); // Erase old paddle
                            PLAYER2_PADDLE_Y += PADDLE_SPEED;
                            draw_paddle(writer, PLAYER2_PADDLE_X, PLAYER2_PADDLE_Y, 255, 255, 255); // Draw new paddle
                        }
                    }
        
                    _ => {}
                }
            }
            GameState::GameOver => {
                match key {
                    DecodedKey::Unicode(' ') => {
                        init_game();
                    },
                    _ => {}
                }
            }
        }
    }
}

