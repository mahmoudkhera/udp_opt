# udpopt

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE-MIT)  
[![License: Apache 2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](LICENSE-APACHE)

`udp_opt` is A simple UDP performance testing and congestion control tool written in Rust.  
It works similarly to `iperf`, allowing you to measure throughput, packet loss, jitter, and out-of-order packets.



## ✨ Features

// - Client: sends UDP datagrams containing a 24-byte header (seq + timestamp + flags)
// - Server: per-client stats (throughput, loss, out-of-order, using  RFC3550 jitter formula )
// - FIN packet signals end-of-test (client sends, server prints final summary)
// - Configurable payload size, duration, bitrate, and report interval 

---
#  Usage:
- cargo new udpperf && replace src/main.rs with this file
-  cargo run --release -- server --bind 0.0.0.0:5201 --interval 1
-  cargo run --release -- client --connect 127.0.0.1:5201 --bitrate 10M --duration 10 --size 1200 --interval 1


## Not : 
 This project was designed to be a feature in my ideal testing tool - but I decided to make it a separate project .



