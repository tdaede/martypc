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

    devices::pc98_graphics::videocard.rs

    Implements the VideoCard trait for the NEC PC-98 graphics subsystem.

*/

use super::*;
use crate::{device_traits::videocard::*, devices::pic::Pic};
use encoding_rs::*;

// Helper macro for pushing video card state entries.
// For CGA, we put the decorator first as there is only one register file an we use it to show the register index.
macro_rules! push_reg_str {
    ($vec: expr, $reg: expr, $decorator: expr, $val: expr ) => {
        $vec.push((
            format!("{} {:?}", $decorator, $reg),
            VideoCardStateEntry::String(format!("{}", $val)),
        ))
    };
}

/*
macro_rules! push_reg_str_bin8 {
    ($vec: expr, $reg: expr, $decorator: expr, $val: expr ) => {
        $vec.push((String::from("{:?} {}", $reg, $decorator), VideoCardStateEntry::String(format!("{:08b}", $val))))
    };
}

macro_rules! push_reg_str_enum {
    ($vec: expr, $reg: expr, $decorator: expr, $val: expr ) => {
        $vec.push((String::from("{:?} {}", $reg, $decorator), VideoCardStateEntry::String(format!("{:?}", $val))))
    };
}
*/

impl VideoCard for PC98Graphics {
    fn get_sync(&self) -> (bool, bool, bool, bool) {
        (
            false,
            false,
            true,
            false,
        )
    }

    fn set_video_option(&mut self, opt: VideoOption) {
    }

    fn get_video_type(&self) -> VideoType {
        VideoType::PC98
    }

    fn get_render_mode(&self) -> RenderMode {
        RenderMode::Direct
    }

    fn get_render_depth(&self) -> RenderBpp {
        // TODO: should be 24 bit output due to palettes
        RenderBpp::Four
    }

    fn get_display_mode(&self) -> DisplayMode {
        DisplayMode::Mode13VGALowRes256 // TODO: hack
    }

    fn set_clocking_mode(&mut self, mode: ClockingMode) {
        // TODO: Switching from cycle clocking mode to character clocking mode
        // must be deferred until character-clock boundaries.
        // For now we only support falling back to cycle clocking mode and
        // staying there.
        log::debug!("Clocking mode set to: {:?}", mode);
        self.clock_mode = mode;
    }

    fn get_display_size(&self) -> (u32, u32) {
        (640, 400)
    }

    fn get_display_extents(&self) -> &DisplayExtents {
        &self.extents
    }

    fn list_display_apertures(&self) -> Vec<DisplayApertureDesc> {
        PC98_APERTURE_DESCS.to_vec()
    }

    fn get_display_apertures(&self) -> Vec<DisplayAperture> {
        PC98_APERTURES.to_vec()
    }

    /// Get the position of the electron beam.
    fn get_beam_pos(&self) -> Option<(u32, u32)> {
        Some((self.beam_x, self.beam_y))
    }

    fn is_40_columns(&self) -> bool {
        false
    }

    fn get_cursor_info(&self) -> CursorInfo {
        CursorInfo {
            addr: 0,
            pos_x: 0,
            pos_y: 0,
            line_start: 0,
            line_end: 0,
            visible: false,
        }
    }

    fn dump_mem(&self, _path: &Path) {
    }

    fn debug_tick(&mut self, _ticks: u32, _cpumem: Option<&[u8]>) {
    }

    fn get_text_mode_strings(&self) -> Vec<String> {
        let mut strings = vec![];
        for line_bytes in self.tvmem[0..80*25*2].chunks(80*2) {
            let (cow, _, _) = ISO_2022_JP.decode(line_bytes);
            strings.push(cow.to_string().replace("\n", "␍"));
        }
        strings
    }

    #[inline]
    fn get_overscan_color(&self) -> u8 {
        0
    }

    /// Get the current scanline being rendered.
    fn get_scanline(&self) -> u32 {
        self.scanline
    }

    /// Return whether to double scanlines for this video device.
    fn get_scanline_double(&self) -> bool {
        false
    }

    /// Return the u8 slice representing the requested buffer type.
    fn get_buf(&self, buf_select: BufferSelect) -> &[u8] {
        match buf_select {
            BufferSelect::Back => &self.buf[self.back_buf][..],
            BufferSelect::Front => &self.buf[self.front_buf][..],
        }
    }

    /// Return the u8 slice representing the front buffer of the device. (Direct rendering only)
    fn get_display_buf(&self) -> &[u8] {
        &self.buf[self.front_buf][..]
    }

    /// Get the current display refresh rate of the device. For CGA, this is always 60.
    fn get_refresh_rate(&self) -> u32 {
        56 // actually 56.4
    }


    fn get_clock_divisor(&self) -> u32 {
        1
    }

    fn get_current_font(&self) -> Option<FontInfo> {
        None
    }

    fn get_character_height(&self) -> u8 {
        0
    }

    #[inline]
    fn is_graphics_mode(&self) -> bool {
        true
    }

    #[rustfmt::skip]
    fn get_videocard_string_state(&self) -> HashMap<String, Vec<(String, VideoCardStateEntry)>> {
        let map = HashMap::new();

        map
    }

    fn run(&mut self, time: DeviceRunTimeUnit, pic: &mut Option<Pic>, _cpumem: Option<&[u8]>) {

        let ticks = if let DeviceRunTimeUnit::Microseconds(us) = time {
            us * GDC_WCLK
        }
        else {
            panic!("PC98 graphics requires Microseconds time unit.");
        };
        self.do_ticks(ticks, pic);
    }

    fn reset(&mut self) {
        log::debug!("Resetting");
    }

    fn get_pixel(&self, _x: u32, _y: u32) -> &[u8] {
        &DUMMY_PIXEL
    }

    fn get_pixel_raw(&self, _x: u32, _y: u32) -> u8 {
        0
    }

    fn get_start_address(&self) -> u16 {
        return 0;
    }

    fn get_plane_slice(&self, _plane: usize) -> &[u8] {
        &DUMMY_PLANE
    }

    fn get_frame_count(&self) -> u64 {
        self.frame_count
    }

    fn write_trace_log(&mut self, _msg: String) {
    }

    fn trace_flush(&mut self) {
    }

    fn get_palette(&self) -> Option<Vec<[u8;4]>> {
        None
    }
}
