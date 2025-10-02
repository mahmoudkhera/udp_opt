//this file contains all the data needed to test the udp connection
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const HEADER_SIZE: usize = 8 + 8 + 4 + 4; // 24 bytes
pub const FLAG_DATA: u32 = 0;
pub const FLAG_FIN: u32 = 1;

pub struct UdpHeader {
    pub seq: u64,   // sequence number
    pub sec: u64,   // seconds since UNIX_EPOCH
    pub usec: u32,  // microseconds part (0..999_999)
    pub flags: u32, // 0 = data, 1 = FIN (end of test)
}

const ACCEPTABLE: u32 = 99;
const ACCEPTABLEDECIMAL: u32 = 98;

impl UdpHeader {
    pub fn new(seq: u64, sec: u64, usec: u32, flag: u32) -> Self {
        Self {
            seq: seq,
            sec: sec,
            usec: usec,
            flags: flag,
        }
    }
    pub fn write_header(&mut self, buffer: &mut [u8]) {
        assert!(buffer.len() >= HEADER_SIZE);

        buffer[0..8].copy_from_slice(&self.seq.to_be_bytes());
        buffer[8..16].copy_from_slice(&self.sec.to_be_bytes());
        buffer[16..20].copy_from_slice(&self.usec.to_be_bytes());
        buffer[20..24].copy_from_slice(&self.flags.to_be_bytes());
    }

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
// helper functions

pub fn now_micros() -> (u64, u32) {
    let d = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    (d.as_secs(), d.subsec_micros())
}

#[derive(Debug, Clone, Copy, Default)]
pub struct IntervalResult {
    pub received: u64,
    pub lost: u64,
    pub bytes: usize,
    pub jitter_ms: f64,
    pub out_of_order: u64,

    pub recommended_bitrate: u64,
}
#[derive(Debug, Clone, Copy)]
pub struct UdpData {
    pub first_rx_set: bool,
    pub last_seq: Option<u64>,
    pub interval_result: IntervalResult,
    pub prev_transit_ms: Option<f64>,
    //required for calculation of recommended bitrate
    pub period_lost: u64,
    pub period_recived: u64,
    pub recommend_pps: f64,
}

impl UdpData {
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

    pub fn calc_bitrate(&mut self, time: Duration) {
        // Reset early if no packets to avoid div-by-zero
        if self.period_recived == 0 {
            self.recommend_pps = 0.0;
            return;
        }
        // Packets per second (pps)
        let act_pps = self.period_recived as f64 / time.as_secs_f64();
        // Reset early if no packets lost
        if self.period_lost == 0 {
            self.period_recived = 0;
            self.recommend_pps = act_pps;
            return;
        }

        // Compute received ratio once
        let received_ratio =
            ((self.period_recived - self.period_lost) as f64 / self.period_recived as f64) * 100.0;

        // Split into integer + decimal parts
        let int_part = received_ratio as u32; // truncates

        let decimal_part = ((received_ratio * 10000.0) as u32) % 100;

        // Decide recommended adjustment
        let recommended = if int_part < ACCEPTABLE {
            act_pps * 0.95 // reduce rate by 5%
        } else if decimal_part < ACCEPTABLEDECIMAL {
            act_pps + 5.0 // small increase
        } else {
            act_pps - 10.0 // bigger decrease
        };

        // Reset counters
        self.period_lost = 0;
        self.period_recived = 0;

        self.recommend_pps = recommended.max(0.0); // never negative
    }

    pub fn get_interval_result(&mut self) -> IntervalResult {
        let temp = self.interval_result.clone();
        self.interval_result = IntervalResult::default();
        temp
    }
}
