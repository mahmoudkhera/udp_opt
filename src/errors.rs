use std::{io, net::AddrParseError, time::Duration};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum MyError {
    #[error("Failed to pind socket address: {0}")]
    BindFailed(io::Error),
    #[error("Udp socket failed to send data: {0}")]
    SendFailed(io::Error),

    #[error(" Udp socket failed to receive data: {0}")]
    RecvFailed(io::Error),
    #[error("Client faild to connect : {0}")]
    ConnectFailed(io::Error),

    #[error("Connection timed out after {0:?}")]
    Timeout(Duration),

    #[error("Invalid address: {0}")]
    InvalidAddress(#[from] AddrParseError),
    #[error("Get random for the test  faild ")]
    FailToGetRandom(io::Error),
}
