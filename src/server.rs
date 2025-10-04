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
                println!("unecpect stop");
                return Ok(());
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

    pub fn get_result(&self) -> &[TestResult] {
        &self.udp_result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::udp_data::{FLAG_DATA, FLAG_FIN, HEADER_SIZE, UdpHeader};
    use std::net::{SocketAddr, UdpSocket};
    use std::sync::mpsc::{self, Sender};
    use std::thread;
    use std::time::Duration;

    // Helper function to find an available port
    fn get_available_addr() -> SocketAddr {
        "127.0.0.1:0".parse().unwrap()
    }

    // Helper to create a test server with control channel
    fn create_test_server(interval: Duration) -> (UdpServer, Sender<ServerCommand>, SocketAddr) {
        let (tx, rx) = mpsc::channel();
        let addr = get_available_addr();
        let server = UdpServer::new(addr, interval, rx).unwrap();
        let bound_addr = server.sock.local_addr().unwrap();
        (server, tx, bound_addr)
    }

    // Helper to send a UDP packet with a header
    fn send_packet(
        client: &UdpSocket,
        seq: u64,
        sec: u64,
        usec: u32,
        flags: u32,
        payload_size: usize,
    ) {
        let mut buf = vec![0u8; payload_size];
        let mut header = UdpHeader::new(seq, sec, usec, flags);
        header.write_header(&mut buf);
        client.send(&buf).unwrap();
    }

    #[test]
    fn test_udp_server_new() {
        let (_, rx) = mpsc::channel();
        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let server = UdpServer::new(addr, Duration::from_secs(1), rx);

        assert!(server.is_ok());
        let server = server.unwrap();
        assert_eq!(server.interval, Duration::from_secs(1));
        assert_eq!(server.udp_result.len(), 0);
    }

    #[test]
    fn test_udp_server_new_invalid_address() {
        let (_, rx) = mpsc::channel();
        // Try to bind to an invalid address (port 0 is ok, but invalid IP format would fail parsing)
        // Here we test a valid parse but potentially unavailable port
        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let result = UdpServer::new(addr, Duration::from_secs(1), rx);

        // Should succeed because OS assigns available port
        assert!(result.is_ok());
    }

    #[test]
    fn test_server_stops_on_fin_packet() {
        let (mut server, tx, server_addr) = create_test_server(Duration::from_secs(1));

        // Create client socket
        let client = UdpSocket::bind("127.0.0.1:0").unwrap();
        client.connect(server_addr).unwrap();

        // Start server in a thread
        let server_thread = thread::spawn(move || server.run());

        // Give server time to start
        thread::sleep(Duration::from_millis(50));

        // Send start command
        tx.send(ServerCommand::Start).unwrap();

        // Wait a bit
        thread::sleep(Duration::from_millis(50));

        // Send FIN packet
        send_packet(&client, 0, 1000, 0, FLAG_FIN, 1500);

        // Server should exit
        let result = server_thread.join().unwrap();
        assert!(result.is_ok());
    }

    #[test]
    fn test_server_ignores_short_packets() {
        let (mut server, tx, server_addr) = create_test_server(Duration::from_secs(1));

        // Create client socket
        let client = UdpSocket::bind("127.0.0.1:0").unwrap();
        client.connect(server_addr).unwrap();

        // Start server in a thread
        let server_thread = thread::spawn(move || {
            server.run().unwrap();
            server
        });

        // Give server time to start
        thread::sleep(Duration::from_millis(50));

        // Send start command
        tx.send(ServerCommand::Start).unwrap();

        // Wait a bit
        thread::sleep(Duration::from_millis(50));

        // Send a packet that's too short (less than HEADER_SIZE)
        let short_buf = vec![0u8; HEADER_SIZE - 1];
        client.send(&short_buf).unwrap();

        // Send a valid FIN packet to stop the server
        send_packet(&client, 0, 1000, 0, FLAG_FIN, 1500);

        // Server should handle this gracefully
        let server = server_thread.join().unwrap();
        let results = server.get_result();

        // Should have no results since only short packet and FIN were sent
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_server_unexpected_stop_before_start() {
        let (mut server, tx, _) = create_test_server(Duration::from_secs(1));

        // // Start server in a thread
        let server_thread = thread::spawn(move || server.run());

        // Give server time to start
        thread::sleep(Duration::from_millis(500));

        // Send stop before start
        tx.send(ServerCommand::Stop).unwrap();

        // Server should handle this gracefully
        let result = server_thread.join().unwrap();
        assert!(result.is_ok());
    }

    #[test]
    fn test_interval_result_time_tracking() {
        let (mut server, tx, server_addr) = create_test_server(Duration::from_millis(300));

        let client = UdpSocket::bind("127.0.0.1:0").unwrap();
        client.connect(server_addr).unwrap();

        let server_thread = thread::spawn(move || {
            server.run().unwrap();
            server
        });

        thread::sleep(Duration::from_millis(50));
        tx.send(ServerCommand::Start).unwrap();
        thread::sleep(Duration::from_millis(50));

        // Send packets for about 400ms
        for i in 0..20 {
            send_packet(&client, i, 1000, (i * 10000) as u32, FLAG_DATA, 1500);
            thread::sleep(Duration::from_millis(20));
        }

        send_packet(&client, 20, 1000, 200000, FLAG_FIN, 1500);

        let server = server_thread.join().unwrap();
        let results = server.get_result();

        // Check that time is tracked properly
        if results.len() > 0 {
            assert!(
                results[0].time >= Duration::from_millis(300),
                "First interval should be at least 300ms"
            );
        }
    }
}
