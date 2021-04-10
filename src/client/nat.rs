use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Weak};
use std::time::{Instant, SystemTime};

use bytes::{BufMut, Bytes};
use parking_lot::RwLock;
use quinn::Connection;
use tokio::io::AsyncWriteExt;
use tokio::io::Result;
use tokio::net::{TcpSocket, TcpStream, UdpSocket};
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::time::{Duration, sleep};

use crate::commons::{encode_msg, StdResAutoConvert, StdResConvert};

type Mapping = Arc<RwLock<HashMap<SocketAddr, ProxySocket>>>;

pub struct NatMapping {
  local_addr: SocketAddr,
  map: Mapping,
  timeout: Duration,
  connection: Connection,
}

struct ProxySocket {
  tx: Sender<Vec<u8>>,
}

impl NatMapping {
  fn new(local_addr: SocketAddr, timeout: Duration, connection: Connection) -> Self {
    NatMapping {
      local_addr,
      map: Arc::new(RwLock::new(HashMap::new())),
      timeout,
      connection,
    }
  }

  fn send(&self, remote_addr: SocketAddr, data: Vec<u8>) -> Result<()> {
    let guard = self.map.read();

    match guard.get(&remote_addr) {
      Some(tx) => {
        tx.send(data)
      }
      None => {
        drop(guard);

        let sock = ProxySocket::new(
          self.local_addr,
          remote_addr,
          self.connection.clone(),
          self.map.clone(),
          self.timeout,
        );

        let res = sock.send(data);
        self.map.write().insert(remote_addr, sock);
        res
      }
    }
  }
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
            map.write().remove(&remote_addr);
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

  fn send(&self, data: Vec<u8>) -> Result<()> {
    self.tx.try_send(data).res_auto_convert()
  }
}
