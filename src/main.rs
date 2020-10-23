#[macro_use]
extern crate log;

use std::env;

use log4rs::append::console::ConsoleAppender;
use log4rs::config::{Appender, Config, Root};
use log4rs::encode::pattern::PatternEncoder;
use log::LevelFilter;
use tokio::fs;
use tokio::io::{Error, ErrorKind, Result};

use crate::commons::{ClientConfig, OptionConvert, ServerConfig, StdResAutoConvert};

mod server;
mod client;
mod commons;

#[tokio::main]
async fn main() -> Result<()> {
  logger_init()?;
  let mut args = env::args();
  args.next();

  let mode = args.next().option_to_res("Params error")?;
  let config_path = args.next().option_to_res("Params error")?;
  let config = fs::read(config_path).await?;

  match mode.as_str() {
    "client" => {
      let client_config: ClientConfig = serde_json::from_slice(&config).res_auto_convert()?;
      client::start(&client_config.server_addr,
                    &client_config.cert_path,
                    &client_config.server_name,
                    client_config.proxy).await
    }
    "server" => {
      let server_config: ServerConfig = serde_json::from_slice(&config).res_auto_convert()?;
      server::start(&server_config.bind_addr, &server_config.cert_path, &server_config.priv_key_path).await
    }
    _ => return Err(Error::new(ErrorKind::Other, "CMD error"))
  }
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
