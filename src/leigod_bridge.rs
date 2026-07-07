use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use anyhow::Context;
use tokio::net::TcpListener;
use tokio::time::sleep;

pub const DEFAULT_BRIDGE_PORT: u16 = 25566;

pub struct LeigodBridge {
    child: Child,
    pub local_addr: String,
}

struct BridgeLaunch {
    javaw: PathBuf,
    class_dir: PathBuf,
}

impl LeigodBridge {
    /// 确认雷神加速器正在运行
    pub fn ensure_leigod_running() -> anyhow::Result<()> {
        let out = Command::new("tasklist")
            .args(["/FI", "IMAGENAME eq leigod.exe", "/NH"])
            .output()
            .context("无法检测雷神进程")?;
        let text = String::from_utf8_lossy(&out.stdout);
        if !text.to_ascii_lowercase().contains("leigod.exe") {
            anyhow::bail!(
                "未检测到雷神加速器 (leigod.exe)。\n请先打开雷神 → 选择 Minecraft → 开启加速后再连接"
            );
        }
        Ok(())
    }

    /// 启动 javaw 中继：127.0.0.1:bridge_port → remote_host:remote_port
    pub async fn start(remote: &str, bridge_port: u16) -> anyhow::Result<Self> {
        let (host, port) = parse_host_port(remote, 25565);
        let launch = prepare_bridge_launch()?;

        let listen_host = "127.0.0.1";
        tracing::info!(
            "启动 javaw 雷神中继 {listen_host}:{bridge_port} → {host}:{port}"
        );

        let child = Command::new(&launch.javaw)
            .arg("-cp")
            .arg(&launch.class_dir)
            .arg("McLeigodBridge")
            .args([
                listen_host,
                &bridge_port.to_string(),
                &host,
                &port.to_string(),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("启动 javaw 中继失败，请确认已安装 Java")?;

        let local_addr = format!("{listen_host}:{bridge_port}");
        wait_for_port(listen_host, bridge_port, Duration::from_secs(15)).await
            .with_context(|| format!("javaw 中继未在 {local_addr} 就绪"))?;

        tracing::info!("javaw 中继已就绪（进程名 javaw.exe，可走雷神 MC 通道）");
        Ok(Self { child, local_addr })
    }
}

impl Drop for LeigodBridge {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

async fn wait_for_port(host: &str, port: u16, timeout_dur: Duration) -> anyhow::Result<()> {
    let deadline = tokio::time::Instant::now() + timeout_dur;
    loop {
        // 端口已被 bridge 占用 = 监听就绪（勿用 connect 探测，会多开一条中继连接）
        if TcpListener::bind((host, port)).await.is_err() {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            anyhow::bail!("等待端口 {host}:{port} 超时");
        }
        sleep(Duration::from_millis(200)).await;
    }
}

fn find_javaw() -> anyhow::Result<PathBuf> {
    if let Ok(p) = which_javaw_via_path() {
        return Ok(p);
    }
    for candidate in [
        r"C:\Program Files\Common Files\Oracle\Java\javapath\javaw.exe",
        r"C:\Program Files (x86)\Common Files\Oracle\Java\javapath\javaw.exe",
    ] {
        let p = PathBuf::from(candidate);
        if p.exists() {
            return Ok(p);
        }
    }
    anyhow::bail!("找不到 javaw.exe，请先安装 Java 或 Minecraft 启动器")
}

fn which_javaw_via_path() -> anyhow::Result<PathBuf> {
    let output = Command::new("where")
        .arg("javaw")
        .output()
        .context("执行 where javaw 失败")?;
    if !output.status.success() {
        anyhow::bail!("where javaw 未找到");
    }
    let line = String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .unwrap_or("")
        .trim()
        .to_string();
    if line.is_empty() {
        anyhow::bail!("where javaw 输出为空");
    }
    Ok(PathBuf::from(line))
}

fn prepare_bridge_launch() -> anyhow::Result<BridgeLaunch> {
    let class_dir = find_bridge_class_dir()?;
    let class_file = class_dir.join("McLeigodBridge.class");
    if !class_file.exists() {
        anyhow::bail!(
            "找不到 McLeigodBridge.class，请运行 bridge 目录下的 javac McLeigodBridge.java"
        );
    }
    Ok(BridgeLaunch {
        javaw: find_javaw()?,
        class_dir,
    })
}

fn find_bridge_class_dir() -> anyhow::Result<PathBuf> {
    let exe = std::env::current_exe().context("无法定位当前 exe")?;
    let dir = exe.parent().context("exe 无父目录")?;
    for candidate in [
        dir.join("bridge"),
        dir.join("..").join("bridge"),
        PathBuf::from("bridge"),
        PathBuf::from("release").join("bridge"),
    ] {
        if candidate.join("McLeigodBridge.class").exists() {
            return Ok(candidate);
        }
    }
    anyhow::bail!("找不到 bridge/McLeigodBridge.class")
}

fn parse_host_port(addr: &str, default_port: u16) -> (String, u16) {
    if let Some((h, p)) = addr.rsplit_once(':') {
        if let Ok(port) = p.parse() {
            return (h.to_string(), port);
        }
    }
    (addr.to_string(), default_port)
}
