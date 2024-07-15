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

    devices::pc98_system_port.rs

    Implement the 8255 PC98 system port.
*/
#![allow(dead_code)]

use modular_bitfield::{
    bitfield,
    BitfieldSpecifier,
};

use crate::
    bus::{BusInterface, DeviceRunTimeUnit, IoDevice, NO_IO_BYTE}
;

#[derive(Debug, Default, BitfieldSpecifier)]
pub enum PpiModeA {
    #[default]
    Mode0Io,
    Mode1StrobedIo,
    Mode2BiDirectional,
    Mode2BiDirectional2,
}

#[derive(Debug, Default, BitfieldSpecifier)]
pub enum PpiModeB {
    #[default]
    Mode0Io,
    Mode1StrobedIo,
}

#[derive(Debug, Default, BitfieldSpecifier)]
pub enum IoMode {
    #[default]
    Output,
    Input,
}

#[bitfield]
#[derive(Copy, Clone, Debug, Default)]
pub struct PpiControlWord {
    pub group_b_c: IoMode,
    pub group_b_b: IoMode,
    pub group_b_mode: PpiModeB,
    pub group_a_c: IoMode,
    pub group_a_a: IoMode,
    pub group_a_mode: PpiModeA,
    pub mode_set: bool,
}

pub const SYSTEM_PORT_A: u16 = 0x31;
pub const SYSTEM_PORT_B: u16 = 0x33;
pub const SYSTEM_PORT_C: u16 = 0x35;
pub const SYSTEM_COMMAND_PORT: u16 = 0x37;

pub const DIP_SW2_80_COLUMN: u8 = 0b0000_0100;
pub const DIP_SW2_25_LINE: u8 = 0b0000_1000;

#[derive(Default)]
pub struct PC98SystemPort {
    control_word: PpiControlWord,
    group_a_mode: PpiModeA,
    group_b_mode: PpiModeB,
    port_a_iomode: IoMode,
    port_b_iomode: IoMode,
    port_cu_iomode: IoMode, // Port C Upper IO mode
    port_cl_iomode: IoMode, // Port C Lower IO mode
    port_b_byte: u8,
    port_c_byte: u8,
    dip_sw2: u8, // 1 means on
}

impl PC98SystemPort {
    pub fn new() -> Self {
        Self {
            dip_sw2: DIP_SW2_80_COLUMN
                | DIP_SW2_25_LINE,
            ..Default::default()
        }
    }
}

impl IoDevice for PC98SystemPort {
    fn read_u8(&mut self, port: u16, _delta: DeviceRunTimeUnit) -> u8 {
        //log::trace!("SYSTEM Read from port: {:04X}", port);
        match port {
            SYSTEM_PORT_A => self.handle_porta_read(),
            SYSTEM_PORT_B => self.handle_portb_read(),
            SYSTEM_PORT_C => self.handle_portc_read(),
            SYSTEM_COMMAND_PORT => NO_IO_BYTE,
            _ => panic!("SYSTEM: Bad port #"),
        }
    }

    fn write_u8(&mut self, port: u16, byte: u8, _bus: Option<&mut BusInterface>, _delta: DeviceRunTimeUnit) {
        match port {
            SYSTEM_PORT_A => {
                // Read-only port
            }
            SYSTEM_PORT_B => {
                self.handle_portb_write(byte);
            }
            SYSTEM_PORT_C => {
                self.handle_portc_write(byte);
            }
            SYSTEM_COMMAND_PORT => {
                self.handle_command_port_write(byte);
            }
            _ => panic!("SYSTEM: Bad port #"),
        }
    }

    fn port_list(&self) -> Vec<(String, u16)> {
        vec![
            ("SYSTEM Port A".to_string(), SYSTEM_PORT_A),
            ("SYSTEM Port B".to_string(), SYSTEM_PORT_B),
            ("SYSTEM Port C".to_string(), SYSTEM_PORT_C),
            ("SYSTEM Command".to_string(), SYSTEM_COMMAND_PORT),
        ]
    }
}

impl PC98SystemPort {
    pub fn handle_command_port_write(&mut self, byte: u8) {
        self.control_word = PpiControlWord::from_bytes([byte]);

        if self.control_word.mode_set() {
            self.group_a_mode = self.control_word.group_a_mode();
            self.group_b_mode = self.control_word.group_b_mode();
        }
        log::trace!("SYSTEM: Write to command port: {:02X}", byte);
    }

    pub fn handle_porta_read(&self) -> u8 {
        !self.dip_sw2 // switches are active low
    }

    pub fn handle_portb_read(&self) -> u8 {
        0
    }

    pub fn handle_portc_read(&self) -> u8 {
        0
    }

    pub fn handle_portb_write(&mut self, byte: u8) {
        self.port_b_byte = byte;
    }

    pub fn handle_portc_write(&mut self, byte: u8) {
        self.port_c_byte = byte;
    }
}
