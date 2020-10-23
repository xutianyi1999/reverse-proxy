use quinn::{Certificate, CertificateChain, ClientConfig, ClientConfigBuilder, PrivateKey, ServerConfig, ServerConfigBuilder};
use tokio::fs;
use tokio::io::Result;

use crate::commons::StdResAutoConvert;

pub async fn configure_client(cert_path: &str) -> Result<ClientConfig> {
  let mut cfg_builder = ClientConfigBuilder::default();
  let cert_chain = fs::read(cert_path).await?;
  let cert_chain = CertificateChain::from_pem(&cert_chain).res_auto_convert()?;

  for cert in cert_chain {
    let cert = Certificate::from(cert);
    cfg_builder.add_certificate_authority(cert).res_auto_convert()?;
  }

  cfg_builder.enable_0rtt();
  Ok(cfg_builder.build())
}

pub async fn configure_server(cert_path: &str, priv_key_path: &str) -> Result<ServerConfig> {
  let cert_future = fs::read(cert_path);
  let priv_key_future = fs::read(priv_key_path);

  let (cert, priv_key) = tokio::try_join!(cert_future, priv_key_future)?;

  let cert = CertificateChain::from_pem(&cert).res_auto_convert()?;
  let priv_key = PrivateKey::from_pem(&priv_key).res_auto_convert()?;

  let mut cfg_builder = ServerConfigBuilder::default();
  cfg_builder.certificate(cert, priv_key).res_auto_convert()?;

  Ok(cfg_builder.build())
}
