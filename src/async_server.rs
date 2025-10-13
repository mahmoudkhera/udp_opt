//!  Async UDP Server implementation for measuring UDP throughput and performance.
//!
//! This module provides [`AsyncUdpServer`] — a simple and efficient UDP server
//! that can receive UDP packets, calculate bitrate periodically, and store
//! interval-based test results.

use std::time::Duration;

use tokio::{
    net::UdpSocket,
    sync::mpsc::{Receiver, error::TryRecvError},
    time::Instant,
};

use crate::{
    errors::UdpOptError,
    utils::{
        net_utils::{IntervalResult, ServerCommand},
        udp_data::{FLAG_FIN, HEADER_SIZE, UdpData, UdpHeader},
        ui::print_result,
    },
};

/// Asynchronous UDP Server for high-throughput packet receiving.
#[derive(Debug)]
pub struct AsyncUdpServer {
    ///Time between each result to save
    interval: Duration,
    /// Collecting the interval results
    udp_result: Vec<IntervalResult>,
    /// Async receiver for control commands (`Start`, `Stop`) from another thread.
    control_rx: Receiver<ServerCommand>,
}

impl AsyncUdpServer {
    /// Creates a new [`AsyncUdpServer`] that binds to the given socket address.
    ///
    /// - `interval`: The duration for each result interval.
    /// - `control_rx`: A channel receiver to control start/stop commands.
    pub async fn new(interval: Duration, control_rx: Receiver<ServerCommand>) -> Self {
        Self {
            interval,
            udp_result: Vec::with_capacity(100),
            control_rx,
        }
    }
    /// Runs the async UDP server loop.
    ///
    /// - Waits for a `Start` command on the control channel before starting.
    /// The loop terminates when:
    /// - A `Stop` command is received.
    /// - A packet with the `FLAG_FIN` flag is received.
    /// - The control channel disconnects.
    ///
    ///
    /// # Arguments
    /// - `sock`: The async bound UDP socket to receive packets from.
    ///
    /// #Return
    ///  [`Vec<IntervalResult>`] the collecting results
    ///
    /// # Errors
    ///
    /// Returns [`UdpOptError::RecvFailed`] if a UDP receive error occurs.
    /// Returns [`UdpOptError::UnexpectedCommand`] if a UDP receive error occurs.
    /// Returns [`UdpOptError::ChannelClosed`] if a UDP receive error occurs.

    pub async fn run(&mut self, sock: &mut UdpSocket) -> Result<Vec<IntervalResult>, UdpOptError> {
        println!("server start");

        let mut udp_data = UdpData::new();
        let mut buf = vec![0u8; 2048];

        // wait for the start udp packet to start the test and set the buf lenght
        match self.control_rx.recv().await {
            Some(ServerCommand::Stop) => return Err(UdpOptError::UnexpectedCommand),
            Some(ServerCommand::Start) => {}
            None => return Err(UdpOptError::ChannelClosed),
        }

        // start measuring after reciving the first packt
        let _ = sock
            .recv(&mut buf)
            .await
            .map_err(|e| UdpOptError::RecvFailed(e))?;

        let mut calc_instat = Instant::now();
        let calc_interval = Duration::from_millis(200);
        let mut start = Instant::now();

        loop {
            // Check control messages
            match self.control_rx.try_recv() {
                Ok(ServerCommand::Stop) => break,
                Ok(ServerCommand::Start) => return Err(UdpOptError::UnexpectedCommand),
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => return Err(UdpOptError::ChannelClosed),
            }
            let len = sock
                .recv(&mut buf)
                .await
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
                let res = udp_data.get_interval_result(start.elapsed());
                print_result(&res);
                self.udp_result.push(res);
                start = Instant::now();
            }
        }
        println!("test finished");
        Ok(self.udp_result.clone())
    }
}
