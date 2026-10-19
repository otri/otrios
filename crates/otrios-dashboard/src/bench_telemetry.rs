// SPDX-License-Identifier: Apache-2.0
//! Opt-in bounded diagnostic; clocks and app values are not written in normal operation.
use std::{
    fs::File,
    io::{BufWriter, Write},
    time::Instant,
};
pub(super) struct Recorder {
    output: Option<BufWriter<File>>,
    samples: usize,
}
impl Recorder {
    pub(super) fn from_env() -> Result<Self, String> {
        let output = std::env::var_os("OTRIOS_BENCH_PRESENT_LOG")
            .map(|path| File::create(path).map(BufWriter::new))
            .transpose()
            .map_err(|e| e.to_string())?;
        Ok(Self { output, samples: 0 })
    }
    pub(super) fn presented(&mut self, id: u64, at: Instant, count: u16) -> Result<(), String> {
        if let Some(output) = &mut self.output
            && self.samples < 8192
        {
            writeln!(
                output,
                "{{\"sample\":{id},\"receive_to_present_us\":{},\"count\":{count}}}",
                at.elapsed().as_micros()
            )
            .map_err(|e| e.to_string())?;
            self.samples += 1;
        }
        Ok(())
    }
}
