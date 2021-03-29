use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Weak};
use std::time::SystemTime;

use bytes::{BufMut, Bytes};
use parking_lot::RwLock;
use quinn::Connection;
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpStream, UdpSocket};
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::time::Duration;

type Mapping = Arc<RwLock<HashMap<SocketAddr, Sender<Vec<u8>>>>>;

pub struct NatMapping {
  map: Mapping,
  timeout: Duration,
}

struct ProxySocket {
  tx: Sender<Vec<u8>>,
}

impl ProxySocket {
  fn new(local_addr: SocketAddr, remote_addr: SocketAddr, connection: &Connection, map: Mapping, timeout: Duration) -> Self {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Vec<u8>>(50);
    let proxy_socket = ProxySocket { tx };

    tokio::spawn(async move {
      let sock = UdpSocket::bind((IpAddr::from([0, 0, 0, 0], 0))).await?;
      sock.connect(local_addr).await?;
      let mut latest_time = SystemTime::now();

      let f1 = async move {
        while let Some(packet) = rx.recv().await {
          sock.send(&packet).await?;
        }
      };

      let mut buff = [0u8; 1500];

      let f2 = async move {
        while let Ok(v) = sock.recv(&mut buff).await {}
      };
    });

    proxy_socket
  }
}
