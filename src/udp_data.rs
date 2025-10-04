//! # UDP Data Structures and Utilities
//!
//! This module contains all the data structures and helper functions required
//! for testing and monitoring UDP connections, including packet headers,
//! jitter calculation, loss detection, and recommended bitrate estimation.
//!
//! It is used by the UDP client and server to process incoming/outgoing packets
//! and generate per-interval statistics.
//!
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Size of the UDP header in bytes (seq + sec + usec + flags)
pub const HEADER_SIZE: usize = 8 + 8 + 4 + 4; // 24 bytes

/// Flag indicating a data packet
pub const FLAG_DATA: u32 = 0;
/// Flag indicating the end of a test (FIN)
pub const FLAG_FIN: u32 = 1;

/// Represents the header of a UDP packet
pub struct UdpHeader {
    pub seq: u64,   // sequence number
    pub sec: u64,   // seconds since UNIX_EPOCH
    pub usec: u32,  // microseconds part (0..999_999)
    pub flags: u32, // 0 = data, 1 = FIN (end of test)
}

const ACCEPTABLE: u32 = 99;
const ACCEPTABLEDECIMAL: u32 = 98;

impl UdpHeader {
    /// Creates a new `UdpHeader`
    ///
    /// # Parameters
    /// - `seq`: sequence number
    /// - `sec`: seconds since UNIX_EPOCH
    /// - `usec`: microseconds part
    /// - `flag`: packet type (`FLAG_DATA` or `FLAG_FIN`)   
    pub fn new(seq: u64, sec: u64, usec: u32, flag: u32) -> Self {
        Self {
            seq: seq,
            sec: sec,
            usec: usec,
            flags: flag,
        }
    }

    /// Writes the header into a buffer (big-endian)
    ///
    /// # Panics
    /// Panics if the buffer length is smaller than `HEADER_SIZE`
    pub fn write_header(&mut self, buffer: &mut [u8]) {
        assert!(buffer.len() >= HEADER_SIZE);

        buffer[0..8].copy_from_slice(&self.seq.to_be_bytes());
        buffer[8..16].copy_from_slice(&self.sec.to_be_bytes());
        buffer[16..20].copy_from_slice(&self.usec.to_be_bytes());
        buffer[20..24].copy_from_slice(&self.flags.to_be_bytes());
    }

    /// Reads a `UdpHeader` from a buffer (big-endian)
    ///
    /// # Panics
    /// Panics if the buffer is smaller than `HEADER_SIZE`.
    pub fn read_header(buffer: &mut [u8]) -> Self {
        let seq = u64::from_be_bytes(buffer[0..8].try_into().unwrap());
        let sec = u64::from_be_bytes(buffer[8..16].try_into().unwrap());
        let usec = u32::from_be_bytes(buffer[16..20].try_into().unwrap());
        let flags = u32::from_be_bytes(buffer[20..24].try_into().unwrap());
        Self {
            seq,
            sec,
            usec,
            flags,
        }
    }
}

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

/// Tracks UDP statistics and state for a connection
#[derive(Debug, Clone, Copy)]
pub struct UdpData {
    /// Indicates if the first packet has been received
    pub first_rx_set: bool,
    /// Last received sequence number
    pub last_seq: Option<u64>,
    /// Interval statistics
    pub interval_result: IntervalResult,
    /// Previous packet transit time (ms)
    pub prev_transit_ms: Option<f64>,
    /// Lost packets in current period
    pub period_lost: u64,
    /// Received packets in current period
    pub period_recived: u64,
    /// Recommended packets per second
    pub recommend_pps: f64,
}

impl UdpData {
    /// Creates a new `UdpData` instance

    pub fn new() -> Self {
        Self {
            first_rx_set: false,
            last_seq: None,
            interval_result: IntervalResult::default(),
            prev_transit_ms: None,
            period_lost: 0,
            period_recived: 0,
            recommend_pps: 0.0,
        }
    }

    /// Processes a received packet, updates statistics and jitter
    ///
    /// # Parameters
    /// - `packet_len`: length of the packet in bytes
    /// - `h`: reference to the packet header
    /// - `now_since_start`: elapsed time since server start
    pub fn process_packet(&mut self, packet_len: usize, h: &UdpHeader, now_since_start: Duration) {
        self.interval_result.received += 1;
        self.interval_result.bytes += packet_len;
        //  determine losses ,out of order
        match self.last_seq {
            None => self.last_seq = Some(h.seq),

            Some(prev) => {
                if h.seq == prev {
                    //duplicate packet
                } else if h.seq == prev + 1 {
                    //set the last accepted sequence to be packet sequnce
                    self.last_seq = Some(h.seq);
                } else if h.seq > (prev + 1) {
                    // when the header sequence is bigger than the previous sequence +1
                    self.interval_result.lost = h.seq - (prev + 1);

                    self.last_seq = Some(h.seq);
                } else {
                    // out of order happend when h.seq<prev
                    self.interval_result.out_of_order += 1;
                }
            }
        }

        //proccess jitter
        // Jitter per RFC3550 (in milliseconds)
        // And read https://support.spirent.com/s/article/FAQ13756
        // Not that  send_ms uses sender's clock (may differ from server), but jitter is based on differences
        // There is no need for NTP

        let send_ms = (h.sec as f64) * 1000.0 + (h.usec as f64) / 1000.0;
        let arrival_ms = now_since_start.as_secs_f64() * 1000.0; // relative to server start
        let transit = arrival_ms - send_ms;
        if let Some(prev_t) = self.prev_transit_ms {
            let d = (transit - prev_t).abs();
            self.interval_result.jitter_ms += (d - self.interval_result.jitter_ms) / 16.0;
        }
        self.prev_transit_ms = Some(transit);
    }

    // custom conjection control

    /// Calculates recommended bitrate based on packet loss and interval duration
    ///
    /// # Parameters
    /// - `time`: duration of the measurement period
    pub fn calc_bitrate(&mut self, time: Duration) {
        let received = self.interval_result.received;
        let lost = self.interval_result.lost;
        // Reset early if no packets to avoid div-by-zero
        if received == 0 {
            self.recommend_pps = 0.0;
            return;
        }
        // Packets per second (pps)
        let act_pps = received as f64 / time.as_secs_f64();
        // Reset early if no packets lost
        if lost == 0 {
            return;
        }

        let interval_secs = time.as_secs_f64();
        if interval_secs <= f64::EPSILON {
            return;
        }

        // Compute received ratio once
        let received_ratio = ((received - lost) as f64 / received as f64) * 100.0;

        // Split into integer + decimal parts
        let int_part = received_ratio as u32; // truncates

        let decimal_part = ((received_ratio * 10000.0) as u32) % 100;

        // Decide recommended adjustment
        let recommended = if int_part < ACCEPTABLE {
            act_pps * 0.95 // reduce rate by 5%
        } else if decimal_part >= ACCEPTABLEDECIMAL {
            act_pps + 5.0 // small increase
        } else {
            act_pps - 10.0 // bigger decrease
        };

        self.recommend_pps = recommended.max(0.0); // never negative
    }

    /// Returns interval statistics and resets them

    pub fn get_interval_result(&mut self) -> IntervalResult {
        let r = std::mem::take(&mut self.interval_result);
        r
    }
}

// helper functions

/// Returns the current system time as seconds + microseconds since UNIX_EPOCH

pub fn now_micros() -> (u64, u32) {
    let d = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    (d.as_secs(), d.subsec_micros())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_udp_header_new() {
        let header = UdpHeader::new(12345, 1000000, 500000, FLAG_DATA);

        assert_eq!(header.seq, 12345);
        assert_eq!(header.sec, 1000000);
        assert_eq!(header.usec, 500000);
        assert_eq!(header.flags, FLAG_DATA);
    }

    #[test]
    fn test_udp_header_write_and_read() {
        let mut buffer = vec![0u8; HEADER_SIZE];
        let mut original = UdpHeader::new(42, 1234567890, 999999, FLAG_FIN);

        // Write header to buffer
        original.write_header(&mut buffer);

        // Read it back
        let read_header = UdpHeader::read_header(&mut buffer);

        assert_eq!(read_header.seq, 42);
        assert_eq!(read_header.sec, 1234567890);
        assert_eq!(read_header.usec, 999999);
        assert_eq!(read_header.flags, FLAG_FIN);
    }

    #[test]
    #[should_panic]
    fn test_udp_header_write_buffer_too_small() {
        let mut buffer = vec![0u8; HEADER_SIZE - 1];
        let mut header = UdpHeader::new(1, 2, 3, 4);

        header.write_header(&mut buffer); // Should panic
    }

    #[test]
    fn test_interval_result_default() {
        let result = IntervalResult::default();

        assert_eq!(result.received, 0);
        assert_eq!(result.lost, 0);
        assert_eq!(result.bytes, 0);
        assert_eq!(result.jitter_ms, 0.0);
        assert_eq!(result.out_of_order, 0);
        assert_eq!(result.recommended_bitrate, 0);
        assert_eq!(result.time, Duration::ZERO);
    }

    #[test]
    fn test_udp_data_new() {
        let data = UdpData::new();

        assert_eq!(data.first_rx_set, false);
        assert_eq!(data.last_seq, None);
        assert_eq!(data.interval_result.received, 0);
        assert_eq!(data.prev_transit_ms, None);
        assert_eq!(data.period_lost, 0);
        assert_eq!(data.period_recived, 0);
        assert_eq!(data.recommend_pps, 0.0);
    }

    #[test]
    fn test_process_packet_jitter_calculation() {
        let mut data = UdpData::new();

        // First packet - establishes baseline
        let h1 = UdpHeader::new(0, 1000, 0, FLAG_DATA);
        data.process_packet(1500, &h1, Duration::from_millis(100));

        assert!(data.prev_transit_ms.is_some());
        assert_eq!(data.interval_result.jitter_ms, 0.0); // No jitter yet

        // Second packet - should calculate jitter
        let h2 = UdpHeader::new(1, 1000, 50000, FLAG_DATA);
        data.process_packet(1500, &h2, Duration::from_millis(200));

        // Jitter should be non-zero now
        assert!(data.interval_result.jitter_ms > 0.0);
    }

    #[test]
    fn test_process_multiple_packets() {
        let mut data = UdpData::new();

        // Simulate receiving 10 packets with one loss and one out-of-order
        for i in 0..10 {
            if i == 3 {
                continue; // Skip packet 3 (loss)
            }

            let seq = if i == 4 {
                5
            } else if i == 5 {
                4
            } else {
                i
            }; // 4 and 5 swapped
            let header = UdpHeader::new(seq, 1000 + i, (i * 1000) as u32, FLAG_DATA);
            data.process_packet(1500, &header, Duration::from_millis(i * 100));
        }

        assert_eq!(data.interval_result.received, 9); // Received 9 out of 10
        assert_eq!(data.interval_result.bytes, 13500); // 9 * 1500
        assert!(data.interval_result.lost > 0); // Should detect loss
        assert_eq!(data.interval_result.out_of_order, 1); // One out-of-order
    }

    #[test]
    fn test_large_sequence_numbers() {
        let mut data = UdpData::new();

        let large_seq = u64::MAX - 10;
        let h1 = UdpHeader::new(large_seq, 1000, 0, FLAG_DATA);
        data.process_packet(1500, &h1, Duration::from_secs(1));

        let h2 = UdpHeader::new(large_seq + 1, 1000, 1000, FLAG_DATA);
        data.process_packet(1500, &h2, Duration::from_secs(1));

        assert_eq!(data.last_seq, Some(large_seq + 1));
        assert_eq!(data.interval_result.lost, 0);
    }

    #[test]
    fn test_calc_bitrate_high_loss() {
        let mut data = UdpData::new();
        data.interval_result.received = 1000;
        data.interval_result.lost = 100; // 10% loss

        data.calc_bitrate(Duration::from_secs(1));

        // High loss (below 99% received), should reduce rate by 5%
        // act_pps = 1000/1 = 1000
        // recommended = 1000 * 0.95 = 950
        assert_eq!(data.recommend_pps, 950.0);
    }

    #[test]
    fn test_calc_bitrate_low_loss_acceptable() {
        let mut data = UdpData::new();
        data.interval_result.received = 1000;
        data.interval_result.lost = 5; // 0.5% loss, very good

        data.calc_bitrate(Duration::from_secs(1));

        // received_ratio = ((1000-5)/1000)*100 = 99.5%
        // int_part = 99 (>= ACCEPTABLE)
        // decimal_part = 50 (>= ACCEPTABLEDECIMAL)
        // Should decrease by 10
        // act_pps = 1000
        // recommended = 1000 - 10 = 990
        assert_eq!(data.recommend_pps, 990.0);
    }

    #[test]
    fn test_calc_bitrate_very_low_loss() {
        let mut data = UdpData::new();
        data.interval_result.received = 10000;
        data.interval_result.lost = 1; // 0.01% loss

        data.calc_bitrate(Duration::from_secs(1));

        // received_ratio = ((10000-1)/10000)*100 = 99.99%
        // int_part = 99 (>= ACCEPTABLE)
        // decimal_part = 99 (>= ACCEPTABLEDECIMAL)
        // Should decrease by 10
        // act_pps = 10000
        // recommended = 10000 - 10 = 9990
        assert_eq!(data.recommend_pps, 9990.0);
    }

    #[test]
    fn test_calc_bitrate_non_one_second_interval() {
        let mut data = UdpData::new();
        data.interval_result.received = 500;
        data.interval_result.lost = 50; // 10% loss

        data.calc_bitrate(Duration::from_millis(500)); // 0.5 seconds

        // act_pps = 500 / 0.5 = 1000 pps
        // High loss, reduce by 5%
        // recommended = 1000 * 0.95 = 950
        assert_eq!(data.recommend_pps, 950.0);
    }

    #[test]
    fn test_get_interval_result() {
        let mut data = UdpData::new();

        // Add some data
        data.interval_result.received = 100;
        data.interval_result.bytes = 150000;
        data.interval_result.lost = 5;
        data.interval_result.jitter_ms = 2.5;
        data.interval_result.out_of_order = 3;

        // Get and reset
        let result = data.get_interval_result();

        // Check returned values
        assert_eq!(result.received, 100);
        assert_eq!(result.bytes, 150000);
        assert_eq!(result.lost, 5);
        assert_eq!(result.jitter_ms, 2.5);
        assert_eq!(result.out_of_order, 3);

        // Check that original is reset
        assert_eq!(data.interval_result.received, 0);
        assert_eq!(data.interval_result.bytes, 0);
        assert_eq!(data.interval_result.lost, 0);
        assert_eq!(data.interval_result.jitter_ms, 0.0);
        assert_eq!(data.interval_result.out_of_order, 0);
    }
}
