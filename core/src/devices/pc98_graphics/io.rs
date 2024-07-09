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
use super::*;
use crate::bus::IoDevice;


impl IoDevice for PC98Graphics {
    fn read_u8(&mut self, port: u16, delta: DeviceRunTimeUnit) -> u8 {
        // Catch up to CPU state.
        //let _ticks = self.catch_up(delta, false);
        match port {
            0x60 => self.tgdc.read_status(),
            0xa0 => self.ggdc.read_status(),
            _ => 0
        }
    }

    fn write_u8(&mut self, port: u16, data: u8, _bus: Option<&mut BusInterface>, delta: DeviceRunTimeUnit) {
        // Catch up to CPU state.
        //let _ticks = self.catch_up(delta, debug_port);
        match port {
            0x60 => self.tgdc.write_parameter(data),
            0x62 => self.tgdc.write_command(data),
            0xa0 => self.ggdc.write_parameter(data),
            0xa2 => self.ggdc.write_command(data),
            _ => {}
        }
    }

    fn port_list(&self) -> Vec<(String, u16)> {
        vec![
            (String::from("Text GDC Status/Parameter Register"), 0x60),
            (String::from("Text GDC FIFO Register"), 0x62),
            (String::from("CRT Interrupt Reset"), 0x64),
            (String::from("CRT Mode 1"), 0x68),
            (String::from("CRT Mode 2"), 0x6a),
            (String::from("Border Color"), 0x6c),
            (String::from("Graphics GDC Status/Parameter Register"), 0xa0),
            (String::from("Graphics GDC FIFO Register"), 0xa2),
            (String::from("Graphics Display Plane"), 0xa4),
            (String::from("Graphics Drawing Plane"), 0xa6),
            (String::from("Palette 1"), 0xa8),
            (String::from("Palette 2"), 0xaa),
            (String::from("Palette 3"), 0xac),
            (String::from("Palette 4"), 0xae),
        ]
    }
}
