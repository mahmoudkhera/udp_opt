use std::time::Instant;

use crate::udp_data::UdpData;

pub fn take_period_report(udp_data: &UdpData, time: Instant) {
    let elapsed = time.elapsed().as_secs_f64();
    let mbps = if elapsed > 0.0 {
        (udp_data.bytes as f64 * 8.0) / elapsed / 1_000_000.0
    } else {
        0.0
    };
    println!(
        " Elapsed {:.2}s | Recv {} pkts | Lost {} | OOO {} | Jitter {:.3} ms | Rate {:.3} Mbps",
        elapsed, udp_data.received, udp_data.lost, udp_data.out_of_order, udp_data.jitter_ms, mbps
    );
}

pub fn final_report(udp_data: &UdpData, time: Instant) {
    let elapsed = time.elapsed().as_secs_f64();
    let mbps = if elapsed > 0.0 {
        (udp_data.bytes as f64 * 8.0) / elapsed / 1_000_000.0
    } else {
        0.0
    };
    let total_sent = if let Some(last) = udp_data.last_seq {
        last + 1
    } else {
        0
    };
    let loss_pct = if total_sent > 0 {
        (udp_data.lost as f64) / (total_sent as f64) * 100.0
    } else {
        0.0
    };
    println!(
        "FINAL   Duration: {:.2}s\n  Bytes: {}\n  Throughput: {:.3} Mbps\n  Received: {} pkts\n  Lost: {} pkts ({:.2}%)\n  Out-of-order: {}\n  Jitter: {:.3} ms",
        elapsed,
        udp_data.bytes,
        mbps,
        udp_data.received,
        udp_data.lost,
        loss_pct,
        udp_data.out_of_order,
        udp_data.jitter_ms
    );
}
