/// 速度/伪装权衡模式
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SpeedMode {
    /// 最高隐蔽：小分片 + 多伪装包（最慢）
    Stealth,
    /// 默认：批量发送 + 中等分片
    Balanced,
    /// 最快速度：大块 Plugin Message + 无伪装延迟
    #[default]
    Fast,
}

impl SpeedMode {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "stealth" | "s" => Some(Self::Stealth),
            "balanced" | "b" | "default" => Some(Self::Balanced),
            "fast" | "f" | "turbo" => Some(Self::Fast),
            _ => None,
        }
    }

    pub fn stego_chunk_size(self) -> usize {
        match self {
            Self::Stealth => 12,
            Self::Balanced => 64,
            Self::Fast => 128,
        }
    }

    pub fn bulk_chunk_size(self) -> usize {
        match self {
            Self::Stealth => 0,
            Self::Balanced => 4096,
            Self::Fast => 131_072, // 128KB MC 载体块
        }
    }

    pub fn decoy_every_n_chunks(self) -> usize {
        match self {
            Self::Stealth => 1,
            Self::Balanced => 12,
            Self::Fast => usize::MAX,
        }
    }

    pub fn leading_decoys(self) -> usize {
        match self {
            Self::Stealth => 2,
            Self::Balanced => 1,
            Self::Fast => 0,
        }
    }

    pub fn trailing_decoys(self) -> usize {
        match self {
            Self::Stealth => 1,
            Self::Balanced => 0,
            Self::Fast => 0,
        }
    }

    pub fn post_batch_delay_ms(self) -> u64 {
        match self {
            Self::Stealth => 5,
            Self::Balanced | Self::Fast => 0,
        }
    }

    pub fn noise_interval_secs(self) -> u64 {
        match self {
            Self::Stealth => 3,
            Self::Balanced => 8,
            Self::Fast => 0, // 关闭背景噪声
        }
    }

    /// 合并写入队列时最多攒多少字节再一次性封装
    pub fn write_batch_bytes(self) -> usize {
        match self {
            Self::Stealth => 16 * 1024,
            Self::Balanced => 128 * 1024,
            Self::Fast => 512 * 1024,
        }
    }
}

const TCP_BUF: usize = 2 * 1024 * 1024;

/// 优化 TCP：禁用 Nagle，扩大缓冲区
pub fn tune_tcp(stream: &tokio::net::TcpStream) -> anyhow::Result<()> {
    stream.set_nodelay(true)?;
    let sock = socket2::SockRef::from(stream);
    let _ = sock.set_send_buffer_size(TCP_BUF);
    let _ = sock.set_recv_buffer_size(TCP_BUF);
    Ok(())
}
