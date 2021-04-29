use std::borrow::Borrow;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::ops::Deref;

use bytes::{Buf, BufMut, BytesMut};
use futures::StreamExt;
use quinn::{Connecting, Connection, Datagrams, Endpoint};
use tokio::io::{AsyncReadExt, Error, ErrorKind, Result};
use tokio::net::{TcpListener, UdpSocket};

use crate::commons::{decode_msg, encode_msg, HEARTBEAT, InitConfig, IPV4, IPV6, OptionConvert, quic_config, StdResAutoConvert, StdResConvert};
use crate::commons;

pub async fn start(addr: &str, cert_path: &str, priv_key_path: &str) -> Result<()> {
  let sever_config = quic_config::configure_server(cert_path, priv_key_path).await?;
  let mut build = Endpoint::builder();
  build.listen(sever_config);

  let (_, mut incoming) = build.bind(&addr.parse().res_auto_convert()?)
    .res_convert(|_| "Quic server bind error".to_string())?;

  info!("Bind {}", addr);

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

  let socket = conn.await?;
  info!("{:?} connected", remote_addr);

  let mut uni = socket.uni_streams;
  let connection = socket.connection;
  let mut datagrams = socket.datagrams;

  let mut recv = uni.next().await.option_to_res(&init_error_msg)??;

  let len = recv.read_u16().await?;
  let mut buff = vec![0u8; len as usize];
  recv.read_exact(&mut buff).await.res_convert(|_| init_error_msg.clone())?;
  let init_config: InitConfig = serde_json::from_slice(&buff)?;

  let f1 = async move {
    match init_config.protocol {
      commons::UDP => udp_server_handler(init_config.bind_port, connection, datagrams).await,
      commons::TCP => tcp_server_handler(init_config.bind_port, connection).await,
      _ => return Err(Error::new(ErrorKind::Other, format!("{:?} config error", remote_addr)))
    }
  };

  let f2 = async move {
    while let Ok(packet) = recv.read_u8().await {
      if packet != HEARTBEAT {
        break;
      }
    }
    Ok(())
  };

  let res = tokio::select! {
     res = f1 => res,
     res = f2 => res
  };

  info!("{:?} disconnect", remote_addr);
  res
}

async fn udp_server_handler(bind_port: u16, quic_connection: Connection, mut datagram: Datagrams) -> Result<()> {
  let socket = UdpSocket::bind(("0.0.0.0", bind_port)).await?;

  let f1 = async {
    let mut buff = [0u8; 65536];

    while let Ok((len, dest_addr)) = socket.recv_from(&mut buff).await {
      let data_slice = &buff[..len];
      let data = encode_msg(data_slice, dest_addr);
      quic_connection.send_datagram(data).res_convert(|_| "Send udp packet error".to_string())?;
    };
    Ok(())
  };

  let f2 = async {
    while let Some(res) = datagram.next().await {
      let mut packet = res?;
      let (data, dest) = decode_msg(packet)?;
      socket.send_to(&data, dest).await?;
    }
    Ok(())
  };

  tokio::select! {
     res = f1 => res,
     res = f2 => res
  }
}

async fn tcp_server_handler(bind_port: u16, quic_connection: Connection) -> Result<()> {
  let listener = TcpListener::bind(("0.0.0.0", bind_port)).await?;

  while let Ok((mut socket, _)) = listener.accept().await {
    let inner_connection = quic_connection.clone();

    tokio::spawn(async move {
      let res = async move {
        let (mut quic_tx, mut quic_rx) = inner_connection.open_bi().await?;
        let (mut tcp_rx, mut tcp_tx) = socket.split();

        let f1 = tokio::io::copy(&mut quic_rx, &mut tcp_tx);
        let f2 = tokio::io::copy(&mut tcp_rx, &mut quic_tx);

        tokio::select! {
          res = f1 => res,
          res = f2 => res
        }
      };

      if let Err(e) = res.await {
        error!("{}", e)
      }
    });
  };
  Ok(())
}
