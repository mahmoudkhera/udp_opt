//! [![github]](https://github.com/mahmoudkhera/udpopt)&ensp;[![crates-io]](https://crates.io/crates/udpopt)&ensp;[![docs-rs]](https://docs.rs/udpopt)
//!
//! [github]: https://img.shields.io/badge/github-8da0cb?style=for-the-badge&labelColor=555555&logo=github
//! [crates-io]: https://img.shields.io/badge/crates.io-fc8d62?style=for-the-badge&labelColor=555555&logo=rust
//! [docs-rs]: https://img.shields.io/badge/docs.rs-66c2a5?style=for-the-badge&labelColor=555555&logo=docs.rs
//!
//!  <br>
//!
//!  A high-performance synchronous and asynchronous UDP testing toolkit for sending and receiving UDP packets
//! with precise timing and bitrate control.
//!
//!
//! # Details
//!
//!
//! - Use `udpopt::UdpClient` to represent the test client
//! ```  
//! use std::net::UdpSocket;
//! use std::sync::mpsc;
//! use std::thread;
//! use std::time::Duration;
//! use udpopt::UdpClient;
//! use udpopt::ClientCommand;
//!
//! fn main() {
//!     
//!    # let mut sock = UdpSocket::bind("127.0.0.1:4000").expect("failed to bind");
//!     
//!     // Connect to server address
//!    # sock.connect("127.0.0.1:5000").expect("failed to connect");
//!
//!     
//!     let (tx, rx) = mpsc::channel();
//!
//!     // Create a UDP client instance
//!     let mut client = UdpClient::new(
//!         1_000_000.0,          // bitrate in bits per second
//!         1200,                 // payload size in bytes
//!         Duration::from_secs(5), // send duration
//!         rx,                   // control receiver
//!     );
//!
//!     // Spawn the client in a separate thread
//!     let handle = thread::spawn(move || {
//!         client.run(&mut sock).unwrap();
//!     });
//!
//!     // Send start command
//!     tx.send(ClientCommand::Start).unwrap();
//!
//!     // Wait for client to finish
//!     handle.join().unwrap();
//! }
//!
//! ```
//! #
//!
//! - After the duration is done the  after 5 seconds the otput should be
//!
//! ```console
//!   Client done. Sent 'Number of packets' packets (+FIN)
//!
//!  ```
//!
//! - Use `udpopt::UdpServer` to represent the test server
//!
//! ```
//! use std::net::UdpSocket;
//! use std::sync::mpsc;
//! use std::thread;
//! use std::time::Duration;
//! use udpopt::UdpServer;
//! use udpopt::ServerCommand;
//!
//! fn main()  {
//!    # // Bind UDP socket to listen on port 5000
//!    # let mut sock = UdpSocket::bind("127.0.0.1:5000").expect("failed to bind");
//!     
//!
//!     // Create a channel for controlling the server (start/stop)
//!     let (tx, rx) = mpsc::channel();
//!
//!     // Create a server that saves results every 5 seconds
//!     let mut server = UdpServer::new(Duration::from_secs(5), rx);
//!
//!     // Run the server in a separate thread
//!     let handle = thread::spawn(move || server.run(&mut sock));
//!
//!     // Start the test
//!     tx.send(ServerCommand::Start).unwrap();
//!
//!     // Simulate running for a while
//!     thread::sleep(Duration::from_secs(1));
//!
//!     // Stop the server
//!     tx.send(ServerCommand::Stop).unwrap();
//!
//!     // Wait for the server thread to finish
//!     let result = handle.join().unwrap();
//!
//!   #  println!("Server finished: {:?}", result);
//!     
//! }
//! ```
//!
//!  - This  module defines the [`TestResult`] struct, which aggregates multiple
//!   [`IntervalResult`] measurements that results from `UdpServer::run`
//!   into final performance metrics — total packets, bitrate, jitter, and more.
//!   Simulated interval results (normally collected by a UDP server)
//!
//!
//! ```rust
//! use std::time::Duration;
//! use udpopt::IntervalResult;
//! use udpopt::TestResult; 
//!
//! # let intervals = vec![
//! #     IntervalResult {
//! #         received: 950,
//! #         lost: 50,
//! #         bytes: 1_200_000,
//! #         time: Duration::from_secs(1),
//! #         jitter_ms: 0.8,
//! #         out_of_order: 2,
//! #         recommended_bitrate: 0,
//! #     },
//! #     IntervalResult {
//! #         received: 970,
//! #         lost: 30,
//! #         bytes: 1_250_000,
//! #         time: Duration::from_secs(1),
//! #         jitter_ms: 1.2,
//! #         out_of_order: 1,
//! #          recommended_bitrate: 0,
//! #     },
//! # ];
//!
//! // Aggregate the results
//! let result = TestResult::from_intervals(&intervals);
//! println!("Total packets: {}", result.total_packets);
//! println!("Mean bitrate: {:.2} bps", result.mean_bitrate);
//! println!("Median jitter: {:.2} ms", result.median_jitter);
//! ```
//! 
//! 
//! - This produces an output similar to:
//! ```text
//! Total packets: 1920
//! Mean bitrate: 9.60e6 bps
//! Median jitter: 1.00 ms
//! ```


mod client;
pub use client::UdpClient;

mod errors;
pub use errors::UdpOptError;
mod result;
pub use result::TestResult;
mod server;
pub use server::UdpServer;
mod utils;
pub use utils::net_utils::{ClientCommand, ServerCommand,IntervalResult};
pub use utils::ui;


// async part
mod async_client;
pub use async_client::AsyncUdpClient;
mod async_server;
pub use async_server::AsyncUdpServer;
