use std::{net::SocketAddr, time::Duration};

use tokio::{
    net::UdpSocket,
    sync::{
        broadcast::{Receiver, error::TryRecvError},
    },
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
    sock: UdpSocket,
    interval: Duration,
    udp_result: Vec<IntervalResult>,
    control_rx: Receiver<ServerCommand>,
}

impl AsyncUdpServer {
    /// Creates a new async UDP server bound to the given local address.
    pub async fn new(
        addr: SocketAddr,
        interval: Duration,
        control_rx: Receiver<ServerCommand>,
    ) -> Result<Self, UdpOptError> {
        let sock = UdpSocket::bind(addr)
            .await
            .map_err(UdpOptError::BindFailed)?;
        Ok(Self {
            sock,
            interval,
            udp_result: Vec::with_capacity(100),
            control_rx,
        })
    }

    pub async fn run(&mut self) -> Result<(), UdpOptError> {
        println!("server start");

        let mut udp_data = UdpData::new();
        let mut buf = vec![0u8; 2048];

        // Wait for Start or Stop before beginning
        match self.control_rx.recv().await {
            Ok(ServerCommand::Start) => {}
            Ok(ServerCommand::Stop) => {
                return Err(UdpOptError::UnexpectedCommand);
            }
            Err(_) => {
                return Err(UdpOptError::ChannelError);
            }
        }
        println!("server startweeeeeeeeeeeeeeeeeeeeeeeee");

        // start measuring after reciving the first packt
        let _ = self
            .sock
            .recv(&mut buf)
            .await
            .map_err(|e| UdpOptError::RecvFailed(e))?;

        let mut calc_instat = Instant::now();
        let calc_interval = Duration::from_millis(200);
        let mut start = Instant::now();

        loop {
            match self.control_rx.try_recv() {
                Ok(ServerCommand::Stop) => {
                    break;
                }
                Ok(ServerCommand::Start) => {
                    println!("unexpect start");
                }
                Err(TryRecvError::Empty) => {}
                Err(_) => {
                    return Err(UdpOptError::ChannelError);
                }
            }

            let len = self
                .sock
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

            print!("header flag {}", header.flags);
            if header.flags == FLAG_FIN {
                println!("server cccccccccccccccccccccc");

                return Ok(());
            }
            if start.elapsed() >= self.interval {
                let res = udp_data.get_interval_result(start.elapsed());
                print_result(&res);
                self.udp_result.push(res);
                start = Instant::now();
            }
        }
        println!("test finished");
        Ok(())
    }
}





#[cfg(test)]
mod tests {
    use crate::utils::udp_data::FLAG_DATA;

    use super::*;
    use std::net::{IpAddr, Ipv4Addr};
    use tokio::sync::broadcast;

    // Helper function to create a test server
    async fn create_test_server(port: u16) -> (AsyncUdpServer, broadcast::Sender<ServerCommand>) {
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
        let interval = Duration::from_secs(1);
        let (tx, rx) = broadcast::channel(10);

        let server = AsyncUdpServer::new(addr, interval, rx)
            .await
            .expect("Failed to create server");

        (server, tx)
    }

    // Helper function to create a client socket
    async fn create_client(server_port: u16) -> UdpSocket {
        let client_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
        let sock = UdpSocket::bind(client_addr).await.unwrap();
        sock.connect(SocketAddr::new(
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            server_port,
        ))
        .await
        .unwrap();
        sock
    }

    #[tokio::test]
    async fn test_server_creation() {
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 9001);
        let interval = Duration::from_secs(1);
        let (_tx, rx) = broadcast::channel(10);

        let result = AsyncUdpServer::new(addr, interval, rx).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_server_bind_failure() {
        // Try to bind to an invalid address
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(255, 255, 255, 255)), 9002);
        let interval = Duration::from_secs(1);
        let (_tx, rx) = broadcast::channel(10);

        let result = AsyncUdpServer::new(addr, interval, rx).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), UdpOptError::BindFailed(_)));
    }

    #[tokio::test]
    async fn test_server_waits_for_start_command() {
        let (mut server, tx) = create_test_server(9003).await;

        let server_handle = tokio::spawn(async move { server.run().await });

        // Give server time to start
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Send start command
        tx.send(ServerCommand::Start).unwrap();

        // Send a FIN packet to stop the server
        let client = create_client(9003).await;
        let mut packet = vec![0u8; HEADER_SIZE];
        packet[12] = FLAG_DATA as u8; // Set FIN flag in header
        client.send(&packet).await.unwrap();

        tx.send(ServerCommand::Stop).unwrap();

        let result = server_handle.await.unwrap();

        println!("{:?}", result);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_server_stops_on_stop_command_before_start() {
        let (mut server, tx) = create_test_server(9004).await;

        // Send stop command before start
        tx.send(ServerCommand::Stop).unwrap();

        let result = server.run().await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            UdpOptError::UnexpectedCommand
        ));
    }

    #[tokio::test]
    async fn test_server_handles_channel_closed() {
        let (mut server, tx) = create_test_server(9005).await;

        // Drop the sender to close the channel
        drop(tx);

        let result = server.run().await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), UdpOptError::ChannelError));
    }

    #[tokio::test]
    async fn test_server_fin_flag_stops_loop() {
        let (mut server, tx) = create_test_server(9008).await;

        let server_handle = tokio::spawn(async move { server.run().await });

        tokio::time::sleep(Duration::from_millis(50)).await;
        tx.send(ServerCommand::Start).unwrap();

        let client = create_client(9008).await;

        // Send normal packet
        let packet = vec![0u8; 100];
        client.send(&packet).await.unwrap();

        tokio::time::sleep(Duration::from_millis(50)).await;

        // Send FIN packet - this should stop the server loop
        let mut fin_packet = vec![0u8; HEADER_SIZE];
        fin_packet[20..24].copy_from_slice(&FLAG_FIN.to_be_bytes());
        client.send(&fin_packet).await.unwrap();

        // Server should exit gracefully
        let result = tokio::time::timeout(Duration::from_secs(2), server_handle).await;
        assert!(result.is_ok());
        assert!(result.unwrap().unwrap().is_ok());
    }

   
}
