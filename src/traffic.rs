//! 代理流量统计（原子计数 + 速率快照）

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Instant;

pub const STATS_PORT: u16 = 8092;

#[derive(Debug, Clone, Copy)]
pub struct TrafficSnapshot {
    pub up_bps: u64,
    pub down_bps: u64,
    pub up_total: u64,
    pub down_total: u64,
}

struct RateState {
    up: u64,
    down: u64,
    at: Instant,
    up_bps: u64,
    down_bps: u64,
}

pub struct TrafficCounter {
    up_bytes: AtomicU64,
    down_bytes: AtomicU64,
    rate: Mutex<RateState>,
}

impl TrafficCounter {
    pub fn new() -> Self {
        Self {
            up_bytes: AtomicU64::new(0),
            down_bytes: AtomicU64::new(0),
            rate: Mutex::new(RateState {
                up: 0,
                down: 0,
                at: Instant::now(),
                up_bps: 0,
                down_bps: 0,
            }),
        }
    }

    pub fn add_up(&self, n: usize) {
        if n > 0 {
            self.up_bytes.fetch_add(n as u64, Ordering::Relaxed);
        }
    }

    pub fn add_down(&self, n: usize) {
        if n > 0 {
            self.down_bytes.fetch_add(n as u64, Ordering::Relaxed);
        }
    }

    pub fn snapshot(&self) -> TrafficSnapshot {
        let up_total = self.up_bytes.load(Ordering::Relaxed);
        let down_total = self.down_bytes.load(Ordering::Relaxed);

        let mut rate = self.rate.lock().unwrap();
        let elapsed = rate.at.elapsed();
        if elapsed.as_millis() >= 400 {
            let secs = elapsed.as_secs_f64().max(0.001);
            let up_delta = up_total.saturating_sub(rate.up);
            let down_delta = down_total.saturating_sub(rate.down);
            rate.up_bps = (up_delta as f64 / secs) as u64;
            rate.down_bps = (down_delta as f64 / secs) as u64;
            rate.up = up_total;
            rate.down = down_total;
            rate.at = Instant::now();
        }

        TrafficSnapshot {
            up_bps: rate.up_bps,
            down_bps: rate.down_bps,
            up_total,
            down_total,
        }
    }
}

impl Default for TrafficCounter {
    fn default() -> Self {
        Self::new()
    }
}

pub async fn run_stats_server(counter: std::sync::Arc<TrafficCounter>) -> anyhow::Result<()> {
    use anyhow::Context;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    let listen = format!("127.0.0.1:{STATS_PORT}");
    let listener = TcpListener::bind(&listen)
        .await
        .with_context(|| format!("绑定流量统计端口 {STATS_PORT} 失败"))?;
    tracing::info!("流量统计 API: http://{listen}/stats");

    loop {
        let (mut stream, _) = listener.accept().await?;
        let counter = counter.clone();
        tokio::spawn(async move {
            let mut req = [0u8; 256];
            let n = stream.read(&mut req).await.unwrap_or(0);
            let req = std::str::from_utf8(&req[..n]).unwrap_or("");
            let path = req.split_whitespace().nth(1).unwrap_or("/");

            let (status, body) = if path == "/stats" || path.starts_with("/stats?") {
                let s = counter.snapshot();
                let body = format!(
                    r#"{{"up_bps":{},"down_bps":{},"up_total":{},"down_total":{}}}"#,
                    s.up_bps, s.down_bps, s.up_total, s.down_total
                );
                ("200 OK", body)
            } else {
                ("404 Not Found", r#"{"error":"not found"}"#.to_string())
            };

            let header = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nCache-Control: no-cache\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(header.as_bytes()).await;
            let _ = stream.write_all(body.as_bytes()).await;
        });
    }
}
