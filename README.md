# udpopt

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE-MIT)  
[![License: Apache 2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](LICENSE-APACHE)

udpopt is a Rust library crate for building high-performance UDP throughput and latency testing tools.
It provides asynchronous and synchronous implementations of UDP clients and servers that can send and receive packets with precise timing, measure bitrates, and collect detailed performance results 



##  Features

- Asynchronous (tokio) and synchronous UDP clients and servers

-  packet pacing at configurable bitrates

- Interval-based performance measurement (bitrate, packet loss, etc.)

- Start/Stop control via channels for coordinated tests

- Easy to integrate into other network test systems or benchmarking tools



## Note : 
 This project was designed to be a feature in my ideal testing tool - but I decided to make it a separate project .



