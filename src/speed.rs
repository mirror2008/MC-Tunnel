/// 速度/伪装权衡模式
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SpeedMode {
    /// 最高隐蔽：小分片 + 多伪装包（最慢）
    Stealth,
    /// 默认：批量发送 + 中等分片
    Balanced,
    /// 快速：大块 Plugin Message
    Fast,
    /// 极速：更大块 + 更大批量（仍用 MC Plugin Message 伪装）
    #[default]
    Turbo,
}

/// SOCKS/隧道读写缓冲区
pub const IO_BUF: usize = 1_048_576;

impl SpeedMode {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "stealth" | "s" => Some(Self::Stealth),
            "balanced" | "b" | "default" => Some(Self::Balanced),
            "fast" | "f" => Some(Self::Fast),
            "turbo" | "t" | "ultra" => Some(Self::Turbo),
            _ => None,
        }
    }

    pub fn stego_chunk_size(self) -> usize {
        match self {
            Self::Stealth => 12,
            Self::Balanced => 64,
            Self::Fast | Self::Turbo => 128,
        }
    }

    pub fn bulk_chunk_size(self) -> usize {
        match self {
            Self::Stealth => 0,
            Self::Balanced => 8192,
            Self::Fast => 262_144,
            Self::Turbo => 524_288,
        }
    }

    pub fn decoy_every_n_chunks(self) -> usize {
        match self {
            Self::Stealth => 1,
            Self::Balanced => 24,
            Self::Fast => 64,
            Self::Turbo => usize::MAX,
        }
    }

    pub fn leading_decoys(self) -> usize {
        match self {
            Self::Stealth => 2,
            Self::Balanced => 1,
            Self::Fast | Self::Turbo => 0,
        }
    }

    pub fn trailing_decoys(self) -> usize {
        match self {
            Self::Stealth => 1,
            Self::Balanced | Self::Fast | Self::Turbo => 0,
        }
    }

    pub fn post_batch_delay_ms(self) -> u64 {
        match self {
            Self::Stealth => 5,
            Self::Balanced | Self::Fast | Self::Turbo => 0,
        }
    }

    pub fn noise_interval_secs(self) -> u64 {
        match self {
            Self::Stealth => 3,
            Self::Balanced => 12,
            Self::Fast | Self::Turbo => 0,
        }
    }

    /// 合并写入队列时最多攒多少字节再一次性封装
    pub fn write_batch_bytes(self) -> usize {
        match self {
            Self::Stealth => 32 * 1024,
            Self::Balanced => 256 * 1024,
            Self::Fast => 1_048_576,
            Self::Turbo => 4_194_304,
        }
    }
}

const TCP_BUF: usize = 4 * 1024 * 1024;

/// 优化 TCP：禁用 Nagle，扩大缓冲区，开启 keepalive 防 NAT idle 断连
pub fn tune_tcp(stream: &tokio::net::TcpStream) -> anyhow::Result<()> {
    use std::time::Duration;

    stream.set_nodelay(true)?;
    let sock = socket2::SockRef::from(stream);
    let _ = sock.set_send_buffer_size(TCP_BUF);
    let _ = sock.set_recv_buffer_size(TCP_BUF);
    let keepalive = socket2::TcpKeepalive::new()
        .with_time(Duration::from_secs(30))
        .with_interval(Duration::from_secs(10));
    let _ = sock.set_tcp_keepalive(&keepalive);
    Ok(())
}
