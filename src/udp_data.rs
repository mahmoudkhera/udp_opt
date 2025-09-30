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

#[derive(Debug, Clone)]
pub struct UdpData {
    pub first_rx_set: bool,
    pub last_seq: Option<u64>,
    pub received: u64,
    pub lost: u64,
    pub bytes: usize,
    pub jitter_ms: f64,
    pub recommended_bitrate: u64,
    pub prev_transit_ms: Option<f64>,
    //required for calculation of recommended bitrate
    pub period_lost: u64,
    pub period_recived: u64,
    pub out_of_order: u64,
    pub recommend_pps: f64,
}

impl UdpData {
    pub fn new() -> Self {
        Self {
            first_rx_set: false,
            last_seq: None,
            received: 0,
            lost: 0,
            period_lost: 0,
            period_recived: 0,
            out_of_order: 0,
            bytes: 0,
            jitter_ms: 0.0,
            recommended_bitrate: 0,
            prev_transit_ms: None,
            recommend_pps: 0.0,
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_udp_header_new() {
        let header = UdpHeader::new(42, 1000, 500000, FLAG_DATA);

        assert_eq!(header.seq, 42);
        assert_eq!(header.sec, 1000);
        assert_eq!(header.usec, 500000);
        assert_eq!(header.flags, FLAG_DATA);
    }

    #[test]
    fn test_udp_header_write_and_read() {
        let mut buffer = vec![0u8; HEADER_SIZE];
        let mut header = UdpHeader::new(12345, 67890, 123456, FLAG_DATA);

        header.write_header(&mut buffer);
        let read_header = UdpHeader::read_header(&mut buffer);

        assert_eq!(read_header.seq, 12345);
        assert_eq!(read_header.sec, 67890);
        assert_eq!(read_header.usec, 123456);
        assert_eq!(read_header.flags, FLAG_DATA);
    }

    #[test]
    fn test_udp_header_max_values() {
        let mut buffer = vec![0u8; HEADER_SIZE];
        let mut header = UdpHeader::new(u64::MAX, u64::MAX, u32::MAX, u32::MAX);

        header.write_header(&mut buffer);
        let read_header = UdpHeader::read_header(&mut buffer);

        assert_eq!(read_header.seq, u64::MAX);
        assert_eq!(read_header.sec, u64::MAX);
        assert_eq!(read_header.usec, u32::MAX);
        assert_eq!(read_header.flags, u32::MAX);
    }

    #[test]
    fn test_udp_header_zero_values() {
        let mut buffer = vec![0u8; HEADER_SIZE];
        let mut header = UdpHeader::new(0, 0, 0, 0);

        header.write_header(&mut buffer);
        let read_header = UdpHeader::read_header(&mut buffer);

        assert_eq!(read_header.seq, 0);
        assert_eq!(read_header.sec, 0);
        assert_eq!(read_header.usec, 0);
        assert_eq!(read_header.flags, 0);
    }

    #[test]
    fn test_write_header_larger_buffer() {
        let mut buffer = vec![0u8; HEADER_SIZE + 100];
        let mut header = UdpHeader::new(100, 200, 300, FLAG_DATA);

        header.write_header(&mut buffer);
        let read_header = UdpHeader::read_header(&mut buffer);

        assert_eq!(read_header.seq, 100);
        assert_eq!(read_header.sec, 200);
        assert_eq!(read_header.usec, 300);
        assert_eq!(read_header.flags, FLAG_DATA);

        // Verify the rest of buffer is unchanged
        assert_eq!(buffer[24], 0);
    }

    #[test]
    fn test_udp_data_first_packet() {
        let mut data = UdpData::new();
        let header = UdpHeader::new(0, 1000, 0, FLAG_DATA);

        data.process_packet(1024, &header, Duration::from_secs(0));

        assert_eq!(data.received, 1);
        assert_eq!(data.bytes, 1024);
        assert_eq!(data.last_seq, Some(0));
        assert_eq!(data.lost, 0);
        assert_eq!(data.out_of_order, 0);
    }

    #[test]
    fn test_udp_data_sequential_packets() {
        let mut data = UdpData::new();

        for seq in 0..10 {
            let header = UdpHeader::new(seq, 1000 + seq, 0, FLAG_DATA);
            data.process_packet(1024, &header, Duration::from_millis(seq * 10));
        }

        assert_eq!(data.received, 10);
        assert_eq!(data.bytes, 10240);
        assert_eq!(data.last_seq, Some(9));
        assert_eq!(data.lost, 0);
        assert_eq!(data.out_of_order, 0);
    }

    #[test]
    fn test_udp_data_packet_loss() {
        let mut data = UdpData::new();

        // Send packets 0, 1, 2, skip 3 and 4, then send 5
        let header0 = UdpHeader::new(0, 1000, 0, FLAG_DATA);
        data.process_packet(1024, &header0, Duration::from_millis(0));

        let header1 = UdpHeader::new(1, 1001, 0, FLAG_DATA);
        data.process_packet(1024, &header1, Duration::from_millis(10));

        let header2 = UdpHeader::new(2, 1002, 0, FLAG_DATA);
        data.process_packet(1024, &header2, Duration::from_millis(20));

        // Skip 3 and 4, jump to 5
        let header5 = UdpHeader::new(5, 1005, 0, FLAG_DATA);
        data.process_packet(1024, &header5, Duration::from_millis(50));

        assert_eq!(data.received, 4);
        assert_eq!(data.lost, 2); // Packets 3 and 4 were lost
        assert_eq!(data.last_seq, Some(5));
    }

    #[test]
    fn test_udp_data_out_of_order() {
        let mut data = UdpData::new();

        // Send 0, 1, 2, then send 1 again (out of order)
        let header0 = UdpHeader::new(0, 1000, 0, FLAG_DATA);
        data.process_packet(1024, &header0, Duration::from_millis(0));

        let header1 = UdpHeader::new(1, 1001, 0, FLAG_DATA);
        data.process_packet(1024, &header1, Duration::from_millis(10));

        let header2 = UdpHeader::new(2, 1002, 0, FLAG_DATA);
        data.process_packet(1024, &header2, Duration::from_millis(20));

        // Out of order: send 1 again
        let header1_again = UdpHeader::new(1, 1001, 0, FLAG_DATA);
        data.process_packet(1024, &header1_again, Duration::from_millis(30));

        assert_eq!(data.received, 4);
        assert_eq!(data.out_of_order, 1);
        assert_eq!(data.last_seq, Some(2)); // Should still be 2
    }

    #[test]
    fn test_udp_data_duplicate_packet() {
        let mut data = UdpData::new();

        let header1 = UdpHeader::new(1, 1000, 0, FLAG_DATA);
        data.process_packet(1024, &header1, Duration::from_millis(0));

        // Send same sequence again
        let header1_dup = UdpHeader::new(1, 1000, 0, FLAG_DATA);
        data.process_packet(1024, &header1_dup, Duration::from_millis(10));

        assert_eq!(data.received, 2);
        assert_eq!(data.last_seq, Some(1));
        assert_eq!(data.lost, 0);
        assert_eq!(data.out_of_order, 0);
    }

    #[test]
    fn test_calc_bitrate_no_packets() {
        let mut data = UdpData::new();
        data.calc_bitrate(Duration::from_secs(1));

        assert_eq!(data.recommend_pps, 0.0);
        assert_eq!(data.period_lost, 0);
        assert_eq!(data.period_recived, 0);
    }

    #[test]
    fn test_calc_bitrate_perfect_reception() {
        let mut data = UdpData::new();
        data.period_recived = 1000;
        data.period_lost = 0;

        data.calc_bitrate(Duration::from_secs(1));
        println!("rec- {}", data.period_recived);

        // With no losses  return with  with actual bitrate
        assert_eq!(data.recommend_pps, 1000.0);
        assert_eq!(data.period_lost, 0);
        assert_eq!(data.period_recived, 0);
    }

    #[test]
    fn test_calc_bitrate_high_loss() {
        let mut data = UdpData::new();
        data.period_recived = 1000;
        data.period_lost = 200; // 20% loss

        data.calc_bitrate(Duration::from_secs(1));

        // With low reception ratio (< 0.99), should reduce by 5%
        // 1000 pps * 0.95 = 950 pps
        assert_eq!(data.recommend_pps, 950.0);
        assert_eq!(data.period_lost, 0);
        assert_eq!(data.period_recived, 0);
    }

    #[test]
    fn test_calc_bitrate_moderate_loss() {
        let mut data = UdpData::new();
        data.period_recived = 1000;
        data.period_lost = 10; // 1% loss

        data.calc_bitrate(Duration::from_secs(1));

        // With int_part >= 99 but decimal < 98, should add 5 pps
        assert_eq!(data.recommend_pps, 1005.0);
    }






    #[test]
    fn test_calc_bitrate_edge_case_98_percent() {
        let mut data = UdpData::new();
        data.period_recived = 10000;
        data.period_lost = 200; // 98% reception exactly

        data.calc_bitrate(Duration::from_secs(1));

        // received_ratio = 9800/10000 = 0.98
        // int_part = 0, should reduce by 5%
        assert_eq!(data.recommend_pps, 9500.0);
    }

    #[test]
    fn test_calc_bitrate_99_point_8_percent() {
        let mut data = UdpData::new();
        data.period_recived = 1000;
        data.period_lost = 2; // 99.8% reception

        data.calc_bitrate(Duration::from_secs(1));

        // received_ratio = 998/1000 = 0.998
        // int_part = 0, decimal_part = 99, should add 5
        assert_eq!(data.recommend_pps, 1005.0);
    }

 

    #[test]
    fn test_full_packet_flow_scenario() {
        let mut data = UdpData::new();

        // Simulate a realistic packet flow
        for seq in 0..100 {
            // Skip some packets to simulate loss
            if seq == 10 || seq == 25 || seq == 50 {
                continue;
            }

            let header =
                UdpHeader::new(seq, 1000 + seq, (seq as u32 * 1000) % 1_000_000, FLAG_DATA);
            data.process_packet(1400, &header, Duration::from_millis(seq * 10));
        }

        assert_eq!(data.received, 97); // 100 - 3 lost
        assert_eq!(data.bytes, 97 * 1400);
        assert!(data.lost > 0);
    }

   

 
}
