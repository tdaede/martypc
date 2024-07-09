/*
    MartyPC
    https://github.com/dbalsom/martypc

    Copyright 2022-2024 Daniel Balsom

    Permission is hereby granted, free of charge, to any person obtaining a
    copy of this software and associated documentation files (the “Software”),
    to deal in the Software without restriction, including without limitation
    the rights to use, copy, modify, merge, publish, distribute, sublicense,
    and/or sell copies of the Software, and to permit persons to whom the
    Software is furnished to do so, subject to the following conditions:

    The above copyright notice and this permission notice shall be included in
    all copies or substantial portions of the Software.

    THE SOFTWARE IS PROVIDED “AS IS”, WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
    IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
    FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
    AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
    LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
    FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
    DEALINGS IN THE SOFTWARE.

    --------------------------------------------------------------------------

    devices::cga::io.rs

    Implementation of the IoDevice interface trait for the IBM CGA card.

*/

use std::collections::VecDeque;

#[derive(Default)]
pub struct GDC {
    x: u16,
    y: u16,
    fifo: VecDeque<u16>, // high bit indicates paramter
}

impl GDC {
    fn vsync_flag(&self) -> bool {
        // TODO: figure out actual timing of this flag
        self.y >= 400
    }
    fn fifo_empty_flag(&self) -> bool {
        self.fifo.len() == 0
    }
    pub fn read_status(&mut self) -> u8 {
        (self.vsync_flag() as u8) << 5 |
        (self.fifo_empty_flag() as u8) << 2
    }
    fn reset(&mut self) {
        self.fifo.clear()
    }
    pub fn write_command(&mut self, b: u8) {
        match b {
            0b00000000 | 0b00000001 | 0b0001001 => {
                self.reset();
            }
            _ => {}
        }
        if self.fifo.len() < 16 {
            self.fifo.push_back(b as u16);
        }
    }
    pub fn write_parameter(&mut self, b: u8) {
        if self.fifo.len() < 16 {
            self.fifo.push_back((b as u16) | 0b1_00000000);
        }
    }
    pub fn tick_wclk(&mut self) {
        if let Some(b) = self.fifo.pop_front() {
            match b {
                0b00000000 => {
                    eprintln!("GDC: got RESET1 command");
                }
                0b00000001 => {
                    eprintln!("GDC: got RESET2 command");
                }
                0b00010001 => {
                    eprintln!("GDC: got RESET3 command");
                }
                0b00001100..=0b00001101 => {
                    eprintln!("GDC: got BLANK1 command");
                }
                0b00000100..=0b00000101 => {
                    eprintln!("GDC: got BLANK2 command");
                }
                0b00001110..=0b00001111 => {
                    eprintln!("GDC: got SYNC command");
                }
                0b01101110..=0b01101111 => {
                    eprintln!("GDC: got VSYNC command");
                }
                0b01001011 => {
                    eprintln!("GDC: got CCHAR command");
                }
                0b01101011 => {
                    eprintln!("GDC: got START command");
                }
                0b01000110 => {
                    eprintln!("GDC: got ZOOM command");
                }
                0b01001001 => {
                    eprintln!("GDC: got CURS command");
                }
                0b01110000..=0b01111111 => {
                    eprintln!("GDC: got PRAM command");
                }
                0b01000111 => {
                    eprintln!("GDC: got PITCH command");
                }
                0b00100000..=0b00100011 |
                0b00101000..=0b00101011 |
                0b00110000..=0b00110011 |
                0b00111000..=0b00111011 => {
                    eprintln!("GDC: got WDAT command");
                }
                0b1_00000000..=0b1_11111111 => {
                    eprintln!("GDC: got parameter {:08b} ({})", b as u8, (b as u8) as char);
                }
                _ => {
                    eprintln!("GDC: unknown command {:08b}", b);
                }
            }
        }
        self.x += 1;
        if self.x > 848 {
            self.x = 0;
            self.y += 1;
        }
        if self.y > 525 {
            self.y = 0;
        }
    }
}
