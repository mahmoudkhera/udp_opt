//! Asynchronous UDP Client for sending high-performance test packets to a UDP server.
//!
//! This module provides [`AsyncUdpClient`] — an async client that can send UDP packets
//! at a specified bitrate using `tokio`, with precise timing, start/stop control,
//! and FIN signaling at the end of transmission.

use std::time::{Duration, Instant};

use tokio::{net::UdpSocket, sync::mpsc::Receiver};

use crate::{
    errors::UdpOptError,
    utils::{
        net_utils::{ClientCommand, interval_per_packet},
        random_utils::AsyncRandomToSend,
        udp_data::{FLAG_DATA, FLAG_FIN, UdpHeader, now_micros},
    },
};

/// Asynchronous UDP client for high-throughput packet sending.
#[derive(Debug)]
pub struct AsyncUdpClient {
    /// Target sending bitrate in bits per second.
    bitrate_bps: f64,
    /// Size of each UDP packet payload, including header.
    payload_size: usize,
    /// Maximum duration for the transmission test.
    timeout: Duration,
    /// Async receiver for control commands (`Start`, `Stop`) from another thread.
    control_rx: Receiver<ClientCommand>,
}

impl AsyncUdpClient {
    /// Creates a new UDP client.
    ///
    /// # Parameters
    /// - `bitrate_bps`: Desired sending bitrate in bits per second.
    /// - `payload_size`: Number of bytes in each packet (typically 512–1500 bytes).
    /// - `timeout`: Total duration to keep sending packets.
    /// - `control_rx`: Async channel to receive [`ClientCommand`] control signals.
    ///
    /// # Returns
    /// A new [`AsyncUdpClient`] instance ready to send packets using [`AsyncUdpClient::run`].
    pub async fn new(
        bitrate_bps: f64,
        payload_size: usize,
        timeout: Duration,
        control_rx: Receiver<ClientCommand>,
    ) -> Self {
        Self {
            bitrate_bps,
            payload_size,
            timeout,
            control_rx,
        }
    }

    /// Runs the UDP async client, sending packets to the specified destination.
    ///
    /// - Waits for a `Start` command from the control channel before sending.
    /// - Sends packets according to the configured bitrate and payload size.
    /// - Stops after `timeout` duration or when the control channel sends `Stop`.
    /// - Sends a FIN packet at the end to notify the server.
    ///
    /// # Parameters
    /// - `sock`: A bound async [`UdpSocket`] that will be used to send packets.
    ///
    /// Returns:
    /// - [`UdpOptError::SendFailed`] if sending fails.
    /// - [`UdpOptError::FailToGetRandom`] if payload randomization fails.
    /// - [`UdpOptError::ChannelClosed`] if control channel disconnects before start.
    /// - [`UdpOptError::UnexpectedCommand`] if an unexpected command is received.

    pub async fn run(&mut self, sock: &mut UdpSocket) -> Result<(), UdpOptError> {
        let ipp = interval_per_packet(self.payload_size, self.bitrate_bps);

        let mut seq = 0;
        let mut buf = vec![0u8; self.payload_size];
        let mut random = AsyncRandomToSend::new()
            .await
            .map_err(|e| UdpOptError::FailToGetRandom(e))?;

        // wait for the start udp packet to start the test and set the buf lenght
        match self.control_rx.recv().await {
            Some(ClientCommand::Stop) => return Err(UdpOptError::UnexpectedCommand),
            Some(ClientCommand::Start) => {}
            None => return Err(UdpOptError::ChannelClosed),
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

            sock.send(&buf)
                .await
                .map_err(|e| UdpOptError::SendFailed(e))?;

            seq += 1;
            time_to_next_target_async(seq, ipp, start).await;
        }

        let (sec, usec) = now_micros();
        let mut fin = UdpHeader::new(seq, sec, usec, FLAG_FIN);
        fin.write_header(&mut buf);

        sock.send(&buf)
            .await
            .map_err(|e| UdpOptError::SendFailed(e))?;
        println!("Client done. Sent {} packets (+FIN)", seq);

        Ok(())
    }
}

//helper function

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
