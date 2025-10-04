use std::{sync::mpsc, thread, time::Duration};

use udp_opt::client::{ClientCommand, UdpClient};

fn main() {
    let (rx, tx) = mpsc::channel();

    let mut client = UdpClient::new(
        "192.168.1.9:5021".parse().unwrap(),
        100_000_000.0,
        1200,
        Duration::from_secs(10),
        tx,
    ).unwrap();

    let server_thread = thread::spawn(move || 
    {
        client.run("192.168.1.7:5021".parse().unwrap())
    });
   let _x= rx.send(ClientCommand::Start).unwrap();
  

  let _=server_thread.join().unwrap();

}
