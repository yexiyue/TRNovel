//! Optional synchronized measurements; disabled during throughput acceptance.
use candle_core::{Device, Result};
use std::time::{Duration, Instant};
#[derive(Default, Debug)]
pub struct Timings {
    pub prefill: Duration,
    pub cfm: Duration,
    pub decoder: Duration,
    pub local_encoder: Duration,
    pub backbone: Duration,
    pub residual: Duration,
}
pub(crate) fn measure<T>(
    enabled: bool,
    device: &Device,
    work: impl FnOnce() -> Result<T>,
) -> Result<(T, Duration)> {
    if !enabled {
        return Ok((work()?, Duration::ZERO));
    }
    device.synchronize()?;
    let start = Instant::now();
    let value = work()?;
    device.synchronize()?;
    Ok((value, start.elapsed()))
}
