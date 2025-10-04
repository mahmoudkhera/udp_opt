//! UDP Server implementation for measuring UDP throughput and performance.
//!
//! This module provides [`UdpServer`] — a simple and efficient UDP server
//! that can receive UDP packets, calculate bitrate periodically, and store
//! interval-based test results.

use crate::errors::UdpOptError;
use crate::udp_data::{FLAG_FIN, HEADER_SIZE, IntervalResult, UdpData, UdpHeader};
use std::net::{SocketAddr, UdpSocket};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct TestResult {
    /// The result metrics collected during this interval (bitrate, packets, etc.).
    pub result: IntervalResult,
    /// The time elapsed since the start of the interval.
    pub time: Duration,
}

#[derive(Debug, Clone)]
pub enum ServerCommand {
    Start,
    Stop,
}

#[derive(Debug)]
pub struct UdpServer {
    sock: UdpSocket,
    interval: Duration,
    udp_result: Vec<TestResult>,
    control_rx: Receiver<ServerCommand>,
}

impl UdpServer {
    /// Creates a new [`UdpServer`] that binds to the given socket address.
    ///
    /// - `addr`: The IP and port to bind to.
    /// - `interval`: The duration for each result interval.
    /// - `control_rx`: A channel receiver to control start/stop commands.
    ///
    /// # Errors
    ///
    /// Returns [`UdpOptError::BindFailed`] if the socket could not be bound.
    pub fn new(
        addr: SocketAddr,
        interval: Duration,
        control_rx: Receiver<ServerCommand>,
    ) -> Result<Self, UdpOptError> {
        let sock = UdpSocket::bind(addr).map_err(UdpOptError::BindFailed)?;

        Ok(Self {
            sock: sock,
            interval,
            udp_result: Vec::with_capacity(100),
            control_rx,
        })
    }
        /// Runs the UDP server loop.
    ///
    /// - Waits for a `Start` command on the control channel before starting.
    /// The loop terminates when:
    /// - A `Stop` command is received.
    /// - A packet with the `FLAG_FIN` flag is received.
    /// - The control channel disconnects.
    ///
    /// # Errors
    ///
    /// Returns [`UdpOptError::RecvFailed`] if a UDP receive error occurs.
    pub fn run(&mut self) -> Result<(), UdpOptError> {
        println!("server start");

        let mut udp_data = UdpData::new();
        // wait for the start udp packet to start the test and set the buf lenght
        let mut buf = vec![0u8; 2048];

        match self.control_rx.recv().unwrap() {
            ServerCommand::Stop => {
                println!("unecpect stop")
            }
            ServerCommand::Start => {}
        }

        let mut calc_instat = Instant::now();
        let calc_interval = Duration::from_millis(200);
        let mut start = Instant::now();

        loop {
            // Check control messages
            match self.control_rx.try_recv() {
                Ok(ServerCommand::Stop) => {
                    println!("Received stop command");
                    break;
                }
                Ok(ServerCommand::Start) => {}
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => {
                    println!("Control channel closed");
                    break;
                }
            }

            let len = self
                .sock
                .recv(&mut buf)
                .map_err(|e| UdpOptError::RecvFailed(e))?;

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

            if start.elapsed() >= self.interval {
                self.udp_result.push(TestResult {
                    result: udp_data.get_interval_result(),
                    time: start.elapsed(),
                });

                start = Instant::now();
            }
        }

        println!("test finished");
        Ok(())
    }

    pub fn get_result(&mut self) -> &[TestResult] {
        &self.udp_result
    }
}
