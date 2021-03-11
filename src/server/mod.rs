use std::sync::Arc;

use futures::StreamExt;
use quinn::{Connecting, Connection, Endpoint};
use tokio::io::{AsyncReadExt, Error, ErrorKind, Result};
use tokio::net::TcpListener;
use tokio::sync::Notify;

use crate::commons::{InitConfig, OptionConvert, quic_config, StdResAutoConvert, StdResConvert};
use crate::commons;

pub async fn start(addr: &str, cert_path: &str, priv_key_path: &str) -> Result<()> {
  let sever_config = quic_config::configure_server(cert_path, priv_key_path).await.res_auto_convert()?;
  let mut build = Endpoint::builder();
  build.listen(sever_config);

  let bind_addr = tokio::net::lookup_host(addr).await?.next().option_to_res("Bind address error")?;
  let (_, mut incoming) = build.bind(&bind_addr)
    .res_convert(|_| "Quic server bind error".to_string())?;

  info!("Bind {:?}", bind_addr);

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

  let socket = conn.await.res_convert(|_| format!("{:?} connection error", remote_addr))?;
  let mut uni = socket.uni_streams;

  let (init_config, mut rx) = match uni.next().await {
    Some(res) => {
      match res {
        Ok(mut recv_stream) => {
          let len = recv_stream.read_u16().await?;
          let mut buff = vec![0u8; len as usize];
          let _ = recv_stream.read_exact(&mut buff).await.res_convert(|_| init_error_msg.clone());
          let init_config: InitConfig = serde_json::from_slice(&buff).res_auto_convert()?;
          (init_config, recv_stream)
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
    _ => return Err(Error::new(ErrorKind::Other, format!("{:?} config error", remote_addr)))
  }

  let _ = rx.read(&mut [0u8; 0]).await;
  notify.notify_waiters();
  info!("{:?} disconnect", remote_addr);
  Ok(())
}

fn tcp_server_handler(bind_port: u16, notify: Arc<Notify>, quic_connection: Connection) {
  tokio::spawn(async move {
    let f1 = async move {
      let listener = match TcpListener::bind(("0.0.0.0", bind_port)).await {
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
              error!("{:?}", e);
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
