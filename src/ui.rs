use std::time::Instant;

use crate::udp_data::{IntervalResult};

pub fn take_period_report(interval_result: &IntervalResult, time: Instant) {
    let elapsed = time.elapsed().as_secs_f64();
    let mbps = if elapsed > 0.0 {
        (interval_result.bytes as f64 * 8.0) / elapsed / 1_000_000.0
    } else {
        0.0
    };
    println!(
        " Elapsed {:.2}s | Recv {} pkts | Lost {} | OOO {} | Jitter {:.3} ms | Rate {:.3} Mbps",
        elapsed, interval_result.received, interval_result.lost, interval_result.out_of_order, interval_result.jitter_ms, mbps
    );
}

// pub fn final_report(interval_result: &UdpData, time: Instant) {
//     let elapsed = time.elapsed().as_secs_f64();
//     let mbps = if elapsed > 0.0 {
//         (interval_result.bytes as f64 * 8.0) / elapsed / 1_000_000.0
//     } else {
//         0.0
//     };
//     let total_sent = if let Some(last) = interval_result.last_seq {
//         last + 1
//     } else {
//         0
//     };
//     let loss_pct = if total_sent > 0 {
//         (interval_result.lost as f64) / (total_sent as f64) * 100.0
//     } else {
//         0.0
//     };
//     println!(
//         "FINAL   Duration: {:.2}s\n  Bytes: {}\n  Throughput: {:.3} Mbps\n  Received: {} pkts\n  Lost: {} pkts ({:.2}%)\n  Out-of-order: {}\n  Jitter: {:.3} ms",
//         elapsed,
//         interval_result.bytes,
//         mbps,
//         interval_result.received,
//         interval_result.lost,
//         loss_pct,
//         interval_result.out_of_order,
//         interval_result.jitter_ms
//     );
// }

pub fn client_period_report(start: Instant, payload: usize, seq: usize) {
    let elapsed = start.elapsed().as_secs_f64();
    let sent_bytes = (seq as usize * payload) as f64;
    let mbps = (sent_bytes * 8.0) / elapsed / 1_000_000.0;
    println!(
        "Elapsed {:.2}s | Sent {} pkts | est rate {:.3} Mbps",
        elapsed, seq, mbps
    );
}
