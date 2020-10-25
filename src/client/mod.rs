use std::net::{SocketAddr, ToSocketAddrs};

use bytes::Bytes;
use quinn::Endpoint;
use tokio::io::{AsyncWriteExt, Error, ErrorKind, Result};
use tokio::net::TcpStream;
use tokio::stream::StreamExt;
use tokio::sync::Notify;
use tokio::time;

use crate::commons::{InitConfig, OptionConvert, ProxyConfig, quic_config, StdResAutoConvert, StdResConvert};
use crate::commons;

pub async fn start(remote_addr: &str, cert_path: &str, server_name: &str, list: Vec<ProxyConfig>) -> Result<()> {
  let client_config = quic_config::configure_client(cert_path).await?;
  let mut builder = Endpoint::builder();

  builder.default_client_config(client_config);

  let bind_addr: SocketAddr = "0.0.0.0:0".parse().res_auto_convert()?;
  let (endpoint, _) = builder.bind(&bind_addr)
    .res_convert(|_| "client bind error".to_string())?;

  let remote_addr = remote_addr.to_socket_addrs()?.next().option_to_res("Address error")?;

  for proxy_config in list {
    let server_name = server_name.to_string();
    let endpoint = endpoint.clone();

    tokio::spawn(async move {
      let protocol = match proxy_config.protocol.as_str() {
        "tcp" => commons::TCP,
        "udp" => commons::UDP,
        _ => {
          error!("Proxy config error");
          return;
        }
      };

      let op = match tokio::net::lookup_host(proxy_config.proxy_addr).await {
        Ok(mut v) => v.next(),
        Err(e) => {
          error!("{}", e);
          return;
        }
      };

      let proxy_addr = match op {
        Some(v) => v,
        None => {
          error!("Bind address error");
          return;
        }
      };

      let init_config = InitConfig { protocol, bind_port: proxy_config.remote_port };

      loop {
        if let Err(e) = process(&endpoint, remote_addr, proxy_addr, &server_name, init_config).await {
          error!("{}", e);
        }
      }
    });
  }

  let notify = Notify::new();
  notify.notified().await;
  Ok(())
}

async fn process(endpoint: &Endpoint, remote_addr: SocketAddr,
                 proxy_addr: SocketAddr, server_name: &str, init_config: InitConfig) -> Result<()> {
  let mut conn = endpoint.connect(&remote_addr, server_name)
    .res_convert(|_| "Connection error".to_string())?.await?;

  info!("Connect {:?} success", remote_addr);

  let connection = conn.connection;
  let mut uni = connection.open_uni().await?;
  let init_config = serde_json::to_vec(&init_config)?;
  uni.write_u16(init_config.len() as u16).await?;
  uni.write_all(&init_config).await?;

  const HEART_BEAT: Bytes = Bytes::from_static(&[0u8; 1]);

  tokio::spawn(async move {
    let mut interval = time::interval(time::Duration::from_secs(3));

    loop {
      interval.tick().await;

      if let Err(e) = connection.send_datagram(HEART_BEAT) {
        error!("{}", e);
        return;
      }
    }
  });

  while let Some(res) = conn.bi_streams.next().await {
    let (mut quic_tx, mut quic_rx) = match res {
      Ok(v) => v,
      Err(_) => return Err(Error::new(ErrorKind::Other, "Remote close"))
    };

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

      tokio::select! {
        _ = f1 => (),
        _ = f2 => ()
      }
    });
  }
  Ok(())
}
