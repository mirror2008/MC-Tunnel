use std::collections::VecDeque;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::sync::mpsc;
use tokio::time::sleep;

use crate::camouflage::{
    self, encode_tunnel_frame, parse_server_keepalive,
};
use crate::gameplay::{GameplaySimulator, ServerGameplay};
use crate::speed::SpeedMode;

const READ_BUF: usize = 262_144;

enum WriteCmd {
    Tunnel(Vec<u8>),
    Raw(Vec<u8>),
}

/// 异步隧道写入队列：合并多帧、消除 Mutex 竞争
#[derive(Clone)]
pub struct TunnelWriterHandle {
    tx: mpsc::UnboundedSender<WriteCmd>,
}

impl TunnelWriterHandle {
    pub fn spawn(writer: TunnelWriter) -> Self {
        let (tx, mut rx) = mpsc::unbounded_channel();
        tokio::spawn(async move {
            let mut writer = writer;
            while let Some(cmd) = rx.recv().await {
                match cmd {
                    WriteCmd::Raw(data) => {
                        if writer.write_raw(&data).await.is_err() {
                            break;
                        }
                        while let Ok(WriteCmd::Raw(d)) = rx.try_recv() {
                            if writer.write_raw(&d).await.is_err() {
                                return;
                            }
                        }
                    }
                    WriteCmd::Tunnel(data) => {
                        let frame = encode_tunnel_frame(&data);
                        if writer.send_encoded_frame(&frame).await.is_err() {
                            break;
                        }
                        while let Ok(WriteCmd::Tunnel(d)) = rx.try_recv() {
                            let f = encode_tunnel_frame(&d);
                            if writer.send_encoded_frame(&f).await.is_err() {
                                return;
                            }
                        }
                        while let Ok(WriteCmd::Raw(d)) = rx.try_recv() {
                            if writer.write_raw(&d).await.is_err() {
                                return;
                            }
                        }
                    }
                }
            }
        });
        Self { tx }
    }

    pub async fn send(&self, data: &[u8]) -> anyhow::Result<()> {
        self.tx
            .send(WriteCmd::Tunnel(data.to_vec()))
            .map_err(|_| anyhow::anyhow!("隧道写入队列已关闭"))
    }

    pub async fn send_server(&self, data: &[u8]) -> anyhow::Result<()> {
        self.send(data).await
    }

    pub async fn write_raw(&self, data: &[u8]) -> anyhow::Result<()> {
        self.tx
            .send(WriteCmd::Raw(data.to_vec()))
            .map_err(|_| anyhow::anyhow!("隧道写入队列已关闭"))
    }
}

/// 隧道写入端：隐写嵌入 + 批量发送
pub struct TunnelWriter {
    writer: OwnedWriteHalf,
    simulator: GameplaySimulator,
    speed: SpeedMode,
}

/// 隧道读取端：解析隐写数据 + 自动应答 keep-alive
pub struct TunnelReader {
    reader: OwnedReadHalf,
    read_buf: Vec<u8>,
    stego_chunks: Vec<Vec<u8>>,
    frame_queue: VecDeque<Vec<u8>>,
    simulator: GameplaySimulator,
}

impl TunnelWriter {
    pub fn new(writer: OwnedWriteHalf, speed: SpeedMode) -> Self {
        Self {
            writer,
            simulator: GameplaySimulator::new(),
            speed,
        }
    }

    pub fn speed(&self) -> SpeedMode {
        self.speed
    }

    pub async fn write_raw(&mut self, data: &[u8]) -> anyhow::Result<()> {
        self.writer.write_all(data).await?;
        Ok(())
    }

    async fn send_encoded_frame(&mut self, frame: &[u8]) -> anyhow::Result<()> {
        let batch = self.simulator.embed_tunnel_batch(frame, self.speed);
        self.writer.write_all(&batch).await?;
        let delay = self.speed.post_batch_delay_ms();
        if delay > 0 {
            sleep(std::time::Duration::from_millis(delay)).await;
        }
        Ok(())
    }

    pub async fn send(&mut self, data: &[u8]) -> anyhow::Result<()> {
        let frame = encode_tunnel_frame(data);
        self.send_encoded_frame(&frame).await
    }

    pub async fn send_server(&mut self, data: &[u8]) -> anyhow::Result<()> {
        self.send(data).await
    }

    pub fn start_background_noise(handle: TunnelWriterHandle, speed: SpeedMode) {
        if speed.noise_interval_secs() == 0 {
            return;
        }
        let interval = speed.noise_interval_secs();
        tokio::spawn(async move {
            let mut sim = GameplaySimulator::new();
            loop {
                sleep(std::time::Duration::from_secs(interval)).await;
                let decoy = sim.next_decoy_packet();
                if handle.write_raw(&decoy).await.is_err() {
                    break;
                }
            }
        });
    }
}

impl TunnelReader {
    pub fn new(reader: OwnedReadHalf) -> Self {
        Self {
            reader,
            read_buf: Vec::with_capacity(READ_BUF * 2),
            stego_chunks: Vec::new(),
            frame_queue: VecDeque::new(),
            simulator: GameplaySimulator::new(),
        }
    }

    pub async fn read_raw_packet(&mut self) -> anyhow::Result<Vec<u8>> {
        loop {
            if let Some((payload, consumed)) = camouflage::try_parse_packet(&self.read_buf)? {
                self.read_buf.drain(..consumed);
                return Ok(payload);
            }
            let n = self.read_more().await?;
            if n == 0 {
                anyhow::bail!("连接已关闭");
            }
        }
    }

    pub async fn recv_with_response(
        &mut self,
        writer: &TunnelWriterHandle,
    ) -> anyhow::Result<Vec<u8>> {
        loop {
            if let Some(data) = self.frame_queue.pop_front() {
                return Ok(data);
            }

            let payload = self.read_next_payload().await?;
            if payload.is_empty() {
                return Ok(Vec::new());
            }

            if let Some(ka_id) = parse_server_keepalive(&payload) {
                let resp = self.simulator.handle_server_keepalive(ka_id);
                writer.write_raw(&resp).await?;
                continue;
            }

            if let Some(chunk) = GameplaySimulator::try_extract_tunnel(&payload) {
                self.stego_chunks.push(chunk);
                self.drain_decoded_frames();
                if let Some(data) = self.frame_queue.pop_front() {
                    return Ok(data);
                }
            }
        }
    }

    fn drain_decoded_frames(&mut self) {
        let (frames, remainder) = camouflage::split_tunnel_frames(&self.stego_chunks);
        self.stego_chunks.clear();
        if !remainder.is_empty() {
            self.stego_chunks.push(remainder);
        }
        self.frame_queue.extend(frames);
    }

    async fn read_next_payload(&mut self) -> anyhow::Result<Vec<u8>> {
        loop {
            if let Some((payload, consumed)) = camouflage::try_parse_packet(&self.read_buf)? {
                self.read_buf.drain(..consumed);
                return Ok(payload);
            }
            let n = self.read_more().await?;
            if n == 0 {
                return Ok(Vec::new());
            }
        }
    }

    async fn read_more(&mut self) -> anyhow::Result<usize> {
        let mut tmp = vec![0u8; READ_BUF];
        let n = self.reader.read(&mut tmp).await?;
        if n > 0 {
            self.read_buf.extend_from_slice(&tmp[..n]);
        }
        Ok(n)
    }
}

pub fn spawn_server_noise(handle: TunnelWriterHandle, reader_id: u64, speed: SpeedMode) {
    if speed.noise_interval_secs() == 0 {
        return;
    }
    let interval = speed.noise_interval_secs();
    tokio::spawn(async move {
        let mut server_sim = ServerGameplay::new();
        loop {
            sleep(std::time::Duration::from_secs(interval)).await;
            let batch = server_sim.next_noise_batch();
            if batch.is_empty() {
                continue;
            }
            if handle.write_raw(&batch).await.is_err() {
                tracing::debug!("背景噪声任务结束 session={reader_id}");
                return;
            }
        }
    });
}

pub fn spawn_client_keepalive(handle: TunnelWriterHandle) {
    tokio::spawn(async move {
        let mut sim = GameplaySimulator::new();
        loop {
            sleep(std::time::Duration::from_secs(20)).await;
            let pkt = sim.client_keepalive_probe();
            if handle.write_raw(&pkt).await.is_err() {
                break;
            }
        }
    });
}
