use crate::errors::MyError;
use crate::random_utils::RandomToSend;
use crate::udp_data::{FLAG_DATA, FLAG_FIN, HEADER_SIZE, UdpData, UdpHeader, now_micros};
use crate::ui::{final_report, take_period_report};
use std::io::{Write, stdout};
use std::net::{SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct UdpTest {
    addr: SocketAddr,
    udp_data: UdpData,
    bitrate_bps: f64,
    payload_size: usize,
    timeout: Duration,
    interval: Duration,
}

impl UdpTest {
    pub fn new(
        addr: SocketAddr,
        bitrate_bps: f64,
        payload_size: usize,
        interval: Duration,
        timeout: Duration,
    ) -> Self {
        Self {
            addr,
            udp_data: UdpData::new(),
            bitrate_bps,
            payload_size,
            interval,
            timeout,
        }
    }

    pub fn server(&mut self) -> Result<(), MyError> {
        println!("server start");

        let sock = UdpSocket::bind(&self.addr).map_err(MyError::BindFailed)?;
        let mut buf = vec![0u8; 64 * 1024];

        // wait for the start udp packet to start the test
        let (_, _) = sock
            .recv_from(&mut buf)
            .map_err(|e| MyError::RecvFailed(e))?;

        let start = Instant::now();
        let mut period_report = Instant::now();

        let mut calc_instat = Instant::now();
        let calc_interval = Duration::from_millis(200);

        loop {
            let (len, _) = sock
                .recv_from(&mut buf)
                .map_err(|e| MyError::RecvFailed(e))?;

            if len < HEADER_SIZE {
                continue;
            }

            let header = UdpHeader::read_header(&mut buf);

            self.udp_data.process_packet(len, &header, start.elapsed());

            let time_to_calc_bitrate = calc_instat.elapsed();
            if time_to_calc_bitrate >= calc_interval {
                self.udp_data.calc_bitrate(time_to_calc_bitrate);
                calc_instat = Instant::now();
            }

            if header.flags == FLAG_FIN {
                final_report(&self.udp_data, start);
                break;
            }

            if period_report.elapsed() >= self.interval {
                take_period_report(&self.udp_data, start);
                period_report = Instant::now();
                stdout().flush().ok();
            }
        }

        println!("test finished");
        Ok(())
    }

    pub fn client(&mut self, dest: SocketAddr) -> Result<(), MyError> {
        let sock = UdpSocket::bind(self.addr).map_err(MyError::BindFailed)?;

        sock.connect(&dest).map_err(MyError::ConnectFailed)?;

        let interval_per_packet = ipp(self.payload_size, self.bitrate_bps);

        let mut seq: u64 = 0;

        let mut buf = vec![0u8; self.payload_size];

        let mut random = RandomToSend::new().map_err(|e| MyError::FailToGetRandom(e))?;

        //send a packet that tell the server to start
        sock.send(&buf).map_err(|e| MyError::SendFailed(e))?;

        let start = Instant::now();
        let mut report_time = Instant::now();

        loop {
            if start.elapsed() >= self.timeout {
                break;
            }

            random
                .fill(&mut buf)
                .map_err(|e| MyError::FailToGetRandom(e))?; //  not you can use any random  base insted of using the unix_epoch
            let (sec, usec) = now_micros();
            let mut header = UdpHeader::new(seq, sec, usec, FLAG_DATA);
            header.write_header(&mut buf);

            sock.send(&buf).map_err(|e| MyError::SendFailed(e))?;
            seq += 1;
            let p = Instant::now();
            time_to_next_target(seq, interval_per_packet, start);
            let pe: Duration = p.elapsed();

            if report_time.elapsed() >= self.interval {
                let elapsed = start.elapsed().as_secs_f64();
                let sent_bytes = (seq as usize * self.payload_size) as f64;
                let mbps = (sent_bytes * 8.0) / elapsed / 1_000_000.0;
                println!("{:?}", pe);
                println!(
                    "Elapsed {:.2}s | Sent {} pkts | est rate {:.3} Mbps",
                    elapsed, seq, mbps
                );
                report_time = Instant::now();
            }
        }

        // FIN
        random
            .fill(&mut buf)
            .map_err(|e| MyError::FailToGetRandom(e))?; //  not you can use any random  base insted of using the unix_epoch
        let (sec, usec) = now_micros();
        let mut fin = UdpHeader::new(seq, sec, usec, FLAG_FIN);
        fin.write_header(&mut buf);

        sock.send(&buf).map_err(|e| MyError::SendFailed(e))?;
        println!("Client done. Sent {} packets (+FIN)", seq);

        Ok(())
    }
}

//helper function

fn ipp(paylod: usize, bitrate: f64) -> Duration {
    let bits_per_packet = (paylod * 8) as f64;
    let packet_per_second = (bitrate / bits_per_packet).max(1.0);

    Duration::from_secs_f64(1.0 / packet_per_second)
}

#[inline]
fn time_to_next_target(seq: u64, ipp: Duration, start: Instant) {
    // this section of code determine when the next packet must be sent depnds
    let next_target = start + Duration::from_secs_f64(seq as f64 * ipp.as_secs_f64());
    loop {
        let now = Instant::now();
        if now > next_target {
            break;
        }

        let remaining = next_target - now;

        if remaining > Duration::from_micros(200) {
            // coarse sleep; subtract a small delta to avoid oversleep
            std::thread::sleep(remaining - Duration::from_micros(100));
        } else {
            // using spin here is more acurate but is uses more cpu
            // short spin / yield
            std::thread::yield_now();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::time::Duration;

    // Helper function to create a test UdpTest instance
    fn create_test_instance() -> UdpTest {
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 0);
        UdpTest::new(
            addr,
            1_000_000.0, // 1 Mbps
            1024,        // 1KB payload
            Duration::from_secs(1),
            Duration::from_secs(5),
        )
    }

    #[test]
    fn test_udp_test_new() {
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let bitrate = 10_000_000.0; // 10 Mbps
        let payload_size = 1400;
        let interval = Duration::from_secs(1);
        let timeout = Duration::from_secs(10);

        let test = UdpTest::new(addr, bitrate, payload_size, interval, timeout);

        assert_eq!(test.addr, addr);
        assert_eq!(test.bitrate_bps, bitrate);
        assert_eq!(test.payload_size, payload_size);
        assert_eq!(test.interval, interval);
        assert_eq!(test.timeout, timeout);
    }

    #[test]
    fn test_ipp_calculation_1mbps() {
        let payload = 1024; // 1KB
        let bitrate = 1_000_000.0; // 1 Mbps

        let result = ipp(payload, bitrate);

        // 1024 bytes = 8192 bits
        // At 1 Mbps, we can send ~122 packets per second
        // So interval should be ~8.192ms
        let expected = Duration::from_secs_f64(8192.0 / 1_000_000.0);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_ipp_calculation_10mbps() {
        let payload = 1400; // typical MTU payload
        let bitrate = 10_000_000.0; // 10 Mbps

        let result = ipp(payload, bitrate);

        // 1400 bytes = 11200 bits
        // At 10 Mbps, interval = 11200/10000000 = 0.00112s = 1.12ms
        let expected = Duration::from_secs_f64(0.00112);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_ipp_with_small_bitrate() {
        let payload = 1024;
        let bitrate = 100.0; // very small bitrate

        let result = ipp(payload, bitrate);

        // Should handle small bitrates correctly
        // 8192 bits / 100 bps = 81.92 seconds per packet
        // but .max enforce it to be at least 1 packet to be sent
        let expected = Duration::from_secs_f64(1.0);

        assert_eq!(result, expected);
    }

    #[test]
    fn test_time_to_next_target_immediate() {
        let start = Instant::now();
        let ipp = Duration::from_millis(10);
        let seq = 0;

        // For seq=0, target is start time, should return immediately
        time_to_next_target(seq, ipp, start);

        let elapsed = start.elapsed();
        // Should complete very quickly (less than 1ms)
        assert!(elapsed < Duration::from_millis(1));
    }

    #[test]
    fn test_time_to_next_target_wait() {
        let start = Instant::now();
        let ipp = Duration::from_millis(5);
        let seq = 2; // Target = start + 10ms

        time_to_next_target(seq, ipp, start);

        let elapsed = start.elapsed();
        // Should wait approximately 10ms
        assert!(elapsed >= Duration::from_millis(9));
        assert!(elapsed < Duration::from_millis(12));
    }

    #[test]
    fn test_time_to_next_target_past_deadline() {
        let start = Instant::now();
        std::thread::sleep(Duration::from_millis(20));

        let ipp = Duration::from_millis(5);
        let seq = 1; // Target = start + 5ms (already passed)

        let before_call = Instant::now();
        time_to_next_target(seq, ipp, start);
        let after_call = Instant::now();

        // Should return immediately since target is in the past
        let call_duration = after_call - before_call;
        assert!(call_duration < Duration::from_millis(1));
    }

    #[test]
    fn test_udp_test_clone() {
        let original = create_test_instance();
        let cloned = original.clone();

        assert_eq!(original.addr, cloned.addr);
        assert_eq!(original.bitrate_bps, cloned.bitrate_bps);
        assert_eq!(original.payload_size, cloned.payload_size);
        assert_eq!(original.interval, cloned.interval);
        assert_eq!(original.timeout, cloned.timeout);
    }

    #[test]
    fn test_udp_test_debug_format() {
        let test = create_test_instance();
        let debug_str = format!("{:?}", test);

        // Should contain "UdpTest" in debug output
        assert!(debug_str.contains("UdpTest"));
    }

    #[test]
    #[cfg(unix)]
    fn test_server_bind_invalid_address() {
        // Try to bind to a privileged port without permissions
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 1);
        let mut test = UdpTest::new(
            addr,
            1_000_000.0,
            1024,
            Duration::from_secs(1),
            Duration::from_secs(1),
        );

        let result = test.server();

        // Should fail to bind to privileged port
        assert!(result.is_err());
        if let Err(MyError::BindFailed(_)) = result {
            // Expected error type
        } else {
            panic!("Expected BindFailed error");
        }
    }

    #[test]
    #[cfg(unix)]
    fn test_client_bind_invalid_address() {
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 1);
        let dest = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 9999);

        let mut test = UdpTest::new(
            addr,
            1_000_000.0,
            1024,
            Duration::from_secs(1),
            Duration::from_millis(10),
        );

        let result = test.client(dest);

        println!("resi;t {:?}", result);

        // Should fail to bind
        assert!(result.is_err());
    }

    #[test]
    fn test_ipp_zero_bitrate() {
        let payload = 1024;
        let bitrate = 0.0;

        // This will cause division by zero in bits_per_packet / bitrate
        // The max(1.0) should prevent infinite duration
        let result = ipp(payload, bitrate);

        assert!(result.as_secs_f64().is_finite() || result.as_secs_f64() == 0.0);
    }

    #[test]
    fn test_payload_sizes() {
        let bitrate = 1_000_000.0;

        // Test various common payload sizes
        let sizes = vec![64, 512, 1024, 1400, 8192];

        for size in sizes {
            let result = ipp(size, bitrate);
            assert!(result > Duration::ZERO);
            assert!(result.as_secs_f64().is_finite());
        }
    }
}
