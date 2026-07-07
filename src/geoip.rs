//! GeoIP 分流：使用内置数据库，启动时不联网下载

use std::collections::HashMap;
use std::net::IpAddr;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use maxminddb::geoip2::Country;
use maxminddb::Reader;
use tokio::sync::Mutex;

use crate::gfw::{is_china_site, needs_proxy_by_domain};
use crate::routing::{is_local_or_lan_host, normalize_host, ProxyPolicy};

const GEOIP_FILENAME: &str = "Country.mmdb";
/// 编译时内置 GeoIP（与 data/Country.mmdb 同步）
const EMBEDDED_GEOIP: &[u8] = include_bytes!("../data/Country.mmdb");
const CACHE_TTL: Duration = Duration::from_secs(600);
const CACHE_MAX: usize = 4096;

pub struct GeoRouter {
    reader: Option<Reader<Vec<u8>>>,
    use_domain_fallback: bool,
    cache: Mutex<HashMap<String, (bool, Instant)>>,
}

impl GeoRouter {
    /// 永不失败：有库用 GeoIP，无库用域名规则
    pub async fn ensure_and_load() -> Self {
        let reader = match try_load_database() {
            Ok(r) => {
                tracing::info!("GeoIP 内置数据库已加载");
                Some(r)
            }
            Err(e) => {
                tracing::warn!(
                    "GeoIP 加载失败，已回退域名规则分流（连接不受影响）: {e:#}"
                );
                None
            }
        };
        let use_fallback = reader.is_none();
        Self {
            reader,
            use_domain_fallback: use_fallback,
            cache: Mutex::new(HashMap::new()),
        }
    }

    pub fn using_domain_fallback(&self) -> bool {
        self.use_domain_fallback
    }

    pub async fn should_proxy(&self, host: &str, policy: &ProxyPolicy) -> bool {
        if let Some(decided) = policy.prefilter(host) {
            return decided;
        }
        self.is_foreign_host(host).await
    }

    pub async fn is_foreign_host(&self, host: &str) -> bool {
        let host = normalize_host(host);
        if host.is_empty() || is_local_or_lan_host(&host) {
            return false;
        }
        if is_china_site(&host) {
            return false;
        }

        if let Some(cached) = self.cache_get(&host).await {
            return cached;
        }

        let foreign = if self.use_domain_fallback {
            needs_proxy_by_domain(&host)
        } else {
            match resolve_first_ip(&host).await {
                Some(ip) => self.is_foreign_ip(ip),
                None => {
                    tracing::debug!("DNS 解析失败，回退域名规则: {host}");
                    needs_proxy_by_domain(&host)
                }
            }
        };

        self.cache_put(host, foreign).await;
        foreign
    }

    fn is_foreign_ip(&self, ip: IpAddr) -> bool {
        if is_local_or_lan_host(&ip.to_string()) {
            return false;
        }
        let Some(reader) = &self.reader else {
            return true;
        };
        match reader.lookup::<Country<'_>>(ip) {
            Ok(country) => {
                let code = country
                    .country
                    .and_then(|c| c.iso_code)
                    .unwrap_or_default();
                let foreign = !code.eq_ignore_ascii_case("CN");
                tracing::debug!("GeoIP {ip} -> {code} foreign={foreign}");
                foreign
            }
            Err(e) => {
                tracing::debug!("GeoIP 查询失败 {ip}: {e}，视为国外");
                true
            }
        }
    }

    async fn cache_get(&self, host: &str) -> Option<bool> {
        let cache = self.cache.lock().await;
        cache.get(host).and_then(|(v, t)| {
            if t.elapsed() < CACHE_TTL {
                Some(*v)
            } else {
                None
            }
        })
    }

    async fn cache_put(&self, host: String, foreign: bool) {
        let mut cache = self.cache.lock().await;
        if cache.len() >= CACHE_MAX {
            cache.retain(|_, (_, t)| t.elapsed() < CACHE_TTL);
            if cache.len() >= CACHE_MAX {
                cache.clear();
            }
        }
        cache.insert(host, (foreign, Instant::now()));
    }
}

fn try_load_database() -> anyhow::Result<Reader<Vec<u8>>> {
    let path = geoip_db_path();
    if path.exists() {
        match Reader::open_readfile(&path) {
            Ok(reader) => {
                tracing::debug!("GeoIP 自 {}", path.display());
                return Ok(reader);
            }
            Err(e) => {
                tracing::warn!("读取 {} 失败: {e}，改用内置库", path.display());
            }
        }
    }
    load_embedded_reader()
}

fn load_embedded_reader() -> anyhow::Result<Reader<Vec<u8>>> {
    if EMBEDDED_GEOIP.len() < 512 * 1024 {
        anyhow::bail!("内置 GeoIP 数据无效");
    }
    Reader::from_source(EMBEDDED_GEOIP.to_vec()).map_err(|e| anyhow::anyhow!("解析内置 GeoIP 失败: {e}"))
}

pub fn geoip_db_path() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            return dir.join("data").join(GEOIP_FILENAME);
        }
    }
    PathBuf::from("data").join(GEOIP_FILENAME)
}

async fn resolve_first_ip(host: &str) -> Option<IpAddr> {
    if let Ok(ip) = host.parse::<IpAddr>() {
        return Some(ip);
    }
    let query = format!("{host}:0");
    let mut addrs = tokio::net::lookup_host(&query).await.ok()?;
    addrs.next().map(|a| a.ip())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn fallback_never_panics() {
        let router = GeoRouter::ensure_and_load().await;
        let _ = router.is_foreign_host("www.google.com").await;
    }

    #[test]
    fn embedded_geoip_valid() {
        let reader = load_embedded_reader().expect("内置 GeoIP 应有效");
        let _: Reader<Vec<u8>> = reader;
    }
}
