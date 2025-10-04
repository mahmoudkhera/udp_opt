use crate::udp_data::IntervalResult;
use std::time::Duration;

/// Final aggregated test statistics computed from a list of `IntervalResult`s.
#[derive(Debug, Clone)]
pub struct TestResult {
    /// Total number of packets received across all intervals.
    pub total_packets: u64,
    /// Total number of packets lost across all intervals.
    pub total_lost: u64,
    /// Total number of bytes received across all intervals.
    pub total_bytes: usize,
    /// Total duration of the test (in seconds).
    pub total_time: f64,
    /// Total duration of the test (in seconds).
    pub total_out_of_order: u64,

    /// Mean bitrate over all intervals (bits/sec).
    pub mean_bitrate: f64,
    /// Median bitrate over all intervals (bits/sec).
    pub median_bitrate: f64,

    /// Mean jitter over all intervals (ms).
    pub mean_jitter: f64,
    /// Median jitter over all intervals (ms).
    pub median_jitter: f64,
}

impl TestResult {
    /// Aggregate multiple interval results into a single test result summary.
    ///
    /// # Arguments
    /// * `intervals` - A list of per-interval measurement results.
    ///
    /// # Returns
    /// A `TestResult` containing total counts and statistical measures such as mean and median.
    pub fn from_intervals(intervals: &[IntervalResult]) -> Self {
        if intervals.is_empty() {
            return Self {
                total_packets: 0,
                total_lost: 0,
                total_bytes: 0,
                total_time: 0.0,
                total_out_of_order: 0,
                mean_bitrate: 0.0,
                median_bitrate: 0.0,
                mean_jitter: 0.0,
                median_jitter: 0.0,
            };
        }

        let n = intervals.len();
        let mut bitrates = Vec::with_capacity(n);
        let mut jitters = Vec::with_capacity(n);

        let mut total_received = 0u64;
        let mut total_lost = 0u64;
        let mut total_bytes = 0usize;
        let mut total_time = Duration::ZERO;
        let mut total_out_of_order = 0;

        // Compute totals and collect per-interval stats in one pass
        for i in intervals {
            total_received += i.received;
            total_lost += i.lost;
            total_bytes += i.bytes;
            total_out_of_order = i.out_of_order;

            bitrates.push((i.bytes * 8) as f64 / i.time.as_secs_f64());
            jitters.push(i.jitter_ms);
            total_time += i.time
        }

        let mean_bitrate = mean(&bitrates);
        let mean_jitter = mean(&jitters);
        let median_bitrate = median_f64(&mut bitrates);
        let median_jitter = median_f64(&mut jitters);

        Self {
            total_packets: total_received,
            total_lost: total_lost,
            total_bytes: total_bytes,
            total_time: total_time.as_secs_f64(),
            total_out_of_order: total_out_of_order,
            mean_bitrate: mean_bitrate,
            median_bitrate: median_bitrate,
            mean_jitter: mean_jitter,
            median_jitter: median_jitter,
        }
    }
}

/// The mean is the sum of a collection of numbers divided by the number of numbers in the collection.
/// (reference)[http://en.wikipedia.org/wiki/Arithmetic_mean]
pub fn mean(v: &[f64]) -> f64 {
    if v.is_empty() {
        return 0.0;
    }

    let sum: f64 = v.iter().copied().sum();
    sum / v.len() as f64
}

// The median is the number separating the higher half of a data sample, a population, or
/// a probability distribution, from the lower half (reference)[http://en.wikipedia.org/wiki/Median)
pub fn median_f64(v: &mut [f64]) -> f64 {
    if v.is_empty() {
        return 0.0;
    }

    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mid = v.len() / 2;

    if v.len() % 2 == 1 {
        v[mid]
    } else {
        (v[mid - 1] + v[mid]) / 2.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::udp_data::IntervalResult;
    use std::time::Duration;

    // Helper function to create a test interval
    fn create_interval(
        received: u64,
        lost: u64,
        bytes: usize,
        time_ms: u64,
        jitter_ms: f64,
        out_of_order: u64,
    ) -> IntervalResult {
        IntervalResult {
            received,
            lost,
            bytes,
            time: Duration::from_millis(time_ms),
            jitter_ms,
            out_of_order,
            recommended_bitrate: 0,
        }
    }

    #[test]
    fn test_from_intervals() {
        let intervals = vec![
            create_interval(100, 0, 8000, 1000, 1.0, 0),
            create_interval(100, 0, 16000, 1000, 2.0, 1),
            create_interval(100, 0, 24000, 1000, 3.0, 2),
            create_interval(100, 0, 32000, 1000, 4.0, 3),
        ];

        let result = TestResult::from_intervals(&intervals);

        assert_eq!(result.total_packets, 400);
        assert_eq!(result.total_lost, 0);
        assert_eq!(result.total_bytes, 80000);
        assert_eq!(result.total_out_of_order, 3);

        // Bitrates: 64000, 128000, 192000, 256000
        assert_eq!(result.mean_bitrate, 160000.0);
        assert_eq!(result.median_bitrate, 160000.0);

        // Jitters: 1.0, 2.0, 3.0, 4.0
        assert_eq!(result.mean_jitter, 2.5);
        assert_eq!(result.median_jitter, 2.5);
    }
}
