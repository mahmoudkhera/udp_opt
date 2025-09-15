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

// helper functions
pub fn write_header(buffer: &mut [u8], header: &UdpHeader) {
    assert!(buffer.len() >= HEADER_SIZE);

    buffer[0..8].copy_from_slice(&header.seq.to_be_bytes());
    buffer[8..16].copy_from_slice(&header.sec.to_be_bytes());
    buffer[16..20].copy_from_slice(&header.usec.to_be_bytes());
    buffer[20..24].copy_from_slice(&header.flags.to_be_bytes());
}

pub fn read_header(buffer: &mut [u8]) -> UdpHeader {
    let seq = u64::from_be_bytes(buffer[0..8].try_into().unwrap());
    let sec = u64::from_be_bytes(buffer[8..16].try_into().unwrap());
    let usec = u32::from_be_bytes(buffer[16..20].try_into().unwrap());
    let flags = u32::from_be_bytes(buffer[20..24].try_into().unwrap());
    UdpHeader {
        seq,
        sec,
        usec,
        flags,
    }
}

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
        // println!("seqnce {}",h.seq);
        //  determine losses ,out of order
        match self.last_seq {
            None => self.last_seq = Some(h.seq),

            Some(prev) => {
                if h.sec == prev {
                    //duplicate packet
                } else if h.seq == prev + 1 {
                    //set the last accepted sequence to be packet sequnce
                    self.last_seq = Some(h.seq);
                } else if h.seq > (prev + 1) {
                    // when the header sequenc is bigger than the previous sequenc +1
                    self.lost = h.seq - (prev + 1);
                    self.last_seq = Some(h.sec);
                } else {
                    // out of order happend when h.seq<prev
                    self.out_of_order += 1;
                }
            }
        }

        //proccess jitter

        let send_ms = (h.sec as f64) * 1000.0 + (h.usec as f64) / 1000.0;
        let arrival_ms = now_since_start.as_secs_f64() * 1000.0; // relative to server start
        let transit = arrival_ms - send_ms;
        if let Some(prev_t) = self.prev_transit_ms {
            let d = (transit - prev_t).abs();
            self.jitter_ms += (d - self.jitter_ms) / 16.0;
        }
        self.prev_transit_ms = Some(transit);
    }
}
