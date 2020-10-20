#[macro_use]
extern crate log;

use std::env;
use std::sync::Arc;

use tokio::io::{Error, ErrorKind, Result};
use tokio::net::UdpSocket;
use tokio::sync::Notify;

use crate::commons::{InitConfig, OptionConvert};

mod server;
mod client;
mod commons;

#[tokio::main]
async fn main() -> Result<()> {
  let mut args = env::args();
  args.next();

  let mode = args.next().option_to_res("CMD error")?;

  match mode.as_str() {
    "server" => server::start("0.0.0.0:12345",
                              "C:\\Users\\xutia\\Desktop\\rs-proxy-dic\\key\\cert.der",
                              "C:\\Users\\xutia\\Desktop\\rs-proxy-dic\\key\\priv_key").await?,
    "client" => {
      let init_config = InitConfig { protocol: 1, bind_port: 14444 };
      client::start("0.0.0.0:12344", "127.0.0.1:12345", "127.0.0.1:80",
                    "C:\\Users\\xutia\\Desktop\\rs-proxy-dic\\key\\cert.der", "proxy", init_config).await?;
    }
    _ => return Err(Error::new(ErrorKind::Other, "CMD error"))
  }
  Ok(())
}

