//! UDP Server implementation for measuring UDP throughput and performance.
//!
//! This module provides [`UdpServer`] — a simple and efficient UDP server
//! that can receive UDP packets, calculate bitrate periodically, and store
//! interval-based test results.

use crate::errors::UdpOptError;
use crate::utils::net_utils::{IntervalResult, ServerCommand};
use crate::utils::udp_data::{FLAG_FIN, HEADER_SIZE, UdpData, UdpHeader};
use std::net::UdpSocket;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

#[derive(Debug)]
pub struct UdpServer {
    ///Time between each result to save
    interval: Duration,
    /// Collecting the interval results
    udp_result: Vec<IntervalResult>,
    /// Async receiver for control commands (`Start`, `Stop`) from another thread.
    control_rx: Receiver<ServerCommand>,
}

impl UdpServer {
    /// Creates a new [`UdpServer`] that binds to the given socket address.
    ///
    /// - `interval`: The duration for each result interval.
    /// - `control_rx`: A channel receiver to control start/stop commands.

    pub fn new(interval: Duration, control_rx: Receiver<ServerCommand>) -> Self {
        Self {
            interval,
            udp_result: Vec::with_capacity(100),
            control_rx,
        }
    }
    /// Runs the UDP server loop.
    ///
    /// - Waits for a `Start` command on the control channel before starting.
    /// The loop terminates when:
    /// - A `Stop` command is received.
    /// - A packet with the `FLAG_FIN` flag is received.
    /// - The control channel disconnects.
    ///
    ///
    ///  /// # Arguments
    /// - `sock`: The bound UDP socket to receive packets from.
    ///
    /// Returns a slice of collected [`IntervalResult`]s.
    ///
    ///
    /// # Errors
    ///
    /// Returns [`UdpOptError::RecvFailed`] if a UDP receive error occurs.
    /// Returns [`UdpOptError::SocketTimeout`] if a UDP receive error occurs.
    /// Returns [`UdpOptError::UnexpectedCommand`] if a UDP receive error occurs.
    /// Returns [`UdpOptError::ChannelClosed`] if a UDP receive error occurs.
    pub fn run(&mut self, sock: &mut UdpSocket) -> Result<Vec<IntervalResult>, UdpOptError> {
        println!("server start");

        let mut udp_data = UdpData::new();
        let mut buf = vec![0u8; 2048];

        // wait for the start udp packet to start the test and set the buf lenght
        match self.control_rx.recv() {
            Ok(ServerCommand::Stop) => return Err(UdpOptError::UnexpectedCommand),
            Ok(ServerCommand::Start) => {}
            Err(_) => return Err(UdpOptError::ChannelClosed),
        }

        // start measuring after reciving the first packt
        let _ = sock
            .recv(&mut buf)
            .map_err(|e| UdpOptError::RecvFailed(e))?;

        sock.set_read_timeout(Some(Duration::from_secs(2)))
            .map_err(|_| UdpOptError::SocketTimeout)?;

        println!("server     start");

        let mut calc_instat = Instant::now();
        let calc_interval = Duration::from_millis(200);
        let mut start = Instant::now();

        println!("Collecting..");

        loop {
            // Check control messages
            match self.control_rx.try_recv() {
                Ok(ServerCommand::Stop) => break,
                Ok(ServerCommand::Start) => return Err(UdpOptError::UnexpectedCommand),
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => return Err(UdpOptError::ChannelClosed),
            }

            let len = sock
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
                let res = udp_data.get_interval_result(start.elapsed());
                self.udp_result.push(res);
                start = Instant::now();
            }
        }
        
        println!("test finished");
        // if the interval time bigger than the total time the client send
        if self.udp_result.len()==0{
            self.udp_result.push(udp_data.get_interval_result(start.elapsed()));
        }
        
        Ok(std::mem::take(&mut self.udp_result))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::UdpSocket;
    use std::sync::mpsc::{Sender, channel};
    use std::thread;
    use std::time::Duration;

    // Helper function to create a test server
    fn create_test_server(interval: Duration) -> (UdpServer, Sender<ServerCommand>) {
        let (tx, rx) = channel();
        let server = UdpServer::new(interval, rx);
        (server, tx)
    }

    // Helper function to create a bound UDP socket pair
    fn create_socket_pair() -> (UdpSocket, UdpSocket) {
        let server_sock = UdpSocket::bind("127.0.0.1:0").expect("Failed to bind server socket");
        let client_sock = UdpSocket::bind("127.0.0.1:0").expect("Failed to bind client socket");

        let server_addr = server_sock.local_addr().unwrap();
        let client_addr = client_sock.local_addr().unwrap();

        server_sock.connect(client_addr).unwrap();
        client_sock.connect(server_addr).unwrap();

        (server_sock, client_sock)
    }

    // Helper to create a UDP packet with header
    fn create_packet(seq: u64, flags: u32) -> Vec<u8> {
        let mut packet = vec![0u8; HEADER_SIZE + 100]; // Header + some payload

        // Assuming UdpHeader layout (adjust based on your actual implementation)
        packet[0..8].copy_from_slice(&seq.to_be_bytes());
        packet[20..24].copy_from_slice(&flags.to_be_bytes());

        packet
    }

    #[test]
    fn test_server_waits_for_start_command() {
        let (mut server, tx) = create_test_server(Duration::from_secs(1));
        let (mut server_sock, client_sock) = create_socket_pair();

        // Run the server in a separate thread
        let handle = thread::spawn(move || server.run(&mut server_sock));

        // Ensure the thread starts and is ready to receive
        thread::sleep(Duration::from_millis(100));

        // Send Start command
        tx.send(ServerCommand::Start).unwrap();

        // Give the server a moment to process the Start command
        thread::sleep(Duration::from_millis(50));

        // Send one UDP packet
        let packet = create_packet(1, 0);
        client_sock.send(&packet).unwrap();

        // Give time for the server to process the packet
        thread::sleep(Duration::from_millis(100));

        // Send Stop command to tell the server to exit
        tx.send(ServerCommand::Stop).unwrap();

        // Unblock the server if it's still in recv()
        client_sock.send(&create_packet(999, 0)).unwrap();

        // Wait for server to finish
        let result = handle.join().unwrap();
        println!("Server result: {:?}", result);
        assert!(result.is_ok());
    }

    #[test]
    fn test_server_stops_on_stop_command() {
        let (mut server, tx) = create_test_server(Duration::from_secs(1));
        let (mut server_sock, client_sock) = create_socket_pair();

        server_sock
            .set_read_timeout(Some(Duration::from_millis(100)))
            .unwrap();

        let handle = thread::spawn(move || server.run(&mut server_sock));

        // Send start command
        tx.send(ServerCommand::Start).unwrap();
        thread::sleep(Duration::from_millis(50));

        // Send initial packet
        let packet = create_packet(1, 0);
        client_sock.send(&packet).unwrap();
        thread::sleep(Duration::from_millis(50));

        // Send stop command
        tx.send(ServerCommand::Stop).unwrap();

        // Unblock the server if it's still in recv()
        client_sock.send(&create_packet(999, 0)).unwrap();

        let result = handle.join().unwrap();
        assert!(result.is_ok());
    }

    #[test]
    fn test_server_stops_on_fin_flag() {
        let (mut server, tx) = create_test_server(Duration::from_secs(1));
        let (mut server_sock, client_sock) = create_socket_pair();

        let handle = thread::spawn(move || server.run(&mut server_sock));

        // Send start command
        tx.send(ServerCommand::Start).unwrap();
        thread::sleep(Duration::from_millis(50));

        // Send initial packet
        let packet = create_packet(1, 0);
        client_sock.send(&packet).unwrap();
        thread::sleep(Duration::from_millis(50));

        // Send FIN packet
        let fin_packet = create_packet(2, 1);
        client_sock.send(&fin_packet).unwrap();

        let result = handle.join().unwrap();
        assert!(result.is_ok());
    }

    #[test]
    fn test_interval_result_collection() {
        let interval = Duration::from_millis(200);
        let (mut server, tx) = create_test_server(interval);
        let (mut server_sock, client_sock) = create_socket_pair();

        server_sock
            .set_read_timeout(Some(Duration::from_millis(100)))
            .unwrap();

        let handle = thread::spawn(move || server.run(&mut server_sock).unwrap());

        tx.send(ServerCommand::Start).unwrap();
        thread::sleep(Duration::from_millis(50));

        // Send initial packet
        let packet = create_packet(1, 0);
        client_sock.send(&packet).unwrap();

        // Send packets over multiple intervals
        for i in 2..=10 {
            thread::sleep(Duration::from_millis(50));
            let packet = create_packet(i, 0);
            client_sock.send(&packet).unwrap();
        }

        thread::sleep(Duration::from_millis(100));
        tx.send(ServerCommand::Stop).unwrap();

        // Unblock the server if it's still in recv()
        client_sock.send(&create_packet(999, 0)).unwrap();

        let results = handle.join().unwrap();

        // Should have collected at least one interval result
        assert!(results.len() > 0);
    }

    #[test]
    fn test_multiple_start_commands() {
        let (mut server, tx) = create_test_server(Duration::from_secs(1));
        let (mut server_sock, client_sock) = create_socket_pair();

        server_sock
            .set_read_timeout(Some(Duration::from_millis(100)))
            .unwrap();

        let handle = thread::spawn(move || server.run(&mut server_sock));

        tx.send(ServerCommand::Start).unwrap();
        thread::sleep(Duration::from_millis(50));

        // Send initial packet
        let packet = create_packet(1, 0);
        client_sock.send(&packet).unwrap();
        thread::sleep(Duration::from_millis(50));

        // Send another start command (should be ignored)
        tx.send(ServerCommand::Start).unwrap();

        // Send another packet
        let packet2 = create_packet(2, 0);
        client_sock.send(&packet2).unwrap();
        thread::sleep(Duration::from_millis(50));

        thread::sleep(Duration::from_millis(50));

        // Unblock the server if it's still in recv()
        client_sock.send(&create_packet(999, 0)).unwrap();

        let result = handle.join().unwrap();

        assert!(result.is_err());
    }
}
