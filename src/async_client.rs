//! Asynchronous UDP Client for sending high-performance test packets to a UDP server.
//!
//! This module provides [`AsyncUdpClient`] — an async client that can send UDP packets
//! at a specified bitrate using `tokio`, with precise timing, start/stop control,
//! and FIN signaling at the end of transmission.

use std::{
    net::SocketAddr,
    time::{Duration, Instant},
};

use tokio::{net::UdpSocket, sync::broadcast::Receiver};

use crate::{
    errors::UdpOptError,
    utils::{
        random_utils::AsyncRandomToSend,
        udp_data::{FLAG_DATA, FLAG_FIN, UdpHeader, now_micros},
    },
};
#[derive(Debug, Clone)]
pub enum ClientCommand {
    Start,
    Stop,
}

/// Asynchronous UDP client for high-throughput packet sending.
#[derive(Debug)]
pub struct AsyncUdpClient {
    sock: UdpSocket,
    bitrate_bps: f64,
    payload_size: usize,
    timeout: Duration,
    control_rx: Receiver<ClientCommand>,
}

impl AsyncUdpClient {
    /// Creates a new async UDP client bound to the given local address.
    pub async fn new(
        addr: SocketAddr,
        bitrate_bps: f64,
        payload_size: usize,
        timeout: Duration,
        control_rx: Receiver<ClientCommand>,
    ) -> Result<Self, UdpOptError> {
        let sock = UdpSocket::bind(addr)
            .await
            .map_err(UdpOptError::BindFailed)?;
        Ok(Self {
            sock,
            bitrate_bps,
            payload_size,
            timeout,
            control_rx,
        })
    }

    /// Runs the async UDP client.
    ///
    /// - Waits for a `Start` command before sending.
    /// - Sends packets at the configured bitrate.
    /// - Stops after timeout or on receiving a `Stop` command.

    pub async fn run(&mut self, dest: SocketAddr) -> Result<(), UdpOptError> {
        self.sock
            .connect(dest)
            .await
            .map_err(UdpOptError::ConnectFailed)?;

        let interval_per_packet = ipp(self.payload_size, self.bitrate_bps);

        let mut seq = 0;
        let mut buf = vec![0u8; self.payload_size];
        let mut random = AsyncRandomToSend::new()
            .await
            .map_err(|e| UdpOptError::FailToGetRandom(e))?;

        // Wait for Start or Stop before beginning
        match self.control_rx.recv().await {
            Ok(ClientCommand::Start) => {}
            Ok(ClientCommand::Stop) => {
                return Err(UdpOptError::UnexpectedCommand);
            }
            Err(_) => {
                return Err(UdpOptError::ChannelClosed);
            }
        }

        let start = Instant::now();

        loop {
            if start.elapsed() >= self.timeout {
                break;
            }

            random
                .fill(&mut buf)
                .await
                .map_err(|e| UdpOptError::FailToGetRandom(e))?;

            let (sec, usec) = now_micros();
            let mut header = UdpHeader::new(seq, sec, usec, FLAG_DATA);
            header.write_header(&mut buf);

            self.sock
                .send(&buf)
                .await
                .map_err(|e| UdpOptError::SendFailed(e))?;

            seq += 1;
            time_to_next_target_async(seq, interval_per_packet, start).await;
        }

        let (sec, usec) = now_micros();
        let mut fin = UdpHeader::new(seq, sec, usec, FLAG_FIN);
        fin.write_header(&mut buf);

        self.sock
            .send(&buf)
            .await
            .map_err(|e| UdpOptError::SendFailed(e))?;
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
/// Asynchronous version of the precise send timing function.
async fn time_to_next_target_async(seq: u64, ipp: Duration, start: Instant) {
    let next_target = start + Duration::from_secs_f64(seq as f64 * ipp.as_secs_f64());
    loop {
        let now = Instant::now();
        if now >= next_target {
            break;
        }

        let remaining = next_target - now;
        if remaining > Duration::from_micros(200) {
            tokio::time::sleep(remaining - Duration::from_micros(100)).await;
        } else {
            tokio::task::yield_now().await;
        }
    }
}
