//! 代理分流策略：三档模式 + 用户白名单 + GeoIP

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::gfw::is_china_site;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProxyMode {
    /// 全部走代理（局域网除外）
    All,
    /// 国外 IP + 白名单走代理
    #[default]
    GfwOnly,
    /// 全部直连
    Direct,
}

impl ProxyMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "全部代理",
            Self::GfwOnly => "仅代理国外 IP",
            Self::Direct => "全部直连",
        }
    }

    pub fn from_cli(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "all" | "global" => Some(Self::All),
            "gfw" | "gfw_only" | "blocked" | "geoip" => Some(Self::GfwOnly),
            "direct" | "none" => Some(Self::Direct),
            _ => None,
        }
    }

    pub fn as_cli(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::GfwOnly => "gfw",
            Self::Direct => "direct",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WhitelistEntry {
    pub domain: String,
    #[serde(default)]
    pub include_subdomains: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProxyPolicy {
    #[serde(default)]
    pub mode: ProxyMode,
    #[serde(default)]
    pub whitelist: Vec<WhitelistEntry>,
}

impl Default for ProxyPolicy {
    fn default() -> Self {
        Self {
            mode: ProxyMode::GfwOnly,
            whitelist: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClientConfig {
    #[serde(default = "default_remote")]
    pub remote: String,
    #[serde(default = "default_local")]
    pub local_proxy: String,
    #[serde(default = "default_speed")]
    pub speed: String,
    #[serde(default)]
    pub proxy_mode: ProxyMode,
    #[serde(default = "default_true")]
    pub set_system_proxy: bool,
    #[serde(default)]
    pub whitelist: Vec<WhitelistEntry>,
}

fn default_remote() -> String {
    String::new()
}

fn default_local() -> String {
    "127.0.0.1:1080".to_string()
}

fn default_speed() -> String {
    "fast".to_string()
}

fn default_true() -> bool {
    true
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            remote: default_remote(),
            local_proxy: default_local(),
            speed: default_speed(),
            proxy_mode: ProxyMode::GfwOnly,
            set_system_proxy: true,
            whitelist: Vec::new(),
        }
    }
}

impl ClientConfig {
    pub fn policy(&self) -> ProxyPolicy {
        ProxyPolicy {
            mode: self.proxy_mode,
            whitelist: self.whitelist.clone(),
        }
    }
}

impl ProxyPolicy {
    /// 同步预判定：`Some(true/false)` 已决定；`None` 需 GeoIP 查询
    pub fn prefilter(&self, host: &str) -> Option<bool> {
        let host = normalize_host(host);
        if host.is_empty() || is_local_or_lan_host(&host) {
            return Some(false);
        }
        match self.mode {
            ProxyMode::Direct => Some(false),
            ProxyMode::All => Some(true),
            ProxyMode::GfwOnly => {
                if self.whitelist.iter().any(|e| whitelist_matches(&host, e)) {
                    return Some(true);
                }
                if is_china_site(&host) {
                    return Some(false);
                }
                None
            }
        }
    }
}

pub fn normalize_host(host: &str) -> String {
    host.trim()
        .trim_end_matches('.')
        .to_ascii_lowercase()
}

pub fn normalize_domain(domain: &str) -> String {
    domain
        .trim()
        .trim_start_matches('.')
        .trim_end_matches('.')
        .to_ascii_lowercase()
}

pub fn whitelist_matches(host: &str, entry: &WhitelistEntry) -> bool {
    let host = normalize_host(host);
    let domain = normalize_domain(&entry.domain);
    if domain.is_empty() {
        return false;
    }
    if entry.include_subdomains {
        host == domain || host.ends_with(&format!(".{domain}"))
    } else {
        host == domain
    }
}

pub fn is_local_or_lan_host(host: &str) -> bool {
    if host == "localhost" || host.ends_with(".local") {
        return true;
    }
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        return match ip {
            std::net::IpAddr::V4(v4) => {
                v4.is_private()
                    || v4.is_loopback()
                    || v4.is_link_local()
                    || (v4.octets()[0] == 100 && (v4.octets()[1] & 0xC0) == 64)
            }
            std::net::IpAddr::V6(v6) => v6.is_loopback() || v6.is_unique_local(),
        };
    }
    false
}

pub fn config_path() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            return dir.join("mc-tunnel-config.json");
        }
    }
    PathBuf::from("mc-tunnel-config.json")
}

pub fn load_client_config(path: &Path) -> anyhow::Result<ClientConfig> {
    if path.exists() {
        let text = std::fs::read_to_string(path)?;
        let cfg: ClientConfig = serde_json::from_str(&text)?;
        Ok(cfg)
    } else {
        Ok(ClientConfig::default())
    }
}

pub fn save_client_config(path: &Path, cfg: &ClientConfig) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string_pretty(cfg)?;
    std::fs::write(path, text)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_mode_prefilter() {
        let p = ProxyPolicy {
            mode: ProxyMode::All,
            whitelist: vec![],
        };
        assert_eq!(p.prefilter("www.google.com"), Some(true));
        assert_eq!(p.prefilter("baidu.com"), Some(true));
        assert_eq!(p.prefilter("127.0.0.1"), Some(false));
    }

    #[test]
    fn direct_mode_prefilter() {
        let p = ProxyPolicy {
            mode: ProxyMode::Direct,
            whitelist: vec![],
        };
        assert_eq!(p.prefilter("www.google.com"), Some(false));
    }

    #[test]
    fn whitelist_and_china_prefilter() {
        let p = ProxyPolicy {
            mode: ProxyMode::GfwOnly,
            whitelist: vec![WhitelistEntry {
                domain: "example.com".into(),
                include_subdomains: true,
            }],
        };
        assert_eq!(p.prefilter("example.com"), Some(true));
        assert_eq!(p.prefilter("api.example.com"), Some(true));
        assert_eq!(p.prefilter("baidu.com"), Some(false));
        assert_eq!(p.prefilter("unknown-foreign.com"), None);
    }
}
