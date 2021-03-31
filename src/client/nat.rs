use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Weak};
use std::time::{Instant, SystemTime};

use bytes::{BufMut, Bytes};
use parking_lot::RwLock;
use quinn::Connection;
use tokio::io::AsyncWriteExt;
use tokio::io::Result;
use tokio::net::{TcpStream, UdpSocket};
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::time::{Duration, sleep};

use crate::commons::{encode_msg, StdResConvert};

type Mapping = Arc<RwLock<HashMap<SocketAddr, Sender<Vec<u8>>>>>;

pub struct NatMapping {
  map: Mapping,
  timeout: Duration,
}

struct ProxySocket {
  tx: Sender<Vec<u8>>,
}

impl ProxySocket {
  fn new(local_addr: SocketAddr, remote_addr: SocketAddr, connection: Connection, map: Mapping, timeout: Duration) -> Self {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Vec<u8>>(50);
    let proxy_socket = ProxySocket { tx };

    tokio::spawn(async move {
      let sock = match UdpSocket::bind((IpAddr::from([0, 0, 0, 0]), 0)).await {
        Ok(sock) => sock,
        Err(e) => {
          error!("{}", e);
          return;
        }
      };

      if let Err(e) = sock.connect(local_addr).await {
        error!("{}", e);
        return;
      }

      let latest_time = RwLock::new(Instant::now());

      let f1 = async {
        while let Some(packet) = rx.recv().await {
          *latest_time.write() = Instant::now();
          sock.send(&packet).await?;
        }
        Result::Ok(())
      };

      let f2 = async {
        let mut buff = [0u8; 1500];

        while let Ok(len) = sock.recv(&mut buff).await {
          *latest_time.write() = Instant::now();
          let data = encode_msg(&buff[..len], remote_addr);
          connection.send_datagram(data).res_convert(|_| "Send datagram error".to_string())?;
        }
        Result::Ok(())
      };

      let f3 = async {
        loop {
          tokio::time::sleep(timeout).await;

          if latest_time.read().elapsed() >= timeout {
            return Result::Ok(());
          }
        }
      };

      let res = tokio::select! {
        res = f1 => res,
        res = f2 => res,
        res = f3 => res
      };

      if let Err(e) = res {
        error!("{}", e)
      }
    });

    proxy_socket
  }
}
