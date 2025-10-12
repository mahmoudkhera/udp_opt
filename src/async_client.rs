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
        net_utils::interval_per_packet,
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

        let ipp = interval_per_packet(self.payload_size, self.bitrate_bps);

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
                return Err(UdpOptError::ChannelError);
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
            time_to_next_target_async(seq, ipp, start).await;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};
    use tokio::net::UdpSocket;
    use tokio::sync::broadcast;

    // Helper function to create a test client
    async fn create_test_client(
        port: u16,
        bitrate_bps: f64,
        payload_size: usize,
        timeout: Duration,
    ) -> (AsyncUdpClient, broadcast::Sender<ClientCommand>) {
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
        let (tx, rx) = broadcast::channel(10);

        let client = AsyncUdpClient::new(addr, bitrate_bps, payload_size, timeout, rx)
            .await
            .expect("Failed to create client");

        (client, tx)
    }

    // Helper function to create a test server socket
    async fn create_test_server(port: u16) -> UdpSocket {
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
        UdpSocket::bind(addr).await.unwrap()
    }

    #[tokio::test]
    async fn test_client_creation() {
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
        let bitrate_bps = 1_000_000.0; // 1 Mbps
        let payload_size = 1024;
        let timeout = Duration::from_secs(1);
        let (_tx, rx) = broadcast::channel(10);

        let result = AsyncUdpClient::new(addr, bitrate_bps, payload_size, timeout, rx).await;
        assert!(result.is_ok());

        let client = result.unwrap();
        assert_eq!(client.bitrate_bps, 1_000_000.0);
        assert_eq!(client.payload_size, 1024);
        assert_eq!(client.timeout, Duration::from_secs(1));
    }

    #[tokio::test]
    async fn test_client_bind_failure() {
        // Try to bind to an invalid address
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(255, 255, 255, 255)), 8001);
        let bitrate_bps = 1_000_000.0;
        let payload_size = 1024;
        let timeout = Duration::from_secs(1);
        let (_tx, rx) = broadcast::channel(10);

        let result = AsyncUdpClient::new(addr, bitrate_bps, payload_size, timeout, rx).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), UdpOptError::BindFailed(_)));
    }

    #[tokio::test]
    async fn test_client_waits_for_start_command() {
        let (mut client, tx) =
            create_test_client(0, 1_000_000.0, 512, Duration::from_millis(100)).await;
        let server = create_test_server(20001).await;
        let dest = server.local_addr().unwrap();

        let client_handle = tokio::spawn(async move { client.run(dest).await });

        // Give client time to start waiting
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Verify client is still waiting (not finished)
        assert!(!client_handle.is_finished());

        // Send start command
        tx.send(ClientCommand::Start).unwrap();

        let result = client_handle.await.unwrap();
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_client_stops_on_stop_command_before_start() {
        let (mut client, tx) =
            create_test_client(0, 1_000_000.0, 512, Duration::from_secs(1)).await;
        let server = create_test_server(20002).await;
        let dest = server.local_addr().unwrap();

        // Send stop command before start
        tx.send(ClientCommand::Stop).unwrap();

        let result = client.run(dest).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            UdpOptError::UnexpectedCommand
        ));
    }

    #[tokio::test]
    async fn test_client_handles_channel_closed() {
        let (mut client, tx) =
            create_test_client(0, 1_000_000.0, 512, Duration::from_secs(1)).await;
        let server = create_test_server(20003).await;
        let dest = server.local_addr().unwrap();

        // Drop the sender to close the channel
        drop(tx);

        let result = client.run(dest).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), UdpOptError::ChannelError));
    }

    #[tokio::test]
    async fn test_client_sends_packets() {
        let (mut client, tx) =
            create_test_client(0, 1_000_000.0, 512, Duration::from_millis(100)).await;
        let server = create_test_server(20004).await;
        let dest = server.local_addr().unwrap();

        let server_handle = tokio::spawn(async move {
            let mut buf = vec![0u8; 2048];
            let mut packet_count = 0;
            let mut received_fin = false;

            loop {
                match tokio::time::timeout(Duration::from_secs(2), server.recv(&mut buf)).await {
                    Ok(Ok(len)) => {
                        if len >= 13 {
                            let flags = u32::from_be_bytes(buf[20..24].try_into().unwrap());
                            if flags == FLAG_FIN {
                                received_fin = true;
                                break;
                            } else if flags == FLAG_DATA {
                                packet_count += 1;
                            }
                        }
                    }
                    _ => break,
                }
            }
            (packet_count, received_fin)
        });

        tokio::time::sleep(Duration::from_millis(50)).await;
        tx.send(ClientCommand::Start).unwrap();

        let result = client.run(dest).await;
        assert!(result.is_ok());

        let (packet_count, received_fin) = server_handle.await.unwrap();
        assert!(packet_count > 0, "Should have received at least one packet");
        assert!(received_fin, "Should have received FIN packet");
    }

    #[tokio::test]
    async fn test_client_respects_timeout() {
        let timeout = Duration::from_millis(200);
        let (mut client, tx) = create_test_client(0, 1_000_000.0, 512, timeout).await;
        let server = create_test_server(20005).await;
        let dest = server.local_addr().unwrap();

        tx.send(ClientCommand::Start).unwrap();

        let start = Instant::now();
        let result = client.run(dest).await;
        let elapsed = start.elapsed();

        assert!(result.is_ok());
        // Should complete around the timeout duration (with some tolerance)
        assert!(
            elapsed >= timeout && elapsed < timeout + Duration::from_millis(150),
            "Expected ~{:?}, got {:?}",
            timeout,
            elapsed
        );
    }

    #[tokio::test]
    async fn test_client_sequence_numbers_monotonic() {
        let (mut client, tx) =
            create_test_client(0, 10_000_000.0, 512, Duration::from_millis(100)).await;
        let server = create_test_server(20007).await;
        let dest = server.local_addr().unwrap();

        let server_handle = tokio::spawn(async move {
            let mut buf = vec![0u8; 2048];
            let mut sequences = Vec::new();

            loop {
                match tokio::time::timeout(Duration::from_secs(1), server.recv(&mut buf)).await {
                    Ok(Ok(len)) => {
                        if len >= 13 {
                            // Read sequence number (first 8 bytes as u64 little-endian)
                            let seq = u64::from_be_bytes(buf[0..8].try_into().unwrap());
                            let flags = u32::from_be_bytes(buf[20..24].try_into().unwrap());
                            sequences.push(seq);

                            if flags == FLAG_FIN {
                                break;
                            }
                        }
                    }
                    _ => break,
                }
            }
            sequences
        });

        tx.send(ClientCommand::Start).unwrap();
        client.run(dest).await.unwrap();

        let sequences = server_handle.await.unwrap();
        assert!(sequences.len() > 1, "Should receive multiple packets");

        // Check that sequences start at 0
        assert_eq!(sequences[0], 0, "First sequence should be 0");

        // Check that sequences are monotonically increasing by 1
        for i in 1..sequences.len() {
            assert_eq!(
                sequences[i],
                sequences[i - 1] + 1,
                "Sequence numbers should increment by 1"
            );
        }
    }

   

    #[tokio::test]
    async fn test_client_with_zero_timeout() {
        let (mut client, tx) =
            create_test_client(0, 1_000_000.0, 512, Duration::from_millis(0)).await;
        let server = create_test_server(20013).await;
        let dest = server.local_addr().unwrap();

        let server_handle = tokio::spawn(async move {
            let mut buf = vec![0u8; 2048];
            let mut received_fin = false;

            match tokio::time::timeout(Duration::from_secs(1), server.recv(&mut buf)).await {
                Ok(Ok(len)) => {
                    if len >= 13 && FLAG_FIN == u32::from_be_bytes(buf[20..24].try_into().unwrap())
                    {
                        received_fin = true;
                    }
                }
                _ => {}
            }
            received_fin
        });

        tx.send(ClientCommand::Start).unwrap();
        let result = client.run(dest).await;

        assert!(result.is_ok());

        // Should immediately send FIN packet
        let received_fin = server_handle.await.unwrap();
        assert!(
            received_fin,
            "Should receive FIN packet immediately with zero timeout"
        );
    }

    #[tokio::test]
    async fn test_time_to_next_target_async_already_past() {
        let start = Instant::now();
        let ipp = Duration::from_micros(1); // Very short interval

        // Wait longer than the interval
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Should return immediately since target time is in the past
        let before = Instant::now();
        time_to_next_target_async(1, ipp, start).await;
        let elapsed = before.elapsed();

        // Should complete very quickly (under 5ms)
        assert!(
            elapsed < Duration::from_millis(5),
            "Should return immediately for past target, took {:?}",
            elapsed
        );
    }
}
