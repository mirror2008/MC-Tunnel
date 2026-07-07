use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use crate::gfw::china_suffix_domains;
use crate::routing::{ProxyMode, ProxyPolicy, WhitelistEntry};

pub const PAC_PORT: u16 = 8091;

pub fn pac_http_url() -> String {
    format!("http://127.0.0.1:{PAC_PORT}/gfw.pac")
}

/// 生成分流 PAC，返回文件路径与内容
pub fn install_pac(http_port: u16, policy: &ProxyPolicy) -> anyhow::Result<(PathBuf, String)> {
    let dir = pac_dir()?;
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("gfw.pac");
    let body = generate_pac(http_port, policy);
    std::fs::write(&path, &body).with_context(|| format!("写入 PAC 失败: {}", path.display()))?;
    tracing::info!("分流 PAC 已写入 {}（{}）", path.display(), policy.mode.label());
    Ok((path, body))
}

/// 本地 HTTP 提供 PAC（Chrome 对 file:// PAC 支持不稳定）
pub async fn run_pac_server(body: String) -> anyhow::Result<()> {
    let listener = TcpListener::bind(format!("127.0.0.1:{PAC_PORT}"))
        .await
        .with_context(|| format!("绑定 PAC 服务端口 {PAC_PORT} 失败"))?;
    let body = Arc::new(body);
    tracing::info!("PAC HTTP 服务: {}", pac_http_url());

    loop {
        let (mut stream, _) = listener.accept().await?;
        let body = body.clone();
        tokio::spawn(async move {
            let mut req = [0u8; 512];
            let _ = stream.read(&mut req).await;
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/x-ns-proxy-autoconfig\r\nCache-Control: no-cache\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(header.as_bytes()).await;
            let _ = stream.write_all(body.as_bytes()).await;
        });
    }
}

fn pac_dir() -> anyhow::Result<PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            return Ok(dir.join("pac"));
        }
    }
    Ok(PathBuf::from("pac"))
}

fn js_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn whitelist_js_arrays(whitelist: &[WhitelistEntry]) -> (String, String) {
    let exact: Vec<String> = whitelist
        .iter()
        .filter(|e| !e.include_subdomains)
        .map(|e| format!("\"{}\"", js_escape(&e.domain)))
        .collect();
    let suffix: Vec<String> = whitelist
        .iter()
        .filter(|e| e.include_subdomains)
        .map(|e| format!("\"{}\"", js_escape(&e.domain)))
        .collect();
    (exact.join(",\n    "), suffix.join(",\n    "))
}

fn lan_direct_block() -> &'static str {
    r#"  if (isPlainHostName(host) || host === "localhost" || shExpMatch(host, "*.local"))
    return "DIRECT";
  if (isInNet(host, "10.0.0.0", "255.0.0.0") ||
      isInNet(host, "127.0.0.0", "255.0.0.0") ||
      isInNet(host, "172.16.0.0", "255.240.0.0") ||
      isInNet(host, "192.168.0.0", "255.255.0.0"))
    return "DIRECT";
"#
}

fn generate_pac(http_port: u16, policy: &ProxyPolicy) -> String {
    match policy.mode {
        ProxyMode::All => format!(
            r#"// MC-Tunnel — 全部代理（局域网除外）
function FindProxyForURL(url, host) {{
  host = host.toLowerCase();
{lan}
  return "PROXY 127.0.0.1:{http_port}";
}}
"#,
            lan = lan_direct_block(),
            http_port = http_port
        ),
        ProxyMode::Direct => format!(
            r#"// MC-Tunnel — 全部直连
function FindProxyForURL(url, host) {{
  return "DIRECT";
}}
"#
        ),
        ProxyMode::GfwOnly => generate_geoip_pac(http_port, policy),
    }
}

/// GeoIP 模式 PAC：国内域名直连，其余交本地代理做 GeoIP 判定
fn generate_geoip_pac(http_port: u16, policy: &ProxyPolicy) -> String {
    let cn_suf = china_suffix_domains()
        .iter()
        .map(|d| format!("\"{d}\""))
        .collect::<Vec<_>>()
        .join(",\n    ");
    let (wl_exact, wl_suffix) = whitelist_js_arrays(&policy.whitelist);

    format!(
        r#"// MC-Tunnel — GeoIP 分流（国外 IP 走代理，由本地 8080 判定）
function FindProxyForURL(url, host) {{
  host = host.toLowerCase();
{lan}
  var wlExact = [
    {wl_exact}
  ];
  for (var w = 0; w < wlExact.length; w++) {{
    if (host === wlExact[w]) return "PROXY 127.0.0.1:{http_port}";
  }}
  var wlSuffix = [
    {wl_suffix}
  ];
  for (var ws = 0; ws < wlSuffix.length; ws++) {{
    if (dnsDomainIs(host, wlSuffix[ws]) || shExpMatch(host, "*." + wlSuffix[ws]))
      return "PROXY 127.0.0.1:{http_port}";
  }}
  var cnSuffix = [
    {cn_suf}
  ];
  for (var i = 0; i < cnSuffix.length; i++) {{
    if (dnsDomainIs(host, cnSuffix[i].substring(1)) || shExpMatch(host, "*" + cnSuffix[i]))
      return "DIRECT";
  }}
  return "PROXY 127.0.0.1:{http_port}";
}}
"#,
        lan = lan_direct_block(),
        wl_exact = wl_exact,
        wl_suffix = wl_suffix,
        cn_suf = cn_suf,
        http_port = http_port
    )
}
