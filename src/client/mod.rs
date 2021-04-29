use std::net::{SocketAddr, ToSocketAddrs};

use bytes::Bytes;
use futures::future::err;
use futures::StreamExt;
use quinn::{Connection, Datagrams, Endpoint, IncomingBiStreams};
use tokio::io::{AsyncWriteExt, Error, ErrorKind, Result};
use tokio::net::TcpStream;
use tokio::sync::Notify;
use tokio::time::{Duration, sleep};

use crate::client::nat::NatMapping;
use crate::commons::{decode_msg, HEARTBEAT, InitConfig, OptionConvert, ProxyConfig, quic_config, StdResAutoConvert, StdResConvert};
use crate::commons;

mod nat;

pub async fn start(server_addr: &str, cert_path: &str, server_name: &str, list: Vec<ProxyConfig>) -> Result<()> {
  let client_config = quic_config::configure_client(cert_path).await?;
  let mut builder = Endpoint::builder();

  builder.default_client_config(client_config);

  let bind_addr: SocketAddr = "0.0.0.0:0".parse().res_auto_convert()?;
  let (endpoint, _) = builder.bind(&bind_addr)
    .res_convert(|_| "client bind error".to_string())?;

  let server_addr = tokio::net::lookup_host(server_addr).await?.next().option_to_res("Address error")?;

  for proxy_config in list {
    let server_name = server_name.to_string();
    let endpoint = endpoint.clone();

    tokio::spawn(async move {
      let res = async move {
        let protocol = match proxy_config.protocol.as_str() {
          "tcp" => commons::TCP,
          "udp" => commons::UDP,
          _ => return Err(Error::new(ErrorKind::Other, "Proxy config error"))
        };

        let init_config = InitConfig { protocol, bind_port: proxy_config.remote_port };
        let proxy_addr: SocketAddr = proxy_config.proxy_addr.parse().res_auto_convert()?;

        loop {
          if let Err(e) = process(&endpoint, server_addr, proxy_addr, &server_name, init_config).await {
            error!("{}", e);
          }
        }
        Ok(())
      };

      if let Err(e) = res.await {
        error!("{}", e)
      }
    });
  }

  Notify::new().notified().await;
  Ok(())
}

async fn process(endpoint: &Endpoint, server_addr: SocketAddr,
                 proxy_addr: SocketAddr, server_name: &str, init_config: InitConfig) -> Result<()> {
  let conn = endpoint.connect(&server_addr, server_name)
    .res_convert(|_| "Connection error".to_string())?.await?;

  info!("Connect {:?} success", server_addr);

  let connection = conn.connection;
  let mut bi_streams = conn.bi_streams;
  let mut datagrams = conn.datagrams;

  let mut uni = connection.open_uni().await?;
  let init_config_data = serde_json::to_vec(&init_config)?;
  uni.write_u16(init_config_data.len() as u16).await?;
  uni.write_all(&init_config_data).await?;

  let f1 = async move {
    match init_config.protocol {
      commons::UDP => udp_handler(connection, datagrams, proxy_addr).await,
      commons::TCP => tcp_handler(connection, bi_streams, proxy_addr).await,
      _ => return Err(Error::new(ErrorKind::Other, format!("{:?} config error", server_addr)))
    }
  };

  let f2 = async move {
    loop {
      uni.write_u8(HEARTBEAT).await?;
    }
  };

  tokio::select! {
    res = f1 => res,
    res = f2 => res
  }
}

async fn udp_handler(connection: Connection, mut datagrams: Datagrams, proxy_addr: SocketAddr) -> Result<()> {
  let timeout = Duration::from_secs(300);
  let nat = NatMapping::new(proxy_addr, timeout, connection);

  let res = async {
    while let Some(res) = datagrams.next().await {
      let packet = res?;
      let (data, remote_addr) = decode_msg(packet)?;
      nat.send(remote_addr, data.to_vec()).await?;
    }
    Result::Ok(())
  };

  if let Err(e) = res.await {
    error!("{}", e)
  }

  nat.drop().await;
  Ok(())
}

async fn tcp_handler(connection: Connection, mut bi_streams: IncomingBiStreams, proxy_addr: SocketAddr) -> Result<()> {
  while let Some(res) = bi_streams.next().await {
    let (mut quic_tx, mut quic_rx) = res?;

    tokio::spawn(async move {
      let mut local_socket = match TcpStream::connect(proxy_addr).await {
        Ok(v) => v,
        Err(e) => {
          error!("{}", e);
          return;
        }
      };

      let (mut local_rx, mut local_tx) = local_socket.split();

      let f1 = tokio::io::copy(&mut local_rx, &mut quic_tx);
      let f2 = tokio::io::copy(&mut quic_rx, &mut local_tx);

      let res = tokio::select! {
          res = f1 => res,
          res = f2 => res
      };

      if let Err(e) = res {
        error!("{:?}", e)
      }
    });
  }
  Ok(())
}
