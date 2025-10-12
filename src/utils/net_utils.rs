use std::time::Duration;

/// Statistics for a given interval
#[derive(Debug, Clone, Copy, Default)]
pub struct IntervalResult {
    /// Number of packets received
    pub received: u64,
    /// Number of packets lost
    pub lost: u64,
    /// Total bytes received
    pub bytes: usize,
    /// Jitter in milliseconds
    pub jitter_ms: f64,
    /// Number of out-of-order packets
    pub out_of_order: u64,
    /// Recommended bitrate (packets per second)
    pub recommended_bitrate: u64,
    pub time: Duration,
}

/// Commands that control the UDP server behavior.

#[derive(Debug, Clone)]
pub enum ServerCommand {
    Start,
    Stop,
}

/// Commands that control the UDP client behavior.
#[derive(Debug, Clone)]
pub enum ClientCommand {
    Start,
    Stop,
}

pub (crate)fn interval_per_packet(paylod: usize, bitrate: f64) -> Duration {
    let bits_per_packet = (paylod * 8) as f64;
    let packet_per_second = (bitrate / bits_per_packet).max(1.0);

    Duration::from_secs_f64(1.0 / packet_per_second)
}
