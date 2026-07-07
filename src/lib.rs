pub mod camouflage;
pub mod gameplay;
pub mod geoip;
pub mod gfw;
pub mod leigod_bridge;
pub mod pac;
pub mod routing;
pub mod socks5;
pub mod speed;
pub mod sysproxy;
pub mod traffic;
pub mod tunnel;

use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use tokio::net::TcpListener;
use crate::tunnel::TunnelWriterHandle;
use tokio::time::{sleep, timeout};
use tracing_subscriber::EnvFilter;

use crate::geoip::GeoRouter;
use crate::leigod_bridge::{LeigodBridge, DEFAULT_BRIDGE_PORT};
use crate::pac::{install_pac, pac_http_url, run_pac_server};
use crate::routing::{ProxyMode, ProxyPolicy};
use crate::socks5::{
    accept_tunnel, connect_tunnel, run_http_proxy, run_socks5_proxy, server_mux_session,
};
use crate::speed::SpeedMode;
use crate::sysproxy::{disable_system_proxy, enable_gfw_pac, enable_system_proxy};
use crate::traffic::{run_stats_server, TrafficCounter};

pub fn init_logging() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .try_init();
}

const HTTP_PROXY: &str = "127.0.0.1:8080";
const HTTP_PORT: u16 = 8080;
const BRIDGE_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(25);
const BRIDGE_MAX_RETRIES: u32 = 3;

async fn connect_via_leigod_only(
    remote: &str,
    speed: SpeedMode,
) -> anyhow::Result<(LeigodBridge, Arc<crate::socks5::TunnelMux>)> {
    LeigodBridge::ensure_leigod_running()?;

    let bridge = LeigodBridge::start(remote, DEFAULT_BRIDGE_PORT)
        .await
        .context("启动 javaw 雷神中继失败")?;

    let tcp_target = format!("127.0.0.1:{DEFAULT_BRIDGE_PORT}");
    let mut last_err = String::new();

    for attempt in 1..=BRIDGE_MAX_RETRIES {
        match timeout(
            BRIDGE_HANDSHAKE_TIMEOUT,
            connect_tunnel(remote, speed, Some(&tcp_target)),
        )
        .await
        {
            Ok(Ok(mux)) => {
                tracing::info!("隧道经 javaw/雷神 MC 通道建立（禁止直连 VPS）");
                return Ok((bridge, Arc::new(mux)));
            }
            Ok(Err(e)) => {
                last_err = e.to_string();
                tracing::warn!("雷神通道握手失败 ({attempt}/{BRIDGE_MAX_RETRIES}): {last_err}");
            }
            Err(_) => {
                last_err = format!("{BRIDGE_HANDSHAKE_TIMEOUT:?} 内无响应");
                tracing::warn!("雷神通道超时 ({attempt}/{BRIDGE_MAX_RETRIES})");
            }
        }
        if attempt < BRIDGE_MAX_RETRIES {
            sleep(Duration::from_secs(2)).await;
        }
    }

    drop(bridge);
    anyhow::bail!(
        "无法经雷神通道连接 VPS（已重试 {BRIDGE_MAX_RETRIES} 次）。最后错误: {last_err}。请确认雷神已加速 Minecraft，并尝试切换路由模式（模式3），关闭其他 VPN/代理"
    )
}

pub async fn run_client(
    remote: &str,
    local: &str,
    policy: ProxyPolicy,
    set_system_proxy: bool,
    speed: SpeedMode,
) -> anyhow::Result<()> {
    tracing::info!(
        "正在经雷神通道连接 MC 服务器 {remote}，速度 {:?}，分流 {}",
        speed,
        policy.mode.label()
    );

    let (_bridge, mux) = connect_via_leigod_only(remote, speed).await?;
    let geo = Arc::new(GeoRouter::ensure_and_load().await);
    if geo.using_domain_fallback() {
        tracing::warn!("GeoIP 未加载，当前使用域名规则分流");
    }
    let policy = Arc::new(policy);
    let traffic = Arc::new(TrafficCounter::new());
    let traffic_stats = traffic.clone();
    tokio::spawn(async move {
        if let Err(e) = run_stats_server(traffic_stats).await {
            tracing::warn!("流量统计服务退出: {e}");
        }
    });

    if set_system_proxy {
        match policy.mode {
            ProxyMode::Direct => {
                disable_system_proxy()?;
                tracing::info!("系统代理已关闭（全部直连）");
            }
            ProxyMode::All => {
                enable_system_proxy("127.0.0.1", HTTP_PORT)?;
                tracing::info!("系统全局 HTTP 代理已开启 — {HTTP_PROXY}");
            }
            ProxyMode::GfwOnly => {
                let (_pac_path, pac_body) = install_pac(HTTP_PORT, &policy)?;
                tokio::spawn(async move {
                    if let Err(e) = run_pac_server(pac_body).await {
                        tracing::warn!("PAC 服务退出: {e}");
                    }
                });
                sleep(Duration::from_millis(300)).await;
                enable_gfw_pac(&pac_http_url())?;
                tracing::info!(
                    "系统 PAC 已启用 — {}（{}）",
                    pac_http_url(),
                    policy.mode.label()
                );
            }
        }
    }

    tracing::info!(
        "隧道已建立 — SOCKS5 {local}，HTTP {HTTP_PROXY}（{}）",
        policy.mode.label()
    );
    tokio::select! {
        r = run_socks5_proxy(local, mux.clone(), policy.clone(), geo.clone(), traffic.clone()) => r,
        r = run_http_proxy(HTTP_PROXY, mux, policy, geo, traffic) => r,
    }
}

pub async fn run_server(listen: &str, speed: SpeedMode) -> anyhow::Result<()> {
    let listener = TcpListener::bind(listen)
        .await
        .with_context(|| format!("绑定 {listen} 失败"))?;

    tracing::info!("服务端监听 {listen}，模式 {:?}", speed);

    loop {
        let (writer, reader) = accept_tunnel(&listener, speed).await?;
        let handle = TunnelWriterHandle::spawn(writer);
        tokio::spawn(async move {
            if let Err(e) = server_mux_session(reader, handle, speed).await {
                tracing::debug!("会话结束: {e}");
            }
        });
    }
}

pub fn stop_proxy() -> anyhow::Result<()> {
    disable_system_proxy()
}
