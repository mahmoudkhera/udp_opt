use crate::random_utils::fill_random;
use crate::udp_data::{FLAG_DATA, FLAG_FIN, HEADER_SIZE, UdpData, UdpHeader, now_micros};
use crate::ui::{final_report, take_period_report};
use anyhow::Result;
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

    pub fn server(&mut self) -> Result<()> {
        println!("server start");

        let sock = UdpSocket::bind(self.addr)?;
        let mut buf = vec![0u8; 64 * 1024];

        // wait for the start udp packet to start the test
        let (_, _) = sock.recv_from(&mut buf)?;

        let start = Instant::now();
        let mut period_report = Instant::now();

        loop {
            let (len, _) = sock.recv_from(&mut buf)?;

            if len < HEADER_SIZE {
                continue;
            }

            let header = UdpHeader::read_header(&mut buf);

            self.udp_data.process_packet(len, &header, start.elapsed());

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

    pub fn client(&mut self, dest: SocketAddr) -> Result<()> {
        //thest for test in the same machine
        let sock = UdpSocket::bind("0.0.0.0:0")?;

        sock.connect(&dest)?;

        let bits_per_packet = (self.payload_size * 8) as f64;
        let packet_per_second = (self.bitrate_bps / bits_per_packet).max(1.0);

        let interval_per_packet = Duration::from_secs_f64(1.0 / packet_per_second);

        let mut seq: u64 = 0;

        let mut buf = vec![0u8; self.payload_size];

        //send a packet that tell the server to start
        sock.send(&buf)?;

        let start = Instant::now();
        let mut report_time = Instant::now();

        loop {
            if start.elapsed() >= self.timeout {
                break;
            }

            fill_random(&mut buf, self.payload_size)?;
            //  not you can use any random  base insted of using the unix_epoch
            let (sec, usec) = now_micros();
            let mut header = UdpHeader::new(seq, sec, usec, FLAG_DATA);
            header.write_header(&mut buf);

            sock.send(&buf)?;
            seq += 1;

            // this section of code determine when the next packet must be sent depnds
            let next_target =
                start + Duration::from_secs_f64(seq as f64 * interval_per_packet.as_secs_f64());
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

            if report_time.elapsed() >= self.interval {
                let elapsed = start.elapsed().as_secs_f64();
                let sent_bytes = (seq as usize * self.payload_size) as f64;
                let mbps = (sent_bytes * 8.0) / elapsed / 1_000_000.0;
                println!(
                    "Elapsed {:.2}s | Sent {} pkts | est rate {:.3} Mbps",
                    elapsed, seq, mbps
                );
                report_time = Instant::now();
            }
        }

        // FIN
        let (sec, usec) = now_micros();
        let mut fin = UdpHeader::new(seq, sec, usec, FLAG_FIN);
        fill_random(&mut buf, self.payload_size)?;
        fin.write_header(&mut buf);

        sock.send(&buf)?;
        println!("Client done. Sent {} packets (+FIN)", seq);

        Ok(())
    }
}
