//! UDP Client for sending high-performance test packets to a UDP server.
//!
//! This module provides [`UdpClient`] — a client that can send UDP packets
//! at a specified bitrate with precise timing, including support for start/stop
//! commands via an `mpsc` channel.

use std::{
    net::UdpSocket,
    sync::mpsc::Receiver,
    time::{Duration, Instant},
};

use crate::{
    errors::UdpOptError,
    utils::{
        net_utils::{ClientCommand, interval_per_packet},
        random_utils::RandomToSend,
        udp_data::{FLAG_DATA, FLAG_FIN, UdpHeader, now_micros},
    },
};

#[derive(Debug)]
pub struct UdpClient {
    /// Target sending bitrate in bits per second.
    bitrate_bps: f64,

    /// Size of each UDP packet payload, including header.
    payload_size: usize,

    /// Maximum duration for the transmission test.
    timeout: Duration,

    /// Receiver for control commands (`Start`, `Stop`) from another thread.
    control_rx: Receiver<ClientCommand>,
}

impl UdpClient {
    /// Creates a new UDP client.
    ///
    /// # Parameters
    /// - `bitrate_bps`: Desired sending bitrate in bits per second.
    /// - `payload_size`: Number of bytes in each packet (typically 512–1500 bytes).
    /// - `timeout`: Total duration to keep sending packets.
    /// - `control_rx`: Channel to receive [`ClientCommand`] control signals.
    ///
    /// # Returns
    /// A new [`UdpClient`] instance ready to send packets using [`UdpClient::run`].
    pub fn new(
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

    /// Runs the UDP client, sending packets to the specified destination.
    ///
    /// - Waits for a `Start` command from the control channel before sending.
    /// - Sends packets according to the configured bitrate and payload size.
    /// - Stops after `timeout` duration or when the control channel sends `Stop`.
    /// - Sends a FIN packet at the end to notify the server.
    ///
    /// # Parameters
    /// - `sock`: A bound [`UdpSocket`] that will be used to send packets.
    ///
    /// Returns:
    /// - [`UdpOptError::SendFailed`] if sending fails.
    /// - [`UdpOptError::FailToGetRandom`] if payload randomization fails.
    /// - [`UdpOptError::ChannelClosed`] if control channel disconnects before start.
    /// - [`UdpOptError::UnexpectedCommand`] if an unexpected command is received.

    pub fn run(&mut self, sock: &mut UdpSocket) -> Result<(), UdpOptError> {
        let ipp = interval_per_packet(self.payload_size, self.bitrate_bps);

        let mut seq: u64 = 0;

        let mut buf = vec![0u8; self.payload_size];

        let mut random = RandomToSend::new().map_err(|e| UdpOptError::FailToGetRandom(e))?;

        // wait for the start udp packet to start the test and set the buf lenght
        match self.control_rx.recv() {
            Ok(ClientCommand::Stop) => return Err(UdpOptError::UnexpectedCommand),
            Ok(ClientCommand::Start) => {}
            Err(_) => return Err(UdpOptError::ChannelClosed),
        }

        let start = Instant::now();

        loop {
            if start.elapsed() >= self.timeout {
                break;
            }

            random
                .fill(&mut buf)
                .map_err(|e| UdpOptError::FailToGetRandom(e))?; //  note you can use any random  base insted of using the unix_epoch

            let (sec, usec) = now_micros();

            let mut header = UdpHeader::new(seq, sec, usec, FLAG_DATA);
            header.write_header(&mut buf);

            sock.send(&buf).map_err(|e| UdpOptError::SendFailed(e))?;

            seq += 1;
            time_to_next_target(seq, ipp, start);
        }

        // Send a final packet (FIN flag) to notify completion.
        let (sec, usec) = now_micros();
        let mut fin = UdpHeader::new(seq, sec, usec, FLAG_FIN);
        fin.write_header(&mut buf);

        sock.send(&buf).map_err(|e| UdpOptError::SendFailed(e))?;
        println!("Client done. Sent {} packets (+FIN)", seq);

        Ok(())
    }
}

//helper function

#[inline]
fn time_to_next_target(seq: u64, ipp: Duration, start: Instant) {
    // this section of code determine when the next packet must be sent depnds
    let next_target = start + Duration::from_secs_f64(seq as f64 * ipp.as_secs_f64());
    loop {
        let now = Instant::now();
        if now >= next_target {
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
mod udp_client_tests {
    use crate::utils::udp_data::HEADER_SIZE;

    use super::*;
    use std::net::UdpSocket;
    use std::sync::mpsc::{Sender, channel};
    use std::thread;
    use std::time::{Duration, Instant};

    /// Creates a test UDP client with control channel
    fn create_test_client(
        bitrate_bps: f64,
        payload_size: usize,
        timeout: Duration,
    ) -> (UdpClient, Sender<ClientCommand>) {
        let (tx, rx) = channel();
        let client = UdpClient::new(bitrate_bps, payload_size, timeout, rx);
        (client, tx)
    }

    /// Creates a pair of connected UDP sockets for testing
    fn create_socket_pair() -> (UdpSocket, UdpSocket) {
        let server_sock = UdpSocket::bind("127.0.0.1:0").expect("Failed to bind server socket");
        let client_sock = UdpSocket::bind("127.0.0.1:0").expect("Failed to bind client socket");

        let server_addr = server_sock.local_addr().unwrap();
        let client_addr = client_sock.local_addr().unwrap();

        server_sock.connect(client_addr).unwrap();
        client_sock.connect(server_addr).unwrap();

        (server_sock, client_sock)
    }

    /// Parses UDP header to extract sequence number and flags
    /// Adjust based on your actual UdpHeader structure
    fn parse_header(buf: &[u8]) -> Option<(u64, u32)> {
        if buf.len() < HEADER_SIZE {
            return None;
        }

        let seq = u64::from_be_bytes(buf[0..8].try_into().unwrap());

        let flags = u32::from_be_bytes(buf[20..24].try_into().unwrap());

        Some((seq, flags))
    }

    /// Receives packets until FIN or timeout
    fn receive_all_packets(sock: &mut UdpSocket, timeout: Duration) -> Vec<(u64, u32, usize)> {
        sock.set_read_timeout(Some(timeout)).unwrap();
        let mut packets = Vec::new();
        let mut buf = vec![0u8; 65536];

        loop {
            match sock.recv(&mut buf) {
                Ok(len) => {
                    if let Some((seq, flags)) = parse_header(&buf) {
                        packets.push((seq, flags, len));
                        if flags == FLAG_FIN {
                            break;
                        }
                    }
                }
                Err(_) => break,
            }
        }

        packets
    }

    #[test]
    fn test_client_waits_for_start_command() {
        let (mut client, tx) = create_test_client(1_000_000.0, 1024, Duration::from_millis(100));
        let (_server_sock, mut client_sock) = create_socket_pair();

        client_sock
            .set_write_timeout(Some(Duration::from_millis(100)))
            .unwrap();

        let handle = thread::spawn(move || client.run(&mut client_sock));

        // Wait a bit to ensure client is waiting for command
        thread::sleep(Duration::from_millis(50));

        // Send start command
        tx.send(ClientCommand::Start).unwrap();

        let result = handle.join().unwrap();
        assert!(result.is_ok());
    }

    #[test]
    fn test_client_sends_packets() {
        let bitrate = 5_000_000.0; // 5 Mbps
        let payload_size = 512;
        let timeout = Duration::from_millis(200);

        let (mut client, tx) = create_test_client(bitrate, payload_size, timeout);
        let (mut server_sock, mut client_sock) = create_socket_pair();

        let handle = thread::spawn(move || client.run(&mut client_sock));

        tx.send(ClientCommand::Start).unwrap();

        let packets = receive_all_packets(&mut server_sock, Duration::from_millis(50));

        let result = handle.join().unwrap();
        assert!(result.is_ok());
        assert!(
            packets.len() > 0,
            "Should have received at least one packet"
        );
    }

    #[test]
    fn test_client_sends_fin_packet() {
        let bitrate = 10_000_000.0;
        let payload_size = 512;
        let timeout = Duration::from_millis(100);

        let (mut client, tx) = create_test_client(bitrate, payload_size, timeout);
        let (mut server_sock, mut client_sock) = create_socket_pair();

        let handle = thread::spawn(move || client.run(&mut client_sock));

        tx.send(ClientCommand::Start).unwrap();

        let packets = receive_all_packets(&mut server_sock, Duration::from_millis(50));

        let result = handle.join().unwrap();
        assert!(result.is_ok());

        // Last packet should be FIN
        let last_packet = packets.last().expect("Should have at least one packet");
        assert_eq!(last_packet.1, FLAG_FIN, "Last packet should have FIN flag");
    }

    #[test]
    fn test_sequence_numbers_increment_correctly() {
        let bitrate = 10_000_000.0;
        let payload_size = 512;
        let timeout = Duration::from_millis(150);

        let (mut client, tx) = create_test_client(bitrate, payload_size, timeout);
        let (mut server_sock, mut client_sock) = create_socket_pair();

        let handle = thread::spawn(move || client.run(&mut client_sock));

        tx.send(ClientCommand::Start).unwrap();

        let packets = receive_all_packets(&mut server_sock, Duration::from_millis(50));

        let result = handle.join().unwrap();
        assert!(result.is_ok());

        assert!(packets.len() >= 2, "Should have at least 2 packets");

        // Verify sequence numbers increment
        for i in 1..packets.len() {
            let expected_seq = packets[i - 1].0 + 1;
            assert_eq!(
                packets[i].0,
                expected_seq,
                "Sequence number should increment from {} to {}",
                packets[i - 1].0,
                expected_seq
            );
        }
    }

    #[test]
    fn test_client_timeout() {
        let bitrate = 1_000_000.0;
        let payload_size = 1024;
        let timeout = Duration::from_millis(200);

        let (mut client, tx) = create_test_client(bitrate, payload_size, timeout);
        let (_server_sock, mut client_sock) = create_socket_pair();

        tx.send(ClientCommand::Start).unwrap();

        let start = Instant::now();
        let result = client.run(&mut client_sock);
        let elapsed = start.elapsed();

        assert!(result.is_ok());
        assert!(
            elapsed >= timeout,
            "Should run for at least timeout duration"
        );
        assert!(
            elapsed < timeout + Duration::from_millis(100),
            "Should not run much longer than timeout"
        );
    }

    #[test]
    fn test_zero_timeout_sends_only_fin() {
        let bitrate = 1_000_000.0;
        let payload_size = 1024;
        let timeout = Duration::from_millis(0);

        let (mut client, tx) = create_test_client(bitrate, payload_size, timeout);
        let (mut server_sock, mut client_sock) = create_socket_pair();

        let handle = thread::spawn(move || client.run(&mut client_sock));

        tx.send(ClientCommand::Start).unwrap();

        let packets = receive_all_packets(&mut server_sock, Duration::from_millis(50));

        let result = handle.join().unwrap();
        assert!(result.is_ok());

        // Should only send FIN packet (sequence 0)
        assert_eq!(packets.len(), 1, "Should only send FIN with zero timeout");
        assert_eq!(packets[0].0, 0, "FIN should have sequence 0");
        assert_eq!(packets[0].1, FLAG_FIN, "Should be FIN packet");
    }

    #[test]
    fn test_no_duplicate_sequence_numbers() {
        let bitrate = 10_000_000.0;
        let payload_size = 512;
        let timeout = Duration::from_millis(150);

        let (mut client, tx) = create_test_client(bitrate, payload_size, timeout);
        let (mut server_sock, mut client_sock) = create_socket_pair();

        let handle = thread::spawn(move || client.run(&mut client_sock));

        tx.send(ClientCommand::Start).unwrap();

        let packets = receive_all_packets(&mut server_sock, Duration::from_millis(50));

        let _ = handle.join().unwrap();

        // Check for duplicates
        let mut seen_seqs = std::collections::HashSet::new();
        for (seq, _, _) in &packets {
            assert!(seen_seqs.insert(*seq), "Duplicate sequence number: {}", seq);
        }
    }
}
