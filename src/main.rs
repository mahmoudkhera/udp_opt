use std::env;
use std::net::SocketAddr;
use std::time::Duration;

use udp_opt::udp_test::UdpTest;

// parse bitrate like "10M", "1.5G", "500K" -> bits per second (f64)
fn parse_bitrate(s: &str) -> Result<f64, String> {
    if s.is_empty() {
        return Err("empty bitrate".into());
    }
    let idx = s
        .find(|c: char| !c.is_ascii_digit() && c != '.')
        .unwrap_or(s.len());
    let (num, suffix) = s.split_at(idx);
    let base: f64 = num
        .parse()
        .map_err(|_| format!("invalid number: {}", num))?;
    let mult = match suffix.trim().to_ascii_uppercase().as_str() {
        "" => 1.0,
        "K" => 1_000.0,
        "M" => 1_000_000.0,
        "G" => 1_000_000_000.0,
        other => return Err(format!("invalid suffix: {}", other)),
    };
    Ok(base * mult)
}

fn parse_addr(s: &str) -> Result<SocketAddr, String> {
    s.parse().map_err(|e| format!("invalid addr {}: {}", s, e))
}

#[derive(Debug)]
struct Args {
    mode: String,
    bind: Option<SocketAddr>,
    connect: Option<SocketAddr>,
    bitrate: Option<f64>,
    duration: Option<u64>,
    size: Option<usize>,
    interval: Option<u64>,
}

fn parse_args() -> Result<Args, String> {
    let mut a = Args {
        mode: String::new(),
        bind: None,
        connect: None,
        bitrate: None,
        duration: None,
        size: None,
        interval: Some(1),
    };
    let mut it = env::args().skip(1);
    a.mode = it.next().ok_or("must specify mode: server|client")?;
    while let Some(k) = it.next() {
        match k.as_str() {
            "--bind" => a.bind = Some(parse_addr(&it.next().ok_or("--bind needs addr")?)?),
            "--connect" => a.connect = Some(parse_addr(&it.next().ok_or("--connect needs addr")?)?),
            "--bitrate" => {
                a.bitrate = Some(parse_bitrate(&it.next().ok_or("--bitrate needs value")?)?)
            }
            "--duration" => {
                a.duration = Some(
                    it.next()
                        .ok_or("--duration needs seconds")?
                        .parse()
                        .map_err(|_| "bad duration")?,
                )
            }
            "--size" => {
                a.size = Some(
                    it.next()
                        .ok_or("--size needs bytes")?
                        .parse()
                        .map_err(|_| "bad size")?,
                )
            }
            "--interval" => {
                a.interval = Some(
                    it.next()
                        .ok_or("--interval needs seconds")?
                        .parse()
                        .map_err(|_| "bad interval")?,
                )
            }
            other => return Err(format!("unknown arg: {}", other)),
        }
    }
    Ok(a)
}

fn usage_and_exit(msg: Option<&str>) {
    if let Some(m) = msg {
        eprintln!("{}", m);
    }
    eprintln!(
        "\nUsage:\n  server --bind <ip:port> [--interval N]\n  client --connect <ip:port> --bitrate <e.g., 10M> --duration <secs> --size <bytes> [--interval N]\n\nExamples:\n  server --bind 0.0.0.0:5201 --interval 1\n  client --connect 127.0.0.1:5201 --bitrate 10M --duration 10 --size 1200 --interval 1"
    );
    std::process::exit(1);
}

fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            usage_and_exit(Some(&format!("Error: {}", e)));
            return;
        }
    };

    match args.mode.as_str() {
        "server" => {
            let bind = args.bind.unwrap_or_else(|| "0.0.0.0:5201".parse().unwrap());
            let interval = Duration::from_secs(args.interval.unwrap_or(1));
            let duration = Duration::from_secs(10);
            let mut udp_test = UdpTest::new(bind, 0.0, 1000, interval, duration);
            let _ = udp_test.server();
        }
        "client" => {
            let dest = match args.connect {
                Some(d) => d,
                None => {
                    usage_and_exit(Some("--connect is required"));
                    return;
                }
            };
            let bitrate = match args.bitrate {
                Some(b) => b,
                None => {
                    usage_and_exit(Some("--bitrate is required"));
                    return;
                }
            };
            let duration = Duration::from_secs(args.duration.unwrap_or(10));
            let size = args.size.unwrap_or(1200).max(2000);
            let interval = Duration::from_secs(args.interval.unwrap_or(1));
            let mut udp_test = UdpTest::new(dest, bitrate, size, interval, duration);
            println!("....tst");
            let _ = udp_test.client(dest);
        }
        other => {
            usage_and_exit(Some(&format!("Unknown mode: {}. Use server|client", other)));
        }
    }
}
