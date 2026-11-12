//! Linux sampling-test calibration, independent of rings, unwinding and aggregation.
//! The sampler itself is Linux-only. Dependent test crates opt in with the
//! `sampling-test-support` feature and gate their sampling tests on Linux.

use std::sync::OnceLock;

/// Effective CPU timer deliveries per second when requesting 1 kHz on this kernel.
/// A dedicated unregistered thread counts raw signals using the sampler's existing
/// once-installed handler. It never replaces handlers owned by concurrent tests.
pub fn cpu_hz() -> f64 {
    static HZ: OnceLock<f64> = OnceLock::new();
    *HZ.get_or_init(|| {
        std::thread::spawn(super::os::measure_cpu_hz)
            .join()
            .expect("CPU timer calibration worker panicked")
            .expect("CPU timer calibration failed")
    })
}

/// CPU workload duration targeting the number of timer deliveries that `nominal_ms`
/// would produce at 1 kHz. Saturation fixtures keep their fixed ring sizes and drop
/// bounds, but run enough CPU work to exercise the same number of signal deliveries.
pub fn cpu_ms_for_samples(nominal_ms: u64) -> u64 {
    (nominal_ms as f64 * 1000.0 / cpu_hz()).ceil() as u64
}

/// Scale a CPU sample-count bound specified for a requested 1 kHz timer.
/// Ceil and a nonzero minimum prevent calibration from erasing small bounds.
/// Wall-clock sample bounds and behavioral proportions must not use this scale.
pub fn cpu_count(nominal_at_1000_hz: u64) -> u64 {
    let scaled = (nominal_at_1000_hz as f64 * cpu_hz() / 1000.0).ceil() as u64;
    if nominal_at_1000_hz == 0 {
        0
    } else {
        scaled.max(1)
    }
}
