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

    devices::cga::mmio.rs

    Implementation of the MMIO interface for PC98 graphics.

*/

use super::*;
use crate::bus::{MemRangeDescriptor, MemoryMappedDevice};

impl MemoryMappedDevice for PC98Graphics {
    fn get_read_wait(&mut self, _address: usize, _cycles: u32) -> u32 {
        0
    }

    fn get_write_wait(&mut self, _address: usize, _cycles: u32) -> u32 {
        0
    }

    fn mmio_read_u8(&mut self, address: usize, _cycles: u32, _cpumem: Option<&[u8]>) -> (u8, u32) {
        match address {
            0xa0000..=0xa3fff =>
                (self.tvmem[address - 0xa0000], 0),
            0xa8000..=0xbffff =>
                (self.gvmem[address - 0xa8000], 0),
            0xe0000..=0xe7fff =>
                (self.gvmem[address - 0xe0000], 0),
            _ => unreachable!()
        }
    }

    fn mmio_peek_u8(&self, address: usize, _cpumem: Option<&[u8]>) -> u8 {
        match address {
            0xa0000..=0xa3fff =>
                self.tvmem[address - 0xa0000],
            0xa8000..=0xbffff =>
                self.gvmem[address - 0xa8000],
            0xe0000..=0xe7fff =>
                self.gvmem[address - 0xe0000],
            _ => unreachable!()
        }
    }

    fn mmio_peek_u16(&self, address: usize, cpumem: Option<&[u8]>) -> u16 {
        let lo_byte = MemoryMappedDevice::mmio_peek_u8(self, address, cpumem);
        let ho_byte = MemoryMappedDevice::mmio_peek_u8(self, address + 1, cpumem);
        return (ho_byte as u16) << 8 | lo_byte as u16;
    }

    fn mmio_write_u8(&mut self, address: usize, byte: u8, _cycles: u32, _cpumem: Option<&mut [u8]>) -> u32 {
        match address {
            0xa0000..=0xa3fff =>
                self.tvmem[address - 0xa0000] = byte,
            0xa8000..=0xbffff =>
                self.gvmem[address - 0xa8000] = byte,
            0xe0000..=0xe7fff =>
                self.gvmem[address - 0xe0000] = byte,
            _ => unreachable!()
        }
        0
    }

    fn mmio_read_u16(&mut self, address: usize, _cycles: u32, cpumem: Option<&[u8]>) -> (u16, u32) {
        let (lo_byte, wait1) = MemoryMappedDevice::mmio_read_u8(self, address, 0, cpumem);
        let (ho_byte, wait2) = MemoryMappedDevice::mmio_read_u8(self, address + 1, 0, cpumem);
        return ((ho_byte as u16) << 8 | lo_byte as u16, wait1 + wait2);
    }

    fn mmio_write_u16(&mut self, address: usize, data: u16, cycles: u32, _cpumem: Option<&mut [u8]>) -> u32 {
        MemoryMappedDevice::mmio_write_u8(self, address, data as u8, cycles, None) +
            MemoryMappedDevice::mmio_write_u8(self, address + 1, (data >> 8) as u8, cycles, None)
        //trace!(self, "16 byte write to VRAM, {:04X} -> {:05X} ", data, address);
    }

    fn get_mapping(&self) -> Vec<MemRangeDescriptor> {
        let mut mapping = Vec::new();

        mapping.push(MemRangeDescriptor {
            address: 0xA0000,
            size: 0x4000,
            cycle_cost: 0,
            read_only: false,
            priority: 0,
        });
        mapping.push(MemRangeDescriptor {
            address: 0xA8000,
            size: 0x18000,
            cycle_cost: 0,
            read_only: false,
            priority: 0,
        });
        mapping.push(MemRangeDescriptor {
            address: 0xE0000,
            size: 0x8000,
            cycle_cost: 0,
            read_only: false,
            priority: 0,
        });

        mapping
    }
}
