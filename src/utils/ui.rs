use std::time::Instant;

use crate::utils::net_utils::IntervalResult;

pub fn print_result(test_result: &IntervalResult) {
    let elapsed = test_result.time.as_secs_f64();
    let mbps = if elapsed > 0.0 {
        (test_result.bytes as f64 * 8.0) / elapsed / 1_000_000.0
    } else {
        0.0
    };
    println!(
        " Elapsed {:.2}s | Recv {} pkts | Lost {} | OOO {} | Jitter {:.3} ms | Rate {:.3} Mbps",
        elapsed,
        test_result.received,
        test_result.lost,
        test_result.out_of_order,
        test_result.jitter_ms,
        mbps
    );
}

// pub fn final_report(test_result:TestResult) {
//     let elapsed = test_result.time.as_secs_f64();
//     let mbps = if elapsed > 0.0 {
//         (test_result.result.bytes as f64 * 8.0) / elapsed / 1_000_000.0
//     } else {
//         0.0
//     };

//     let loss_pct = if total_sent > 0 {
//         (intervatest_result.result.l_result.lost as f64) / (total_sent as f64) * 100.0
//     } else {
//         0.0
//     };
//     println!(
//         "FINAL   Duration: {:.2}s\n  Bytes: {}\n  Throughput: {:.3} Mbps\n  Received: {} pkts\n  Lost: {} pkts ({:.2}%)\n  Out-of-order: {}\n  Jitter: {:.3} ms",
//         elapsed,
//         intervatest_result.result.l_result.bytes,
//         mbps,
//         intervatest_result.result.l_result.received,
//         intervatest_result.result.l_result.lost,
//         loss_pct,
//         intervatest_result.result.l_result.out_of_order,
//         intervatest_result.result.l_result.jitter_ms
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
