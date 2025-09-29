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

#[derive(Debug, Clone)]
pub struct UdpData {
    pub first_rx_set: bool,
    pub last_seq: Option<u64>,
    pub received: u64,
    pub lost: u64,
    pub out_of_order: u64,
    pub bytes: usize,
    pub jitter_ms: f64,
    pub prev_transit_ms: Option<f64>,
}

impl UdpData {
    pub fn new() -> Self {
        Self {
            first_rx_set: false,
            last_seq: None,
            received: 0,
            lost: 0,
            out_of_order: 0,
            bytes: 0,
            jitter_ms: 0.0,
            prev_transit_ms: None,
        }
    }

    pub fn process_packet(&mut self, packet_len: usize, h: &UdpHeader, now_since_start: Duration) {
        self.received += 1;
        self.bytes += packet_len;
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
                    self.lost = h.seq - (prev + 1);
                    self.last_seq = Some(h.seq);
                } else {
                    // out of order happend when h.seq<prev
                    self.out_of_order += 1;
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
            self.jitter_ms += (d - self.jitter_ms) / 16.0;
        }
        self.prev_transit_ms = Some(transit);
    }

    // custom conjection control
}
