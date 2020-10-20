use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::Arc;

use quinn::{Connecting, Endpoint, EndpointBuilder, Connection};
use tokio::io::{Error, ErrorKind, Result, AsyncReadExt};
use tokio::net::TcpListener;
use tokio::stream::StreamExt;
use tokio::sync::Barrier;
use tokio::sync::Notify;

use crate::commons::{InitConfig, OptionConvert, quic_config, StdResAutoConvert, StdResConvert};
use crate::commons;
use quinn::crypto::rustls::TlsSession;

pub async fn start(addr: &str, cert_path: &str, priv_key_path: &str) -> Result<()> {
  let sever_config = quic_config::configure_server(cert_path, priv_key_path).await.res_auto_convert()?;
  let mut build = Endpoint::builder();
  build.listen(sever_config);

  let local_addr: SocketAddr = addr.to_socket_addrs()?.next().option_to_res("Address error")?;
  let (_, mut incoming) = build.bind(&local_addr)
    .res_convert(|_| "Quic server bind error".to_string())?;

  while let Some(conn) = incoming.next().await {
    tokio::spawn(async move {
      if let Err(e) = process(conn).await {
        error!("{}", e);
      }
    });
  };
  Ok(())
}

async fn process(conn: Connecting) -> Result<()> {
  let remote_addr = conn.remote_address();
  let init_error_msg = format!("{:?} init error", remote_addr);
  let config_error_msg = format!("{:?} config error", remote_addr);

  let mut socket = conn.await.res_convert(|_| "Connection error".to_string())?;
  let mut uni = socket.uni_streams;

  let init_config = match uni.next().await {
    Some(res) => {
      match res {
        Ok(mut recv_stream) => {
          let len = recv_stream.read_u16().await?;
          let mut buff = vec![0u8; len as usize];
          recv_stream.read_exact(&mut buff).await.res_convert(|_| "init error".to_string());
          let init_config: InitConfig = serde_json::from_slice(&buff).res_auto_convert()?;
          init_config
        }
        Err(_) => return Err(Error::new(ErrorKind::Other, init_error_msg))
      }
    }
    None => return Err(Error::new(ErrorKind::Other, init_error_msg))
  };

  let notify = Arc::new(Notify::new());

  match init_config.protocol {
    commons::TCP => tcp_server_handler(init_config.bind_port, notify.clone(), socket.connection),
    commons::UDP => {}
    _ => return Err(Error::new(ErrorKind::Other, config_error_msg))
  }

  let _ = uni.next().await;
  notify.notify();
  info!("{:?} disconnect", remote_addr);
  Ok(())
}

fn tcp_server_handler(bind_port: u16, notify: Arc<Notify>, quic_connection: Connection) {
  tokio::spawn(async move {
    let f1 = async move {
      let mut listener = match TcpListener::bind(("0.0.0.0", bind_port)).await {
        Ok(listener) => listener,
        Err(e) => {
          error!("{}", e);
          return;
        }
      };

      while let Ok((mut socket, _)) = listener.accept().await {
        let inner_connection = quic_connection.clone();

        tokio::spawn(async move {
          let (mut quic_tx, mut quic_rx) = match inner_connection.open_bi().await {
            Ok(socket) => socket,
            Err(e) => {
              error!("{}", e);
              return;
            }
          };

          let (mut tcp_rx, mut tcp_tx) = socket.split();

          let f1 = tokio::io::copy(&mut quic_rx, &mut tcp_tx);
          let f2 = tokio::io::copy(&mut tcp_rx, &mut quic_tx);

          tokio::select! {
             _ = f1 => (),
             _ = f2 => ()
          }
        });
      }
    };

    let f2 = async move {
      notify.notified().await
    };

    tokio::select! {
      _ = f1 => (),
      _ = f2 => ()
    }
  });
}