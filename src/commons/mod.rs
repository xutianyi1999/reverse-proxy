use tokio::io::{Result, Error, ErrorKind};
use serde::{Deserialize, Serialize};

pub mod quic_config;

pub trait OptionConvert<T> {
  fn option_to_res(self, msg: &str) -> Result<T>;
}

impl<T> OptionConvert<T> for Option<T> {
  fn option_to_res(self, msg: &str) -> Result<T> {
    option_convert(self, msg)
  }
}

pub trait StdResConvert<T, E> {
  fn res_convert(self, f: fn(E) -> String) -> Result<T>;
}

impl<T, E> StdResConvert<T, E> for std::result::Result<T, E> {
  fn res_convert(self, f: fn(E) -> String) -> Result<T> {
    std_res_convert(self, f)
  }
}

pub trait StdResAutoConvert<T, E: ToString> {
  fn res_auto_convert(self) -> Result<T>;
}

impl<T, E: ToString> StdResAutoConvert<T, E> for std::result::Result<T, E> {
  fn res_auto_convert(self) -> Result<T> {
    std_res_convert(self, |e| e.to_string())
  }
}

fn option_convert<T>(o: Option<T>, msg: &str) -> Result<T> {
  match o {
    Some(v) => Ok(v),
    None => Err(Error::new(ErrorKind::Other, msg))
  }
}

fn std_res_convert<T, E>(res: std::result::Result<T, E>, f: fn(E) -> String) -> Result<T> {
  match res {
    Ok(v) => Ok(v),
    Err(e) => {
      let msg = f(e);
      Err(Error::new(ErrorKind::Other, msg))
    }
  }
}

pub const TCP: u8 = 1;
pub const UDP: u8 = 2;

#[derive(Serialize, Deserialize)]
#[derive(Copy, Clone)]
pub struct InitConfig {
  pub protocol: u8,
  pub bind_port: u16,
}
