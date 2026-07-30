// SPDX-License-Identifier: 0BSD

use core::ptr;
use slopos_boot_protocol::{FramebufferInfo, PixelFormat};

pub type Color = u32;

pub const WHITE: Color = 0xf4f4f8;
pub const BLACK: Color = 0x0b0d14;
pub const PANEL: Color = 0x161a2a;
pub const INDIGO: Color = 0x6558f5;
pub const CYAN: Color = 0x4dd8e5;
pub const GREEN: Color = 0x5ee28a;
pub const RED: Color = 0xff667d;
pub const MUTED: Color = 0x9ba3c7;
pub const WINDOW: Color = 0x20263a;
pub const WINDOW_ALT: Color = 0x171c2b;

pub struct Framebuffer {
    base: *mut u32,
    pixel_count: usize,
    width: usize,
    height: usize,
    stride: usize,
    format: PixelFormat,
}

impl Framebuffer {
    pub fn new(info: FramebufferInfo) -> Option<Self> {
        let width = info.width as usize;
        let height = info.height as usize;
        let stride = info.stride as usize;
        let pixel_count = (info.size as usize) / core::mem::size_of::<u32>();
        if info.base == 0
            || width < 320
            || height < 200
            || stride < width
            || stride.checked_mul(height)? > pixel_count
        {
            return None;
        }
        Some(Self {
            base: info.base as *mut u32,
            pixel_count,
            width,
            height,
            stride,
            format: info.pixel_format,
        })
    }

    pub const fn width(&self) -> usize {
        self.width
    }

    pub const fn height(&self) -> usize {
        self.height
    }

    pub fn pixel(&mut self, x: i32, y: i32, color: Color) {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return;
        }
        let offset = y as usize * self.stride + x as usize;
        if offset >= self.pixel_count {
            return;
        }
        // UEFI PixelBlueGreenRedReserved8BitPerColor stores the low byte as blue;
        // our 0xRRGGBB color already has that in little-endian memory. RGB swaps it.
        let encoded = match self.format {
            PixelFormat::Rgb => {
                let red = (color >> 16) & 0xff;
                let green = color & 0x00ff00;
                let blue = color & 0xff;
                blue << 16 | green | red
            }
            PixelFormat::Bgr | PixelFormat::Bitmask | PixelFormat::Unknown => color,
        };
        // SAFETY: offset was checked against both geometry and mapped framebuffer size.
        unsafe { ptr::write_volatile(self.base.add(offset), encoded) };
    }

    pub fn rect(&mut self, x: i32, y: i32, width: i32, height: i32, color: Color) {
        let x0 = x.max(0);
        let y0 = y.max(0);
        let x1 = (x + width).min(self.width as i32);
        let y1 = (y + height).min(self.height as i32);
        for py in y0..y1 {
            for px in x0..x1 {
                self.pixel(px, py, color);
            }
        }
    }

    pub fn outline(
        &mut self,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        thickness: i32,
        color: Color,
    ) {
        self.rect(x, y, width, thickness, color);
        self.rect(x, y + height - thickness, width, thickness, color);
        self.rect(x, y, thickness, height, color);
        self.rect(x + width - thickness, y, thickness, height, color);
    }

    pub fn text(&mut self, mut x: i32, y: i32, text: &str, color: Color, scale: i32) {
        for byte in text.bytes() {
            crate::font::draw_glyph(self, x, y, byte, color, scale);
            x += 6 * scale;
        }
    }

    pub fn cursor(&mut self, x: i32, y: i32) {
        for row in 0..17 {
            let width = if row < 11 { row / 2 + 2 } else { 4 };
            for col in 0..width {
                let edge = col == 0 || col == width - 1 || row == 0 || row == 16;
                self.pixel(x + col, y + row, if edge { BLACK } else { WHITE });
            }
        }
    }
}
