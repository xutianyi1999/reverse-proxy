# reverse-proxy
内网穿透

## Config
### server

```json
{
  "bind_addr": "0.0.0.0:12345",
  "cert_path": "./cfg-example/cert.pem",
  "priv_key_path": "./cfg-example/priv.key"
}
```
- bind_addr: 服务端监听端口
- cert_path: PEM编码证书路径
- priv_key_path: 私钥路径

### client

```json
{
  "server_addr": "0.0.0.0:12345",
  "cert_path": "./cfg-example/cert.pem",
  "server_name": "proxy",
  "proxy": [
    {
      "protocol": "tcp",
      "proxy_addr": "127.0.0.1:30000",
      "remote_port": 18888
    }
  ]
}
```
- server_addr: 服务器地址
- cert_path: PEM编码证书路径
- server_name: subjectAltName
- proxy: 本地需代理的服务地址
  - protocol: 代理协议(TCP/UDP)
  - proxy_addr: 本地服务地址
  - remote_port: 映射在远端的端口

## Usage
### server
```shell script
./reverse-proxy server server-config.json
```

### client
```shell script
./reverse-proxy client client-config.json
```
## Build project
```shell script
cargo build --release
```
