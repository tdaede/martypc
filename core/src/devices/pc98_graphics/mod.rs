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

    devices::cga::mod.rs

*/

#![allow(dead_code)]
use bytemuck;
use const_format::formatcp;
use std::{collections::HashMap, convert::TryInto, path::Path};

#[macro_use]
mod io;
mod draw;
mod mmio;
mod tablegen;
mod videocard;
mod gdc;

use super::*;

use gdc::GDC;

use crate::{
    bus::{BusInterface, DeviceRunTimeUnit},
    device_traits::videocard::*,
    tracelogger::TraceLogger,
    devices::pic::Pic,
};

// text gdc runs at 2.5mhz for 40 column, 5mhz for 80 column
// graphics gdc 2.5 or 5mhz
// todo: figure out which register controls text gdc clock rate
const GDC_WCLK: f64 = 21.0526 / 16.0; // 2.5mhz clock = 1.25mhz wclk
const US_PER_CLOCK: f64 = 1.0 / GDC_WCLK;
const US_PER_FRAME: f64 = 1.0 / 50.0;

static DUMMY_PLANE: [u8; 1] = [0];
static DUMMY_PIXEL: [u8; 4] = [0, 0, 0, 0];

const PC98_FONT: &'static [u8] = include_bytes!("../../../../assets/FONT.ROM");

const PC98_APERTURES: [DisplayAperture; 1] = [
    DisplayAperture {
        w: 640,
        h: 400,
        x: 0,
        y: 0,
        debug: false,
    },
];

const CROPPED_STRING: &str = &formatcp!("Cropped: 640x400");

const PC98_APERTURE_DESCS: [DisplayApertureDesc; 1] = [
    DisplayApertureDesc {
        name: CROPPED_STRING,
        aper_enum: DisplayApertureType::Cropped,
    },
];

const PC98_DEFAULT_APERTURE: usize = 0;

pub struct PC98Graphics {
    clock_mode: ClockingMode,
    ticks_accum: f64,
    frame_count:  u64,
    beam_x: u32,
    beam_y: u32,
    scanline: u32,
    extents: DisplayExtents,
    back_buf: usize,
    front_buf: usize,
    tvmem: Box<[u8; 0x4000]>,
    gvmem: Box<[u8; 0x20000]>, // 4 planes sequential
    tgdc: GDC,
    ggdc: GDC,
    buf: [Box<[u8; 640*400]>; 2],
}

impl Default for PC98Graphics {
    fn default() -> Self {
        Self {
            clock_mode: ClockingMode::Default,
            ticks_accum: 0.0,
            frame_count: 0,
            beam_x: 0,
            beam_y: 0,
            scanline: 0,
            extents: PC98Default::default(),
            back_buf: 1,            front_buf: 0,
            tvmem: vec![0; 0x4000].into_boxed_slice().try_into().unwrap(),
            gvmem: vec![0; 0x20000].into_boxed_slice().try_into().unwrap(),
            tgdc: GDC::default(),
            ggdc: GDC::default(),
            buf: [
                vec![0; 640*400].into_boxed_slice().try_into().unwrap(),
                vec![0; 640*400].into_boxed_slice().try_into().unwrap(),
            ],
        }
    }
}

trait PC98Default {
    fn default() -> Self;
}
impl PC98Default for DisplayExtents {
    fn default() -> Self {
        Self {
            apertures: PC98_APERTURES.to_vec(),
            field_w: 640,
            field_h: 400,
            row_stride: 640 as usize,
            double_scan: false,
            mode_byte: 0,
        }
    }
}

impl PC98Graphics {
    pub fn new(_trace_logger: TraceLogger, clock_mode: ClockingMode, _video_frame_debug: bool) -> Self {
        let mut pc98 = Self::default();

        if let ClockingMode::Default = clock_mode {
            pc98.clock_mode = ClockingMode::Dynamic;
        }
        else {
            pc98.clock_mode = clock_mode;
        }

        pc98
    }

    pub fn do_ticks(&mut self, ticks: f64, pic: &mut Option<Pic>) {
        self.ticks_accum += ticks;
        // Drain the accumulator while emitting chars
        while self.ticks_accum > 1.0 {
            self.tick(pic);
            self.ticks_accum -= 1.0;
        }
    }

    fn draw_char(&mut self, beam_x: u32) {
        if !self.tgdc.blank {
            let char_code: u16 = self.tvmem[(self.tgdc.address & 0x1fff) as usize * 2] as u16;
            let b: u8 = PC98_FONT[char_code as usize * 16 + (self.tgdc.address >> 13) as usize + 0x800];
            for x in 0..8 {
                let p = if self.tgdc.cursor_active {
                    15
                } else {
                    if (b >> (7-x)) & 1 == 1 { 15 } else { 0 }
                };
                let buf_addr: i32 = 640* (self.scanline as i32).saturating_sub(25) + (beam_x as i32 + x as i32).saturating_sub(8*8);
                if buf_addr >= 0 && buf_addr < 640*400 {
                    self.buf[0][buf_addr as usize] = p;
                }
            }
        }
    }

    pub fn tick(&mut self, pic: &mut Option<Pic>) {
        // in 80 column mode, tgdc runs at 5mhz
        self.tgdc.tick_wclk();
        self.draw_char(self.beam_x);
        self.tgdc.tick_wclk();
        self.draw_char(self.beam_x + 8);
        self.ggdc.tick_wclk();
        // every WCLK, 16 pixels are transferred out to serializer
        self.beam_x += 16;
        if self.beam_x >= 848 {
            self.beam_x = 0;
            self.scanline += 1;
        }
        if self.scanline >= 525 {
            self.scanline = 0;
            if let Some(pic) = pic {
                // todo: figure out exact interrupt line timing
                pic.pulse_interrupt(2);
            }
        }
    }
 }
