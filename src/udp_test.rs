use crate::errors::MyError;
use crate::random_utils::RandomToSend;
use crate::udp_data::{
    FLAG_DATA, FLAG_FIN, HEADER_SIZE, IntervalResult, UdpData, UdpHeader, now_micros,
};
use crate::ui::client_period_report;
use std::net::{SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct TestResult {
    result: IntervalResult,
    time: Instant,
}

pub struct UdpServer {
    sock: UdpSocket,
    timeout: Duration,
    interval: Duration,
    udp_result: Vec<TestResult>,
}

impl UdpServer {
    pub fn new(addr: SocketAddr, interval: Duration, timeout: Duration) -> Result<Self, MyError> {
        let sock = UdpSocket::bind(addr).map_err(MyError::BindFailed)?;

        Ok(Self {
            sock: sock,
            interval,
            timeout,
            udp_result: vec![],
        })
    }

    pub fn run(&mut self) -> Result<(), MyError> {
        println!("server start");

        let mut udp_data = UdpData::new();
        // wait for the start udp packet to start the test and set the buf lenght
        let mut buf = vec![0u8; 2048];

        let _ = self
            .sock
            .recv(&mut buf)
            .map_err(|e| MyError::RecvFailed(e))?;

        let start = Instant::now();
        let mut period_report = Instant::now();

        let mut calc_instat = Instant::now();
        let calc_interval = Duration::from_millis(200);

        loop {
            let (len, _) = self
                .sock
                .recv_from(&mut buf)
                .map_err(|e| MyError::RecvFailed(e))?;

            if len < HEADER_SIZE {
                continue;
            }

            let header = UdpHeader::read_header(&mut buf);

            udp_data.process_packet(len, &header, start.elapsed());

            let time_to_calc_bitrate = calc_instat.elapsed();
            if time_to_calc_bitrate >= calc_interval {
                udp_data.calc_bitrate(time_to_calc_bitrate);
                calc_instat = Instant::now();
            }

            if header.flags == FLAG_FIN {
                break;
            }

            if period_report.elapsed() >= self.interval {
                self.udp_result.push(TestResult {
                    result: udp_data.get_interval_result(),
                    time: start,
                });

                period_report = Instant::now();
            }
        }

        println!("test finished");
        Ok(())
    }

}

#[derive(Debug)]
pub struct UdpClient {
    sock: UdpSocket,
    bitrate_bps: f64,
    payload_size: usize,
    timeout: Duration,
    interval: Duration,
}

impl UdpClient {
    pub fn new(
        addr: SocketAddr,
        bitrate_bps: f64,
        payload_size: usize,
        interval: Duration,
        timeout: Duration,
    ) -> Result<Self, MyError> {
        let sock = UdpSocket::bind(addr).map_err(MyError::BindFailed)?;

        Ok(Self {
            sock: sock,
            bitrate_bps,
            payload_size,
            interval,
            timeout,
        })
    }

    pub fn run(&mut self, dest: SocketAddr) -> Result<(), MyError> {
        self.sock.connect(&dest).map_err(MyError::ConnectFailed)?;

        let interval_per_packet = ipp(self.payload_size, self.bitrate_bps);

        let mut seq: u64 = 0;

        let mut buf = vec![0u8; self.payload_size];

        let mut random = RandomToSend::new().map_err(|e| MyError::FailToGetRandom(e))?;

        //send a packet that tell the server to start
        self.sock.send(&buf).map_err(|e| MyError::SendFailed(e))?;

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

            self.sock.send(&buf).map_err(|e| MyError::SendFailed(e))?;

            seq += 1;
            time_to_next_target(seq, interval_per_packet, start);

            if report_time.elapsed() >= self.interval {
                client_period_report(start, self.payload_size, seq as usize);
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

        self.sock.send(&buf).map_err(|e| MyError::SendFailed(e))?;
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
