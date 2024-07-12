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

    devices::keyboard_pc98.rs

    Implements the PC98 keyboard, which is based on the 8251 USART.
*/

use std::collections::VecDeque;

use crate::{
    bus::{BusInterface, DeviceRunTimeUnit, IoDevice},
    devices::pic,
};

/* PC98 keyboard is 19200 baud, 8 bit data, 1 bit parity, 1 start bit, 1 stop bit */
/* additionally, ~RDY must be low for at least 13us before keyboard sends next byte */
pub const PC98_US_PER_BYTE: f64 = (8.0 + 1.0 + 1.0 + 1.0) / 19200.0 / 1_000_000.0 + 13.0;

pub const PC98_KEYBOARD_IRQ: u8 = 1;

pub const PC98_KEYBOARD_DATA: u16 = 0x41;
pub const PC98_KEYBOARD_CONTROL: u16 = 0x43;

impl IoDevice for PC98Keyboard {
    fn read_u8(&mut self, port: u16, _delta: DeviceRunTimeUnit) -> u8 {
        match port {
            PC98_KEYBOARD_DATA => self.upd8251.data_read(),
            PC98_KEYBOARD_CONTROL => self.upd8251.status_read(),
            _ => unreachable!(),
        }
    }

    fn write_u8(&mut self, port: u16, byte: u8, _bus: Option<&mut BusInterface>, _delta: DeviceRunTimeUnit) {
        match port {
            PC98_KEYBOARD_DATA => self.upd8251.data_write(byte),
            PC98_KEYBOARD_CONTROL => self.upd8251.control_write(byte),
            _ => unreachable!(),
        }
    }

    fn port_list(&self) -> Vec<(String, u16)> {
        vec![
            (String::from("Keyboard Data"), PC98_KEYBOARD_DATA),
            (String::from("Keyboard Status/Control"), PC98_KEYBOARD_CONTROL),
        ]
    }
}

#[derive(Default)]
struct UPD8251 {
    rxbuf: u8,
    rxrdy: bool,
    oe: bool,
}

impl UPD8251 {
    fn data_read(&mut self) -> u8 {
        self.rxrdy = false;
        self.rxbuf
    }

    fn data_write(&self, _: u8) {
        /* unimplemented */
    }

    fn status_read(&self) -> u8 {
        1 << 2 // txe always 1
        | (self.rxrdy as u8) << 1
        | 1 // txrdy always 1
    }

    fn control_write(&self, _: u8) {
        /* unimplemented */
    }

    fn reset(&mut self) {
        self.rxbuf = 0;
        self.rxrdy = false;
        self.oe = false;
    }

    fn push_byte(&mut self, byte: u8) {
        self.rxbuf = byte;
        if self.rxrdy {
            self.oe = true;
        }
        self.rxrdy = true;
    }
}

pub struct PC98Keyboard {
    rx_queue: VecDeque<u8>,
    upd8251: UPD8251,
    rx_timer: f64,
}

impl PC98Keyboard {
    pub fn new() -> Self {
        Self {
            rx_queue: VecDeque::new(),
            upd8251: UPD8251::default(),
            rx_timer: 0.0,
        }
    }

    pub fn reset(&mut self) {
        self.rx_queue.clear();
        self.upd8251.reset();
    }

    /// Queue a byte from the keyboard to the 8251
    pub fn send_keyboard(&mut self, byte: u8) {
        eprintln!("sending keycode {:0x}", byte);
        // TODO: figure out how long the keyboard hardware's send queue is
        self.rx_queue.push_back(byte);
    }

    /// Run the keyboard 8251 for the specified number of microseconds
    pub fn run(&mut self, pic: &mut pic::Pic, _us: f64) {
        // TODO: implement timing
        // keyboard receives rdy status and will never transmit
        // without
        if !self.upd8251.rxrdy {
            if let Some(byte) = self.rx_queue.pop_front() {
                self.upd8251.push_byte(byte);
            }
        }
        if self.upd8251.rxrdy {
            pic.request_interrupt(PC98_KEYBOARD_IRQ);
        } else {
            pic.clear_interrupt(PC98_KEYBOARD_IRQ);
        }
    }
}
