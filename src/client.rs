//! UDP Client for sending high-performance test packets to a UDP server.
//!
//! This module provides [`UdpClient`] — a client that can send UDP packets
//! at a specified bitrate with precise timing, including support for start/stop
//! commands via an `mpsc` channel.

use std::{
    net::{SocketAddr, UdpSocket},
    sync::mpsc::Receiver,
    time::{Duration, Instant},
};

use crate::{
    errors::UdpOptError,
    random_utils::RandomToSend,
    udp_data::{FLAG_DATA, FLAG_FIN, UdpHeader, now_micros},
};

/// Commands that control the UDP client behavior.
#[derive(Debug, Clone)]
pub enum ClientCommand {
    Start,
    Stop,
}
#[derive(Debug)]
pub struct UdpClient {
    sock: UdpSocket,
    bitrate_bps: f64,
    payload_size: usize,
    timeout: Duration,
    control_rx: Receiver<ClientCommand>,
}

impl UdpClient {
    /// Creates a new UDP client bound to a local address.
    ///
    /// # Parameters
    /// - `addr`: Local socket address to bind.
    /// - `bitrate_bps`: Target sending bitrate in bits per second.
    /// - `payload_size`: Size of each UDP packet in bytes.
    /// - `timeout`: Maximum duration to send packets.
    /// - `control_rx`: Channel receiver for start/stop commands.
    ///
    /// # Errors
    /// Returns [`UdpOptError::BindFailed`] if the socket cannot be bound.
    pub fn new(
        addr: SocketAddr,
        bitrate_bps: f64,
        payload_size: usize,
        timeout: Duration,
        control_rx: Receiver<ClientCommand>,
    ) -> Result<Self, UdpOptError> {
        let sock = UdpSocket::bind(addr).map_err(UdpOptError::BindFailed)?;

        Ok(Self {
            sock: sock,
            bitrate_bps,
            payload_size,
            timeout,
            control_rx,
        })
    }

    /// Runs the UDP client, sending packets to the specified destination.
    ///
    /// - Waits for a `Start` command from the control channel before sending.
    /// - Sends packets according to the configured bitrate and payload size.
    /// - Stops after `timeout` duration or when the control channel sends `Stop`.
    /// - Sends a FIN packet at the end to notify the server.
    ///
    /// # Parameters
    /// - `dest`: Destination UDP socket address.
    ///
    /// # Errors
    /// Returns [`UdpOptError::SendFailed`] if a packet cannot be sent.
    /// Returns [`UdpOptError::FailToGetRandom`] if random data cannot be generated.
    /// Returns [`UdpOptError::ConnectFailed`] if socket cannot connect.

    pub fn run(&mut self, dest: SocketAddr) -> Result<(), UdpOptError> {
        self.sock
            .connect(&dest)
            .map_err(UdpOptError::ConnectFailed)?;

        let interval_per_packet = ipp(self.payload_size, self.bitrate_bps);

        let mut seq: u64 = 0;

        let mut buf = vec![0u8; self.payload_size];

        let mut random = RandomToSend::new().map_err(|e| UdpOptError::FailToGetRandom(e))?;

        match self.control_rx.recv().unwrap() {
            ClientCommand::Stop => {
                println!("unecpect stop")
            }
            ClientCommand::Start => {}
        }
        let start = Instant::now();

        loop {
            if start.elapsed() >= self.timeout {
                break;
            }

            random
                .fill(&mut buf)
                .map_err(|e| UdpOptError::FailToGetRandom(e))?; //  not you can use any random  base insted of using the unix_epoch
            let (sec, usec) = now_micros();
            let mut header = UdpHeader::new(seq, sec, usec, FLAG_DATA);
            header.write_header(&mut buf);

            self.sock
                .send(&buf)
                .map_err(|e| UdpOptError::SendFailed(e))?;

            seq += 1;
            time_to_next_target(seq, interval_per_packet, start);
        }

        // FIN
        random
            .fill(&mut buf)
            .map_err(|e| UdpOptError::FailToGetRandom(e))?; //  not you can use any random  base insted of using the unix_epoch
        let (sec, usec) = now_micros();
        let mut fin = UdpHeader::new(seq, sec, usec, FLAG_FIN);
        fin.write_header(&mut buf);

        self.sock
            .send(&buf)
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
mod tests {
    use super::*;
    use crate::udp_data::{FLAG_DATA, FLAG_FIN, HEADER_SIZE, UdpHeader};
    use std::net::{SocketAddr, UdpSocket};
    use std::sync::mpsc::{self, Sender};
    use std::thread;
    use std::time::{Duration, Instant};

    // Helper function to get an available address
    fn get_available_addr() -> SocketAddr {
        "127.0.0.1:0".parse().unwrap()
    }

    // Helper to create a test client with control channel
    fn create_test_client(
        bitrate_bps: f64,
        payload_size: usize,
        timeout: Duration,
    ) -> (UdpClient, Sender<ClientCommand>) {
        let (tx, rx) = mpsc::channel();
        let addr = get_available_addr();
        let client = UdpClient::new(addr, bitrate_bps, payload_size, timeout, rx).unwrap();
        (client, tx)
    }

    // Helper to create a receiving server
    fn create_receiver() -> (UdpSocket, SocketAddr) {
        let sock = UdpSocket::bind("127.0.0.1:0").unwrap();
        sock.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let addr = sock.local_addr().unwrap();
        (sock, addr)
    }

    #[test]
    fn test_udp_client_new() {
        let (_, rx) = mpsc::channel();
        let addr = get_available_addr();
        let client = UdpClient::new(addr, 1_000_000.0, 1500, Duration::from_secs(1), rx);

        assert!(client.is_ok());
        let client = client.unwrap();
        assert_eq!(client.bitrate_bps, 1_000_000.0);
        assert_eq!(client.payload_size, 1500);
        assert_eq!(client.timeout, Duration::from_secs(1));
    }

    #[test]
    fn test_ipp_calculation() {
        let interval = ipp(1500, 1_000_000.0);
        assert!(interval.as_millis() >= 11 && interval.as_millis() <= 13);
    }

    #[test]
    fn test_client_sends_packets() {
        let (mut client, tx) = create_test_client(100_000.0, 100, Duration::from_millis(200));
        let (receiver, receiver_addr) = create_receiver();

        // Start client in a thread
        let client_thread = thread::spawn(move || client.run(receiver_addr));

        // Send start command
        tx.send(ClientCommand::Start).unwrap();

        let mut buf = vec![0u8; 2048];
        let mut packet_count = 0;
        let mut received_fin = false;

        // Receive packets
        loop {
            match receiver.recv(&mut buf) {
                Ok(len) => {
                    if len >= HEADER_SIZE {
                        let header = UdpHeader::read_header(&mut buf);
                        packet_count += 1;

                        if header.flags == FLAG_FIN {
                            received_fin = true;
                            break;
                        }
                    }
                }
                Err(_) => break, // Timeout
            }
        }

        // Wait for client to finish
        let result = client_thread.join().unwrap();
        assert!(result.is_ok());
        assert!(packet_count > 0, "Should receive at least one packet");
        assert!(received_fin, "Should receive FIN packet");
    }

    #[test]
    fn test_client_respects_timeout() {
        let timeout = Duration::from_millis(100);
        let (mut client, tx) = create_test_client(1_000_000.0, 1000, timeout);
        let (_, receiver_addr) = create_receiver();

        let start = Instant::now();

        // Start client in a thread
        let client_thread = thread::spawn(move || client.run(receiver_addr));

        // Send start command
        tx.send(ClientCommand::Start).unwrap();

        // Wait for client to finish
        client_thread.join().unwrap().unwrap();

        let elapsed = start.elapsed();

        // Should finish around the timeout (with some margin)
        assert!(elapsed >= timeout);
        assert!(elapsed < timeout + Duration::from_millis(200));
    }

    #[test]
    fn test_client_fin_packet() {
        let (mut client, tx) = create_test_client(100_000.0, 500, Duration::from_millis(50));
        let (receiver, receiver_addr) = create_receiver();

        // Start client in a thread
        let client_thread = thread::spawn(move || client.run(receiver_addr));

        // Send start command
        tx.send(ClientCommand::Start).unwrap();

        let mut buf = vec![0u8; 2048];
        let mut last_seq = 0u64;
        let mut fin_found = false;

        // Receive packets
        loop {
            match receiver.recv(&mut buf) {
                Ok(len) => {
                    if len >= HEADER_SIZE {
                        let header = UdpHeader::read_header(&mut buf);

                        if header.flags == FLAG_FIN {
                            fin_found = true;
                            // FIN should come after all data packets
                            assert!(
                                header.seq >= last_seq,
                                "FIN sequence should be >= last data sequence"
                            );
                            break;
                        }

                        last_seq = header.seq;
                    }
                }
                Err(_) => break,
            }
        }

        client_thread.join().unwrap().unwrap();
        assert!(fin_found, "FIN packet should be sent");
    }

    #[test]
    fn test_client_unexpected_stop_before_start() {
        let (mut client, tx) = create_test_client(100_000.0, 500, Duration::from_millis(100));
        let (_, receiver_addr) = create_receiver();

        // Start client in a thread
        let client_thread = thread::spawn(move || client.run(receiver_addr));

        // Send stop before start
        tx.send(ClientCommand::Stop).unwrap();

        // Should handle gracefully
        let result = client_thread.join().unwrap();
        assert!(result.is_ok());
    }

    #[test]
    fn test_time_to_next_target_precision() {
        let start = Instant::now();
        let ipp = Duration::from_millis(10);

        // Test first packet (seq 1)
        time_to_next_target(1, ipp, start);
        let elapsed = start.elapsed();

        // Should wait approximately 10ms
        assert!(elapsed >= Duration::from_millis(9));
        assert!(elapsed <= Duration::from_millis(15));
    }

    #[test]
    fn test_time_to_next_target_multiple_packets() {
        let start = Instant::now();
        let ipp = Duration::from_millis(5);

        // Simulate sending 5 packets
        for seq in 1..=5 {
            time_to_next_target(seq, ipp, start);
        }

        let elapsed = start.elapsed();

        // Should take approximately 25ms (5 * 5ms)
        assert!(elapsed >= Duration::from_millis(23));
        assert!(elapsed <= Duration::from_millis(30));
    }

    #[test]
    fn test_time_to_next_target_already_past() {
        let start = Instant::now();
        thread::sleep(Duration::from_millis(50));

        // Packet is already late
        let ipp = Duration::from_millis(10);

        let before = Instant::now();
        time_to_next_target(1, ipp, start);
        let wait_time = before.elapsed();

        // Should return immediately (very small wait)
        assert!(wait_time < Duration::from_millis(1));
    }
}
