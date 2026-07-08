use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::Context;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::OwnedWriteHalf;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Mutex, mpsc};

use crate::camouflage::{
    self, build_client_login_response, build_handshake, build_server_login_sequence,
};
use crate::geoip::GeoRouter;
use crate::gfw::host_from_dest;
use crate::routing::ProxyPolicy;
use crate::speed::{self, SpeedMode, IO_BUF};
use crate::traffic::TrafficCounter;
use crate::tunnel::{
    spawn_client_keepalive, spawn_server_noise, TunnelReader, TunnelWriter,
    TunnelWriterHandle,
};

const MSG_OPEN: u8 = 0x01;
const MSG_CLOSE: u8 = 0x02;
const MSG_DATA: u8 = 0x03;
const MSG_OPEN_OK: u8 = 0x00;
const MSG_OPEN_FAIL: u8 = 0x05;

fn encode_msg(msg_type: u8, conn_id: u32, payload: &[u8]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(5 + payload.len());
    buf.push(msg_type);
    buf.extend_from_slice(&conn_id.to_be_bytes());
    buf.extend_from_slice(payload);
    buf
}

fn decode_msg(data: &[u8]) -> anyhow::Result<(u8, u32, &[u8])> {
    if data.len() < 5 {
        anyhow::bail!("消息过短");
    }
    let msg_type = data[0];
    let conn_id = u32::from_be_bytes([data[1], data[2], data[3], data[4]]);
    Ok((msg_type, conn_id, &data[5..]))
}

/// 共享隧道多路复用器（客户端）— 每连接独立队列，避免并发死锁
pub struct TunnelMux {
    writer: TunnelWriterHandle,
    routes: Arc<Mutex<HashMap<u32, mpsc::UnboundedSender<Vec<u8>>>>>,
    next_id: std::sync::atomic::AtomicU32,
    alive: Arc<AtomicBool>,
}

impl TunnelMux {
    pub fn new(writer: TunnelWriter, reader: TunnelReader) -> Self {
        Self::from_shared(TunnelWriterHandle::spawn(writer), reader)
    }

    pub fn from_shared(writer: TunnelWriterHandle, reader: TunnelReader) -> Self {
        let alive = writer.alive_flag();
        let routes: Arc<Mutex<HashMap<u32, mpsc::UnboundedSender<Vec<u8>>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let routes_dispatch = routes.clone();
        let writer_for_dispatch = writer.clone();
        let reader = Arc::new(Mutex::new(reader));
        let alive_watch = alive.clone();

        tokio::spawn(async move {
            loop {
                let data = {
                    let mut r = reader.lock().await;
                    r.recv_with_response(&writer_for_dispatch).await
                };
                match data {
                    Ok(d) if d.is_empty() => {
                        tracing::warn!("隧道 TCP 已关闭");
                        break;
                    }
                    Ok(d) => {
                        if let Ok((msg_type, conn_id, payload)) = decode_msg(&d) {
                            let mut out = vec![msg_type];
                            out.extend_from_slice(payload);
                            let routes = routes_dispatch.lock().await;
                            if let Some(tx) = routes.get(&conn_id) {
                                let _ = tx.send(out);
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!("隧道读取失败: {e:#}");
                        break;
                    }
                }
            }
            alive_watch.store(false, Ordering::Release);
        });

        Self {
            writer,
            routes,
            next_id: std::sync::atomic::AtomicU32::new(1),
            alive,
        }
    }

    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::Acquire)
    }

    pub fn alive_flag(&self) -> Arc<AtomicBool> {
        self.alive.clone()
    }

    pub fn alloc_id(&self) -> u32 {
        self.next_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }

    pub async fn register(&self, conn_id: u32) -> mpsc::UnboundedReceiver<Vec<u8>> {
        let (tx, rx) = mpsc::unbounded_channel();
        self.routes.lock().await.insert(conn_id, tx);
        rx
    }

    pub async fn unregister(&self, conn_id: u32) {
        self.routes.lock().await.remove(&conn_id);
    }

    pub async fn send(&self, msg_type: u8, conn_id: u32, payload: &[u8]) -> anyhow::Result<()> {
        if !self.is_alive() {
            anyhow::bail!("隧道已断开");
        }
        let data = encode_msg(msg_type, conn_id, payload);
        self.writer.send(&data).await
    }
}

async fn route_recv(rx: &mut mpsc::UnboundedReceiver<Vec<u8>>) -> anyhow::Result<Vec<u8>> {
    rx.recv()
        .await
        .ok_or_else(|| anyhow::anyhow!("隧道连接已关闭"))
}

async fn tunnel_connect(
    mux: &Arc<TunnelMux>,
    dest: &str,
) -> anyhow::Result<(u32, mpsc::UnboundedReceiver<Vec<u8>>)> {
    let conn_id = mux.alloc_id();
    let mut rx = mux.register(conn_id).await;
    mux.send(MSG_OPEN, conn_id, dest.as_bytes()).await?;
    let resp = route_recv(&mut rx).await?;
    if resp.first() != Some(&MSG_OPEN_OK) {
        mux.unregister(conn_id).await;
        anyhow::bail!("隧道连接 {dest} 失败");
    }
    Ok((conn_id, rx))
}

pub async fn run_http_proxy(
    listen: &str,
    mux: Arc<TunnelMux>,
    policy: Arc<ProxyPolicy>,
    geo: Arc<GeoRouter>,
    traffic: Arc<TrafficCounter>,
) -> anyhow::Result<()> {
    let listener = TcpListener::bind(listen)
        .await
        .with_context(|| format!("绑定 HTTP 代理端口失败: {listen}"))?;
    tracing::info!("HTTP 代理监听 {listen}（Chrome/系统代理用此端口）");

    loop {
        let (client, addr) = listener.accept().await?;
        tracing::debug!("HTTP 代理连接来自 {addr}");
        let m = mux.clone();
        let p = policy.clone();
        let g = geo.clone();
        let t = traffic.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_http(client, m, p, g, t).await {
                tracing::debug!("HTTP 代理会话结束: {e}");
            }
        });
    }
}

async fn handle_http(
    mut client: TcpStream,
    mux: Arc<TunnelMux>,
    policy: Arc<ProxyPolicy>,
    geo: Arc<GeoRouter>,
    traffic: Arc<TrafficCounter>,
) -> anyhow::Result<()> {
    let _ = client.set_nodelay(true);
    let result = handle_http_inner(&mut client, &mux, &policy, &geo, &traffic).await;
    let _ = client.shutdown().await;
    result
}

async fn handle_http_inner(
    client: &mut TcpStream,
    mux: &Arc<TunnelMux>,
    policy: &ProxyPolicy,
    geo: &GeoRouter,
    traffic: &Arc<TrafficCounter>,
) -> anyhow::Result<()> {
    let mut buf = Vec::with_capacity(4096);
    let mut tmp = [0u8; 1];
    loop {
        client.read_exact(&mut tmp).await?;
        buf.push(tmp[0]);
        if buf.len() >= 4 && &buf[buf.len() - 4..] == b"\r\n\r\n" {
            break;
        }
        if buf.len() > 65536 {
            anyhow::bail!("HTTP 请求头过长");
        }
    }

    let headers = std::str::from_utf8(&buf)?;
    let first = headers.lines().next().unwrap_or("");
    let parts: Vec<&str> = first.split_whitespace().collect();

    if parts.first() == Some(&"CONNECT") {
        let dest = parts.get(1).copied().unwrap_or("");
        if dest.is_empty() {
            anyhow::bail!("无效 CONNECT 目标");
        }
        let host = host_from_dest(dest);
        if geo.should_proxy(host, policy).await {
            tracing::debug!("HTTP CONNECT [代理] -> {dest}");
            let (conn_id, rx) = tunnel_connect(mux, dest).await?;
            client
                .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
                .await?;
            let r = relay_bidirectional(client, mux.clone(), conn_id, rx, traffic.clone()).await;
            mux.unregister(conn_id).await;
            return r;
        }
        tracing::debug!("HTTP CONNECT [直连] -> {dest}");
        client
            .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
            .await?;
        return relay_direct_bidirectional(client, dest, traffic.clone()).await;
    }

    client
        .write_all(b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\n\r\n")
        .await?;
    Ok(())
}

pub async fn run_socks5_proxy(
    listen: &str,
    mux: Arc<TunnelMux>,
    policy: Arc<ProxyPolicy>,
    geo: Arc<GeoRouter>,
    traffic: Arc<TrafficCounter>,
) -> anyhow::Result<()> {
    let listener = TcpListener::bind(listen)
        .await
        .with_context(|| format!("绑定 SOCKS5 端口失败: {listen}"))?;
    tracing::info!("SOCKS5 代理监听 {listen}");

    loop {
        let (client, addr) = listener.accept().await?;
        tracing::debug!("SOCKS5 连接来自 {addr}");
        let m = mux.clone();
        let p = policy.clone();
        let g = geo.clone();
        let t = traffic.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_socks5(client, m, p, g, t).await {
                tracing::debug!("SOCKS5 会话结束: {e}");
            }
        });
    }
}

async fn handle_socks5(
    mut client: TcpStream,
    mux: Arc<TunnelMux>,
    policy: Arc<ProxyPolicy>,
    geo: Arc<GeoRouter>,
    traffic: Arc<TrafficCounter>,
) -> anyhow::Result<()> {
    let _ = client.set_nodelay(true);
    let result = handle_socks5_inner(&mut client, mux.clone(), policy, geo, traffic).await;
    let _ = client.shutdown().await;
    result
}

async fn handle_socks5_inner(
    client: &mut TcpStream,
    mux: Arc<TunnelMux>,
    policy: Arc<ProxyPolicy>,
    geo: Arc<GeoRouter>,
    traffic: Arc<TrafficCounter>,
) -> anyhow::Result<()> {
    let mut header = [0u8; 2];
    client.read_exact(&mut header).await?;
    if header[0] != 0x05 {
        anyhow::bail!("非 SOCKS5");
    }
    let mut methods = vec![0u8; header[1] as usize];
    client.read_exact(&mut methods).await?;
    client.write_all(&[0x05, 0x00]).await?;

    let mut req = [0u8; 4];
    client.read_exact(&mut req).await?;
    if req[1] != 0x01 {
        anyhow::bail!("仅支持 CONNECT");
    }

    let target = match req[3] {
        0x01 => {
            let mut ip = [0u8; 4];
            client.read_exact(&mut ip).await?;
            format!("{}.{}.{}.{}", ip[0], ip[1], ip[2], ip[3])
        }
        0x03 => {
            let mut len = [0u8; 1];
            client.read_exact(&mut len).await?;
            let mut domain = vec![0u8; len[0] as usize];
            client.read_exact(&mut domain).await?;
            String::from_utf8(domain)?
        }
        _ => anyhow::bail!("不支持的地址类型"),
    };

    let mut port_buf = [0u8; 2];
    client.read_exact(&mut port_buf).await?;
    let port = u16::from_be_bytes(port_buf);
    let dest = format!("{target}:{port}");
    let host = host_from_dest(&dest);

    if geo.should_proxy(host, &policy).await {
        tracing::debug!("SOCKS5 [代理] -> {dest}");
        let (conn_id, rx) = tunnel_connect(&mux, &dest).await?;
        client
            .write_all(&[0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
            .await?;
        let r = relay_bidirectional(client, mux.clone(), conn_id, rx, traffic.clone()).await;
        mux.unregister(conn_id).await;
        return r;
    }

    tracing::debug!("SOCKS5 [直连] -> {dest}");
    client
        .write_all(&[0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
        .await?;
    relay_direct_bidirectional(client, &dest, traffic).await
}

async fn relay_direct_bidirectional(
    client: &mut TcpStream,
    dest: &str,
    traffic: Arc<TrafficCounter>,
) -> anyhow::Result<()> {
    let remote = TcpStream::connect(dest)
        .await
        .with_context(|| format!("直连 {dest} 失败"))?;
    remote.set_nodelay(true)?;
    let _ = speed::tune_tcp(&remote);
    let (mut cr, mut cw) = client.split();
    let (mut rr, mut rw) = remote.into_split();
    let traffic_up = traffic.clone();
    let traffic_down = traffic;

    let up = async move {
        let mut buf = vec![0u8; IO_BUF];
        loop {
            let n = cr.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            traffic_up.add_up(n);
            rw.write_all(&buf[..n]).await?;
        }
        Ok::<(), anyhow::Error>(())
    };

    let down = async move {
        let mut buf = vec![0u8; IO_BUF];
        loop {
            let n = rr.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            traffic_down.add_down(n);
            cw.write_all(&buf[..n]).await?;
        }
        Ok::<(), anyhow::Error>(())
    };

    tokio::select! {
        r = up => r?,
        r = down => r?,
    }
    Ok(())
}

async fn relay_upload(
    mut cr: tokio::net::tcp::ReadHalf<'_>,
    mux: Arc<TunnelMux>,
    conn_id: u32,
    traffic: Arc<TrafficCounter>,
) -> anyhow::Result<()> {
    use std::io::ErrorKind;
    let mut buf = vec![0u8; IO_BUF];
    loop {
        let mut end = match cr.read(&mut buf).await {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) => return Err(e.into()),
        };
        while end < IO_BUF * 9 / 10 {
            match cr.try_read(&mut buf[end..]) {
                Ok(0) => break,
                Ok(n2) => end += n2,
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(e) => return Err(e.into()),
            }
        }
        traffic.add_up(end);
        mux.send(MSG_DATA, conn_id, &buf[..end]).await?;
    }
    let _ = mux.send(MSG_CLOSE, conn_id, &[]).await;
    Ok(())
}

async fn relay_download(
    mut cw: tokio::net::tcp::WriteHalf<'_>,
    mut route_rx: mpsc::UnboundedReceiver<Vec<u8>>,
    traffic: Arc<TrafficCounter>,
) -> anyhow::Result<()> {
    loop {
        let msg = route_recv(&mut route_rx).await?;
        match msg[0] {
            MSG_DATA => {
                let payload = &msg[1..];
                traffic.add_down(payload.len());
                cw.write_all(payload).await?;
            }
            MSG_CLOSE => break,
            _ => {}
        }
    }
    Ok(())
}

async fn relay_bidirectional(
    client: &mut TcpStream,
    mux: Arc<TunnelMux>,
    conn_id: u32,
    route_rx: mpsc::UnboundedReceiver<Vec<u8>>,
    traffic: Arc<TrafficCounter>,
) -> anyhow::Result<()> {
    let (cr, cw) = client.split();
    let mux_up = mux.clone();
    let traffic_up = traffic.clone();
    let traffic_down = traffic;
    tokio::try_join!(
        relay_upload(cr, mux_up, conn_id, traffic_up),
        relay_download(cw, route_rx, traffic_down),
    )?;
    Ok(())
}

pub async fn connect_tunnel(
    remote: &str,
    speed: SpeedMode,
    tcp_target: Option<&str>,
) -> anyhow::Result<TunnelMux> {
    let connect_addr = tcp_target.unwrap_or(remote);
    let stream = TcpStream::connect(connect_addr)
        .await
        .with_context(|| format!("连接 {connect_addr} 失败"))?;
    speed::tune_tcp(&stream)?;

    let (host, port) = parse_host_port(remote, 25565);
    let handshake = build_handshake(&host, port);
    if tcp_target.is_some() {
        tracing::info!(
            "TCP 经 javaw 中继 {connect_addr} → {host}:{port}（MC 握手伪装目标不变）"
        );
    }

    let (reader, writer) = stream.into_split();
    let mut writer = TunnelWriter::new(writer, speed);
    let mut reader = TunnelReader::new(reader);

    writer.write_raw(&handshake).await?;
    tracing::info!("已发送 MC 握手 → {host}:{port}");

    let mut got_login = false;
    for _ in 0..8 {
        let pkt = reader
            .read_raw_packet()
            .await
            .context("读取服务端 MC 响应失败")?;
        if let Ok((id, _)) = camouflage::read_varint(&pkt) {
            if id == camouflage::LOGIN_SUCCESS_ID {
                got_login = true;
            }
        }
        if let Some(ka_id) = camouflage::parse_server_keepalive(&pkt) {
            let mut sim = crate::gameplay::GameplaySimulator::new();
            let resp = sim.handle_server_keepalive(ka_id);
            writer.write_raw(&resp).await?;
        }
        if got_login {
            break;
        }
    }

    if !got_login {
        anyhow::bail!("服务端未返回 Login Success");
    }

    writer.write_raw(&build_client_login_response()).await?;
    tracing::info!("MC 会话建立完成（模式: {:?}）", speed);

    let handle = TunnelWriterHandle::spawn(writer);
    spawn_client_keepalive(handle.clone());
    TunnelWriter::start_background_noise(handle.clone(), speed);

    Ok(TunnelMux::from_shared(handle, reader))
}

pub async fn accept_tunnel(
    listener: &TcpListener,
    speed: SpeedMode,
) -> anyhow::Result<(TunnelWriter, TunnelReader)> {
    let (stream, addr) = listener.accept().await?;
    accept_tunnel_stream(stream, addr, speed).await
}

/// 单连接握手（在独立 task 中调用，避免阻塞 accept 循环）
pub async fn accept_tunnel_stream(
    stream: TcpStream,
    addr: SocketAddr,
    speed: SpeedMode,
) -> anyhow::Result<(TunnelWriter, TunnelReader)> {
    tracing::info!("新连接来自 {addr}");
    speed::tune_tcp(&stream)?;

    let (reader, writer) = stream.into_split();
    let mut reader = TunnelReader::new(reader);
    let mut writer = TunnelWriter::new(writer, speed);

    let _ = reader.read_raw_packet().await?; // Handshake
    let _ = reader.read_raw_packet().await?; // Login Start

    writer.write_raw(&build_server_login_sequence()).await?;
    tracing::info!("已发送完整 MC 登录序列 ({addr})");

    // 消费客户端 brand + 位置确认
    let _ = reader.read_raw_packet().await.ok();
    let _ = reader.read_raw_packet().await.ok();

    tracing::info!("MC 伪装握手完成 ({addr})");
    Ok((writer, reader))
}

/// 服务端多路复用会话
pub async fn server_mux_session(
    mut reader: TunnelReader,
    writer: TunnelWriterHandle,
    speed: SpeedMode,
) -> anyhow::Result<()> {
    let mut writers: HashMap<u32, OwnedWriteHalf> = HashMap::new();
    spawn_server_noise(writer.clone(), 0, speed);

    loop {
        let data = reader.recv_with_response(&writer).await?;
        if data.is_empty() {
            break;
        }

        let (msg_type, conn_id, payload) = decode_msg(&data)?;

        match msg_type {
            MSG_OPEN => {
                let dest = std::str::from_utf8(payload)?;
                tracing::info!("打开连接 {dest} (id={conn_id})");

                match TcpStream::connect(dest).await {
                    Ok(stream) => {
                        let _ = speed::tune_tcp(&stream);
                        writer
                            .send_server(&encode_msg(MSG_OPEN_OK, conn_id, &[]))
                            .await?;

                        let (mut target_reader, target_writer) = stream.into_split();
                        writers.insert(conn_id, target_writer);

                        let w = writer.clone();
                        tokio::spawn(async move {
                            let mut buf = vec![0u8; IO_BUF];
                            loop {
                                match target_reader.read(&mut buf).await {
                                    Ok(0) | Err(_) => {
                                        let _ = w
                                            .send_server(&encode_msg(MSG_CLOSE, conn_id, &[]))
                                            .await;
                                        break;
                                    }
                                    Ok(n) => {
                                        let mut end = n;
                                        while end < IO_BUF * 9 / 10 {
                                            match target_reader.try_read(&mut buf[end..]) {
                                                Ok(0) => break,
                                                Ok(n2) => end += n2,
                                                Err(e)
                                                    if e.kind()
                                                        == std::io::ErrorKind::WouldBlock =>
                                                {
                                                    break
                                                }
                                                Err(_) => break,
                                            }
                                        }
                                        if w.send_server(&encode_msg(
                                            MSG_DATA,
                                            conn_id,
                                            &buf[..end],
                                        ))
                                        .await
                                        .is_err()
                                        {
                                            break;
                                        }
                                    }
                                }
                            }
                        });
                    }
                    Err(e) => {
                        tracing::warn!("连接 {dest} 失败: {e}");
                        writer
                            .send_server(&encode_msg(MSG_OPEN_FAIL, conn_id, &[]))
                            .await?;
                    }
                }
            }
            MSG_DATA => {
                if let Some(w) = writers.get_mut(&conn_id) {
                    w.write_all(payload).await?;
                }
            }
            MSG_CLOSE => {
                writers.remove(&conn_id);
            }
            _ => {}
        }
    }
    Ok(())
}

fn parse_host_port(addr: &str, default_port: u16) -> (String, u16) {
    if let Some((h, p)) = addr.rsplit_once(':') {
        if let Ok(port) = p.parse() {
            return (h.to_string(), port);
        }
    }
    (addr.to_string(), default_port)
}
