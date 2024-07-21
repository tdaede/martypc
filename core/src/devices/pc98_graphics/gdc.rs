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
enum GDCState {
    #[default]
    Idle,
    SyncP1,
    SyncP2,
    SyncP3,
    SyncP4,
    SyncP5,
    SyncP6,
    SyncP7,
    SyncP8,
    Zoom,
    Pitch,
}

#[derive(Default)]
pub struct GDC {
    x: u16,
    y: u16,
    fifo: VecDeque<u16>, // high bit indicates paramter
    s: GDCState,
    mode: u8,
    aw_minus2: u8, // active display words per line - 2
    hs_minus1: u8, // horizontal sync width - 1
    vs: u8, // vertical sync width
    hfp_minus1: u8, // horizontal front porch width - 1
    hbp_minus1: u8, // horizontal back porch width - 1
    pitch: u16, // pitch
    dh: bool, // drawing hold
    vfp: u8, // vertical front porch width
    vl: bool, // 0 = odd, 1 = even
    vh: bool, // status register indicates horizontal vs vertical blank
    al: u16, // active display lines per field
    vbp: u8, // vertical back porch width
    zoom: u8,
    started: bool,
    wait: u8, // number of cycles to delay before processing next fifo
    address: u32, // 18 bit output address
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
        if self.wait > 0 {
            self.wait -= 1;
            return;
        }
        if let Some(b) = self.fifo.pop_front() {
            match b {
                0b00000000 => {
                    eprintln!("GDC: got RESET1 command");
                    self.wait = 2;
                    self.s = GDCState::SyncP1;
                }
                0b00000001 => {
                    eprintln!("GDC: got RESET2 command");
                    self.wait = 2;
                    self.s = GDCState::SyncP1;
                }
                0b00010001 => {
                    eprintln!("GDC: got RESET3 command");
                    self.wait = 2;
                    self.s = GDCState::SyncP1;
                }
                0b00001100..=0b00001101 => {
                    eprintln!("GDC: got BLANK1 command");
                    self.wait = 2;
                }
                0b00000100..=0b00000101 => {
                    eprintln!("GDC: got BLANK2 command");
                    self.wait = 2;
                }
                0b00001110..=0b00001111 => {
                    eprintln!("GDC: got SYNC command");
                    self.wait = 2;
                    self.s = GDCState::SyncP1;
                }
                0b01101110..=0b01101111 => {
                    eprintln!("GDC: got VSYNC command");
                    self.wait = 5;
                    // not emulated
                    self.s = GDCState::Idle;
                }
                0b01001011 => {
                    eprintln!("GDC: got CCHAR command");
                    self.wait = 4;
                }
                0b01101011 => {
                    eprintln!("GDC: got START command");
                    self.started = true;
                    self.wait = 5;
                    self.s = GDCState::Idle;
                }
                0b01000110 => {
                    eprintln!("GDC: got ZOOM command");
                    self.wait = 4;
                    self.s = GDCState::Zoom;
                }
                0b01001001 => {
                    eprintln!("GDC: got CURS command");
                    self.wait = 2;
                }
                0b01110000..=0b01111111 => {
                    eprintln!("GDC: got PRAM command");
                    self.wait = 4;
                }
                0b01000111 => {
                    eprintln!("GDC: got PITCH command");
                    self.wait = 4;
                    self.s = GDCState::Pitch;
                }
                0b00100000..=0b00100011 |
                0b00101000..=0b00101011 |
                0b00110000..=0b00110011 |
                0b00111000..=0b00111011 => {
                    eprintln!("GDC: got WDAT command");
                    // TODO: different between word and byte writes
                    self.wait = 5;
                }
                0b1_00000000..=0b1_11111111 => {
                    let p = b as u8;
                    match self.s {
                        GDCState::SyncP1 => {
                            self.mode = p;
                            self.s = GDCState::SyncP2;
                        },
                        GDCState::SyncP2 => {
                            self.aw_minus2 = p;
                            self.s = GDCState::SyncP3;
                        },
                        GDCState::SyncP3 => {
                            self.vs = p >> 5;
                            self.hs_minus1 = p & 0b00011111;
                            self.s = GDCState::SyncP4;
                        },
                        GDCState::SyncP4 => {
                            self.hfp_minus1 = p >> 2;
                            self.vs |= (p & 0b00000011) << 3;
                            self.s = GDCState::SyncP5;
                        },
                        GDCState::SyncP5 => {
                            self.dh = p & 0b10000000 != 0;
                            self.pitch = (self.pitch & 0xff) | (p as u16 & 0b01000000) << 2;
                            self.hbp_minus1 = p & 0b00111111;
                            self.s = GDCState::SyncP6;
                        },
                        GDCState::SyncP6 => {
                            self.vh = p & 0b10000000 != 0;
                            self.vl = p & 0b01000000 != 0;
                            self.vfp = p & 0b00111111;
                            self.s = GDCState::SyncP7;
                        },
                        GDCState::SyncP7 => {
                            self.al = p as u16;
                            self.s = GDCState::SyncP8;
                        },
                        GDCState::SyncP8 => {
                            self.vbp = p >> 2;
                            self.al |= ((p & 0b00000011) as u16) << 8;
                            eprintln!("GDC: got sync parameters");
                            eprintln!("hs: {}, hfp: {}, aw: {}, hbp: {}", self.hs_minus1 + 1,
                                      self.hfp_minus1 + 1, self.aw_minus2 + 2, self.hbp_minus1 + 1);
                            eprintln!("vs: {}, vfp: {}, al: {}, vbp: {}", self.vs, self.vfp, self.al, self.vbp);
                            if self.dh {
                                eprintln!("GDC: drawing hold not implemented!");
                            }
                            if self.vl {
                                eprintln!("GDC: even number of scan lines not supported!");
                            }
                            if !self.vh {
                                eprintln!("GDC: status register indicating hblank not supported!");
                            }
                            self.s = GDCState::Idle;
                        }
                        GDCState::Zoom => {
                            self.zoom = p;
                            if self.zoom != 0 {
                                eprintln!("GDC: zoom unsupported!");
                            }
                            self.s = GDCState::Idle;
                        }
                        GDCState::Pitch => {
                            self.pitch = (self.pitch & 0x100) | (p as u16);
                            eprintln!("GDC: pitch set to {}", self.pitch);
                            self.s = GDCState::Idle;
                        }
                        _ => {
                            eprintln!("GDC: got unused parameter {:08b} ({})", b as u8, (b as u8) as char);
                            self.s = GDCState::Idle;
                        }
                    }
                }
                _ => {
                    eprintln!("GDC: unknown command {:08b}", b);
                }
            }
        }
        self.x += 1;
        if self.x > (848/16) {
            self.x = 0;
            self.y += 1;
        }
        if self.y > 525 {
            self.y = 0;
        }
    }
}
