use std::{net::ToSocketAddrs, time::Duration};

use udp_opt::udp_test::UdpTest;

fn main() {
    let addr = "192.168.1.7:5021"
        .to_socket_addrs()
        .unwrap()
        .next()
        .unwrap();

    let mut udp_test = UdpTest::new(
        addr,
        1000_000_00.0,
        1200,
        Duration::from_secs(1),
        Duration::from_secs(10),
    );

    let dest = "192.168.1.9:5021"
        .to_socket_addrs()
        .unwrap()
        .next()
        .unwrap();

    udp_test.client(dest).unwrap();
}
