/*
    MartyPC Emulator 
    (C)2023 Daniel Balsom
    https://github.com/dbalsom/marty

    This program is free software: you can redistribute it and/or modify
    it under the terms of the GNU General Public License as published by
    the Free Software Foundation, either version 3 of the License, or
    (at your option) any later version.

    This program is distributed in the hope that it will be useful,
    but WITHOUT ANY WARRANTY; without even the implied warranty of
    MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
    GNU General Public License for more details.

    You should have received a copy of the GNU General Public License
    along with this program.  If not, see <https://www.gnu.org/licenses/>.

    --------------------------------------------------------------------------

    sampler.rs

    This module implements a sampler class used to sample an audio device.
    In theory it could be generalized to sample other things.
  
*/

use std::time::{Duration, Instant};

use biquad::*;

use crate::sound::SoundPlayer;

pub enum SampleFilter {
    None,
    Average,
    Lowpass
}

// Main Sampler struct.
// Eventually we will need to move ownership of SoundPlayer to a Mixer device
// to support multiple sound outputs. 
pub struct Sampler {

    sample_rate: f64,
    us_per_sample: f64,
    us_accumulator: f64,
    sec_accumulator: f64,
    samples_per_second: u64,
    submits_per_second: u64,
    avg_sample_ct: u32,
    avg_sample_total: f32,
    last_instant: Instant,
    sample_due: bool,
    filter_type: SampleFilter,
    filter: Option<DirectForm2Transposed::<f32>>,
    player: SoundPlayer,
}

impl Sampler {
    pub fn new(sample_rate: f64, player: SoundPlayer, filter_type: SampleFilter) -> Self {

        let mut filter = None;

        if let SampleFilter::Lowpass = filter_type {

            // Cutoff and sampling frequencies
            let f0 = 8.hz();
            let fs = 1.0.khz();

            let coeffs = Coefficients::<f32>::from_params(Type::LowPass, fs, f0, Q_BUTTERWORTH_F32).unwrap();
            let biquad2 = DirectForm2Transposed::<f32>::new(coeffs);

            filter = Some(biquad2)
        }

        let us_per_sample = 1_000_000.0 / sample_rate;
        log::debug!("Sampler created at sample rate {}, us per sample: {}", sample_rate, us_per_sample);
        
        Self {
            sample_rate,
            us_per_sample,
            us_accumulator: 0.0,
            sec_accumulator: 0.0,
            samples_per_second: 0,         
            submits_per_second: 0,
            avg_sample_ct: 0,
            avg_sample_total: 0.0,               
            last_instant: Instant::now(),
            sample_due: false,
            filter_type,
            filter,
            player
        }
    }

    /// Update the sampler 
    pub fn tick(&mut self, us: f64 ) {

        self.us_accumulator += us;

        if self.us_accumulator > self.us_per_sample {
            self.sample_due = true;
            self.us_accumulator -= self.us_per_sample;
        }
        else {
            self.sample_due = false;
        }

        self.sec_accumulator += us;

        if self.sec_accumulator > 1_000_000.0 {
            // One second has elapsed.

            self.sec_accumulator -= 1_000_000.0;
            log::debug!("Samples per sec: {} submits: {} last elapsed: {} us_accumulator {} avg_total {} avg_ct {}", self.samples_per_second, self.submits_per_second, us, self.us_accumulator, self.avg_sample_total, self.avg_sample_ct);
            self.samples_per_second = 0;
            self.submits_per_second = 0;
        }
    }

    /// A device being sampled calls this method to submit a sample at an arbitrary rate.
    /// The sampler itself determines whether the sample should be ignored or processed.
    pub fn submit(&mut self, sample: f32) {

        self.submits_per_second += 1;

        match self.filter_type {
            SampleFilter::None => {    
                if self.sample_due {

                    if sample != 0.0 {
                        //log::debug!("Q sample: {}", sample);
                    }                    
                    self.player.queue_sample(sample);
                    self.samples_per_second = self.samples_per_second.wrapping_add(1);
                    //self.sample_due = false;
                }
            }
            SampleFilter::Average => {

                self.avg_sample_total += sample;
                self.avg_sample_ct += 1;

                if self.sample_due {
                    let avg_sample = self.avg_sample_total / (self.avg_sample_ct as f32);
                    
                    //if avg_sample > 0.5 {
                    //    log::debug!("t: {} c: {} s: {}", self.avg_sample_total, self.avg_sample_ct, avg_sample);
                    //    panic!("foo");
                    //}
                        

                    
                    self.player.queue_sample(avg_sample);

                    self.avg_sample_ct = 0;
                    self.avg_sample_total = 0.0;
                    //self.sample_due = false;
                }
            }
            SampleFilter::Lowpass => {
                // Pass every sample through filter, but only submit when due.
                
                let filtered_sample = self.filter.unwrap().run(sample);

                if self.sample_due {
                    if filtered_sample > 0.50 {
                        log::debug!("Q sample: {} {}", sample, filtered_sample);
                    }

                    self.player.queue_sample(filtered_sample);
                    //self.player.queue_sample(self.filter.unwrap().run(sample));
                    self.samples_per_second = self.samples_per_second.wrapping_add(1);
                    self.sample_due = false;
                }
            }
        }
    }

    /// Begin playing the sound device
    pub fn play(&self) {
        self.player.play();
    }
}
