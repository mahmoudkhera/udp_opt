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
        self.sock.connect(&dest).map_err(UdpOptError::ConnectFailed)?;

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

            self.sock.send(&buf).map_err(|e| UdpOptError::SendFailed(e))?;

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

        self.sock.send(&buf).map_err(|e| UdpOptError::SendFailed(e))?;
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
