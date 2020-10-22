#[macro_use]
extern crate log;

use std::env;
use std::sync::Arc;

use tokio::io::{Error, ErrorKind, Result};
use tokio::net::UdpSocket;
use tokio::sync::Notify;

use crate::commons::{InitConfig, OptionConvert, StdResAutoConvert};
use log4rs::append::console::ConsoleAppender;
use log4rs::encode::pattern::PatternEncoder;
use log4rs::config::{Config, Appender, Root};
use log::LevelFilter;

mod server;
mod client;
mod commons;

#[tokio::main]
async fn main() -> Result<()> {
  logger_init()?;
  let mut args = env::args();
  args.next();

  let mode = args.next().option_to_res("CMD error")?;

  match mode.as_str() {
    "server" => server::start("0.0.0.0:12345",
                              "C:\\Users\\xutia\\Desktop\\rs-proxy-dic\\key\\cert.der",
                              "C:\\Users\\xutia\\Desktop\\rs-proxy-dic\\key\\priv_key").await?,
    "client" => {
      let init_config = InitConfig { protocol: 1, bind_port: 12333 };
      client::start("0.0.0.0:12344", "127.0.0.1:12345", "127.0.0.1:18888",
                    "C:\\Users\\xutia\\Desktop\\rs-proxy-dic\\key\\cert.der", "proxy", init_config).await?;
    }
    _ => return Err(Error::new(ErrorKind::Other, "CMD error"))
  }
  Ok(())
}

fn logger_init() -> Result<()> {
  let stdout = ConsoleAppender::builder()
    .encoder(Box::new(PatternEncoder::new("[Console] {d} - {l} -{t} - {m}{n}")))
    .build();

  let config = Config::builder()
    .appender(Appender::builder().build("stdout", Box::new(stdout)))
    .build(Root::builder().appender("stdout").build(LevelFilter::Info))
    .res_auto_convert()?;

  log4rs::init_config(config).res_auto_convert()?;
  Ok(())
}
