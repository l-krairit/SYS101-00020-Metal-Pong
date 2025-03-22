// Original code from rust-osdev/bootloader crate https://github.com/rust-osdev/bootloader

use core::{fmt, ptr};
use noto_sans_mono_bitmap::{FontWeight, get_raster, RasterizedChar};
use bootloader_api::info::{FrameBuffer, FrameBufferInfo, PixelFormat};
use noto_sans_mono_bitmap::RasterHeight::Size16;
use kernel::RacyCell;
use crate::alloc::string::ToString;

static WRITER: RacyCell<Option<ScreenWriter>> = RacyCell::new(None);
pub struct Writer;

impl fmt::Write for Writer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let writer = unsafe { WRITER.get_mut() }.as_mut().unwrap();
        writer.write_str(s)
    }
}

pub fn screenwriter() -> &'static mut ScreenWriter {
    let writer = unsafe { WRITER.get_mut() }.as_mut().unwrap();
    writer
}


pub fn init(buffer: &'static mut FrameBuffer) {
    let info = buffer.info();
    let framebuffer = buffer.buffer_mut();
    let writer = ScreenWriter::new(framebuffer, info);
    *unsafe { WRITER.get_mut() } = Some(writer);
}

const LINE_SPACING: usize = 0;

pub struct ScreenWriter {
    framebuffer: &'static mut [u8],
    info: FrameBufferInfo,
    x_pos: usize,
    y_pos: usize,
}

impl ScreenWriter {
    pub fn new(framebuffer: &'static mut [u8], info: FrameBufferInfo) -> Self {
        let mut logger = Self {
            framebuffer,
            info,
            x_pos: 0,
            y_pos: 0,
        };
        logger.clear();
        logger
    }

    fn newline(&mut self) {
        self.y_pos += Size16 as usize + LINE_SPACING;
        self.carriage_return()
    }

    fn carriage_return(&mut self) {
        self.x_pos = 0;
    }

    pub fn clear(&mut self) {
        self.x_pos = 0;
        self.y_pos = 0;
        self.framebuffer.fill(0);
    }

    fn width(&self) -> usize {
        self.info.width.into()
    }

    fn height(&self) -> usize {
        self.info.height.into()
    }

    fn write_char(&mut self, c: char) {
        match c {
            '\n' => self.newline(),
            '\r' => self.carriage_return(),
            c => {
                match get_raster(c, FontWeight::Regular, Size16) {
                    Some(bitmap_char) => {
                        if self.x_pos + bitmap_char.width() > self.width() {
                            self.newline();
                        }
                        if self.y_pos + bitmap_char.height() > self.height() {
                            self.clear();
                        }
                        self.write_rendered_char(bitmap_char);
                    },
                    None => {}
                }
            }
        }
    }

    fn write_rendered_char(&mut self, rendered_char: RasterizedChar) {
        for (y, row) in rendered_char.raster().iter().enumerate() {
            for (x, byte) in row.iter().enumerate() {
                self.write_pixel(self.x_pos + x, self.y_pos + y, *byte);
            }
        }
        self.x_pos += rendered_char.width();
    }

    pub fn write_pixel(&mut self, x: usize, y: usize, intensity: u8) {
        let pixel_offset = y * usize::from(self.info.stride) + x;
        let color = match self.info.pixel_format {
            PixelFormat::Rgb => [intensity / 4, intensity, intensity / 2, 0],
            PixelFormat::Bgr => [intensity / 2, intensity, intensity / 4, 0],
            other => {
                // set a supported (but invalid) pixel format before panicking to avoid a double
                // panic; it might not be readable though
                self.info.pixel_format = PixelFormat::Rgb;
                panic!("pixel format {:?} not supported in logger", other)
            }
        };
        let bytes_per_pixel = self.info.bytes_per_pixel;
        let byte_offset = pixel_offset * usize::from(bytes_per_pixel);
        self.framebuffer[byte_offset..(byte_offset + usize::from(bytes_per_pixel))]
            .copy_from_slice(&color[..usize::from(bytes_per_pixel)]);
        let _ = unsafe { ptr::read_volatile(&self.framebuffer[byte_offset]) };
    }

    pub fn draw_pixel(&mut self, x: usize, y: usize, r: u8, g: u8, b: u8) {
        if x >= self.info.width || y >= self.info.height {
            return;
        }
    
        let pixel_offset = match y.checked_mul(self.info.stride.into()) {
            Some(offset) => offset + x,
            None => return, 
        };
    
        let byte_offset = match pixel_offset.checked_mul(self.info.bytes_per_pixel.into()) {
            Some(offset) => offset,
            None => return, 
        };
    
        if byte_offset + self.info.bytes_per_pixel as usize > self.framebuffer.len() {
            return; 
        }
    
        let color = match self.info.pixel_format {
            PixelFormat::Rgb => [r, g, b, 0],
            PixelFormat::Bgr => [b, g, r, 0],
            _ => return, 
        };
    
        self.framebuffer[byte_offset..(byte_offset + self.info.bytes_per_pixel as usize)]
            .copy_from_slice(&color[..usize::from(self.info.bytes_per_pixel)]);
    }
}


pub fn draw_paddle(writer: &mut ScreenWriter, x: usize, y: usize, r: u8, g: u8, b: u8) {
    const PADDLE_WIDTH: usize = 15;  
    const PADDLE_HEIGHT: usize = 100; 

    for dy in 0..PADDLE_HEIGHT {
        for dx in 0..PADDLE_WIDTH {
            writer.draw_pixel(x + dx, y + dy, r, g, b);
        }
    }
}


pub fn draw_ball(writer: &mut ScreenWriter, x: usize, y: usize, r: u8, g: u8, b: u8) {
    const BALL_SIZE: usize = 12;     
    for dy in 0..BALL_SIZE {  
        for dx in 0..BALL_SIZE {
            writer.draw_pixel(x + dx, y + dy, r, g, b);
        }
    }
}


pub fn draw_center_line(writer: &mut ScreenWriter) {
    let mid_x = writer.width() / 2;
    for y in (0..writer.height()).step_by(20) {  
        for dy in 0..10 { 
            writer.draw_pixel(mid_x, y + dy, 200, 200, 200);
        }
    }
}


pub fn draw_score(writer: &mut ScreenWriter, player1_score: usize, player2_score: usize) {
    let mid_x = writer.width() / 2;

    writer.write_number(player1_score, mid_x - 40, 20);  // Left player score
    writer.write_number(player2_score, mid_x + 20, 20);  // Right player score
}



unsafe impl Send for ScreenWriter {}
unsafe impl Sync for ScreenWriter {}

impl fmt::Write for ScreenWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for c in s.chars() {
            self.write_char(c);
        }
        Ok(())
    }
}

impl ScreenWriter {
    pub fn write_number(&mut self, num: usize, x: usize, y: usize) {
        let text = num.to_string();
        self.x_pos = x;
        self.y_pos = y;
        for c in text.chars() {
            self.write_char(c);
        }
    }

    pub fn write_large_char(&mut self, c: char, x: usize, y: usize, r: u8, g: u8, b: u8) {
        const SCALE: usize = 3; 
        
        if let Some(bitmap_char) = get_raster(c, FontWeight::Regular, Size16) {
            for (char_y, row) in bitmap_char.raster().iter().enumerate() {
                for (char_x, intensity) in row.iter().enumerate() {
                    if *intensity > 0 {
                        for dy in 0..SCALE {
                            for dx in 0..SCALE {
                                self.draw_pixel(
                                    x + char_x * SCALE + dx,
                                    y + char_y * SCALE + dy,
                                    r, g, b
                                );
                            }
                        }
                    }
                }
            }
        }
    }
    pub fn write_large_text(&mut self, text: &str, x: usize, y: usize, r: u8, g: u8, b: u8) {
        const SCALE: usize = 3;
        const CHAR_WIDTH: usize = 8 * SCALE;

        let mut current_x = x;
        for c in text.chars() {
            self.write_large_char(c, current_x, y, r, g, b);
            current_x += CHAR_WIDTH;
        }
    }
}