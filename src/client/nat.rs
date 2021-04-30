use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Instant;

use quinn::Connection;
use tokio::io::Result;
use tokio::net::UdpSocket;
use tokio::sync::mpsc::Sender;
use tokio::sync::RwLock;
use tokio::time::Duration;

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
  pub fn new(local_addr: SocketAddr, timeout: Duration, connection: Connection) -> Self {
    NatMapping {
      local_addr,
      map: Arc::new(RwLock::new(HashMap::new())),
      timeout,
      connection,
    }
  }

  pub async fn send(&self, remote_addr: SocketAddr, data: Vec<u8>) -> Result<()> {
    let guard = self.map.read().await;

    match guard.get(&remote_addr) {
      Some(tx) => tx.send(data).await,
      None => {
        drop(guard);

        let sock = ProxySocket::new(
          self.local_addr,
          remote_addr,
          self.connection.clone(),
          self.map.clone(),
          self.timeout,
        );

        sock.send(data).await?;
        self.map.write().await.insert(remote_addr, sock);
        Ok(())
      }
    }
  }

  pub async fn drop(&self) -> () {
    self.map.write().await.clear();
  }
}

impl ProxySocket {
  fn new(local_addr: SocketAddr,
         remote_addr: SocketAddr,
         connection: Connection,
         map: Arc<RwLock<HashMap<SocketAddr, ProxySocket>>>,
         timeout: Duration) -> Self {
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

      let latest_time = parking_lot::RwLock::new(Instant::now());

      let f1 = async {
        while let Some(packet) = rx.recv().await {
          *latest_time.write() = Instant::now();
          sock.send(&packet).await?;
        }
        Result::Ok(())
      };

      let f2 = async {
        let mut buff = [0u8; 65536];

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

      map.write().await.remove(&remote_addr);

      if let Err(e) = res {
        error!("{}", e)
      }
    });

    proxy_socket
  }

  async fn send(&self, data: Vec<u8>) -> Result<()> {
    self.tx.send(data).await.res_auto_convert()
  }
}
