//! 基于 MinecruftPT 论文的逼真 MC 游戏行为模拟
//!
//! 参考: Tusing et al. "Minecraft tunnels for covert communications" (2025)
//! - 隐马尔可夫链生成统计上合理的动作包序列
//! - 将隧道数据嵌入 Player Position 等动作包的数据字段
//! - 双向 keep-alive 与背景噪声流量

use rand::Rng;

use crate::camouflage::{self, frame_packet, PlayPacket};
use crate::speed::SpeedMode;

/// 大块载体魔数（Plugin Message）
pub const BULK_MAGIC: [u8; 2] = [0xFE, 0xED];
/// 坐标隐写最大单包载荷（字节）
const MAX_STEGO_PAYLOAD: usize = 255;

/// 协议 763 (1.20.1) Play 状态包 ID
pub mod ids {
    pub const SB_KEEP_ALIVE: i32 = 0x1B;
    pub const SB_PLAYER_POS: i32 = 0x1D;
    pub const SB_PLAYER_POS_ROT: i32 = 0x1E;
    pub const SB_PLAYER_ROT: i32 = 0x1F;
    pub const SB_PLUGIN_MSG: i32 = 0x15;
    pub const SB_SWING_ARM: i32 = 0x38;

    pub const CB_KEEP_ALIVE: i32 = 0x2B;
    pub const CB_SYNC_POS: i32 = 0x46;
    pub const CB_UPDATE_TIME: i32 = 0x6E;
    pub const CB_LOGIN_PLAY: i32 = 0x30;
}

const STEGO_MARKER: u8 = 0xAD;

/// 游戏行为状态（论文中的 standing / walking / mining 场景）
#[derive(Clone, Copy, Debug, PartialEq)]
enum GameState {
    Standing,
    Walking,
    Mining,
}

/// 马尔可夫链动作模拟器
pub struct GameplaySimulator {
    state: GameState,
    x: f64,
    y: f64,
    z: f64,
    yaw: f32,
    pitch: f32,
    tick: u64,
    last_keepalive: u64,
    pending_ka_id: Option<i64>,
}

impl Default for GameplaySimulator {
    fn default() -> Self {
        Self {
            state: GameState::Standing,
            x: 64.5,
            y: 70.0,
            z: 64.5,
            yaw: 90.0,
            pitch: 0.0,
            tick: 0,
            last_keepalive: 0,
            pending_ka_id: None,
        }
    }
}

impl GameplaySimulator {
    pub fn new() -> Self {
        Self::default()
    }

    fn transition_state(&mut self) {
        let mut rng = rand::thread_rng();
        let roll: f32 = rng.gen();
        self.state = match self.state {
            GameState::Standing => {
                if roll < 0.25 {
                    GameState::Walking
                } else if roll < 0.35 {
                    GameState::Mining
                } else {
                    GameState::Standing
                }
            }
            GameState::Walking => {
                if roll < 0.2 {
                    GameState::Standing
                } else if roll < 0.3 {
                    GameState::Mining
                } else {
                    GameState::Walking
                }
            }
            GameState::Mining => {
                if roll < 0.3 {
                    GameState::Standing
                } else if roll < 0.45 {
                    GameState::Walking
                } else {
                    GameState::Mining
                }
            }
        };
    }

    fn drift_position(&mut self) {
        let mut rng = rand::thread_rng();
        match self.state {
            GameState::Standing => {
                self.yaw += rng.gen_range(-2.0..2.0);
                self.pitch += rng.gen_range(-0.5..0.5);
            }
            GameState::Walking => {
                let rad = self.yaw.to_radians();
                self.x += (rad.sin() as f64) * 0.08;
                self.z += (rad.cos() as f64) * 0.08;
                self.yaw += rng.gen_range(-1.0..1.0);
            }
            GameState::Mining => {
                self.pitch += rng.gen_range(-3.0..3.0);
            }
        }
        self.pitch = self.pitch.clamp(-90.0, 90.0);
    }

    /// 生成下一个伪装动作包（不含隧道数据）
    pub fn next_decoy_packet(&mut self) -> Vec<u8> {
        self.tick += 1;
        if self.tick % 8 == 0 {
            self.transition_state();
        }
        self.drift_position();

        let mut rng = rand::thread_rng();
        let action: u8 = rng.gen_range(0..100);

        let packet = match self.state {
            GameState::Standing => {
                if action < 30 {
                    PlayPacket::player_rotation(self.yaw, self.pitch, true)
                } else if action < 60 {
                    PlayPacket::player_position(self.x, self.y, self.z, true)
                } else {
                    PlayPacket::swing_arm()
                }
            }
            GameState::Walking => {
                if action < 60 {
                    PlayPacket::player_position_and_rotation(
                        self.x, self.y, self.z, self.yaw, self.pitch, true,
                    )
                } else {
                    PlayPacket::player_position(self.x, self.y, self.z, true)
                }
            }
            GameState::Mining => {
                if action < 40 {
                    PlayPacket::swing_arm()
                } else {
                    PlayPacket::player_position_and_rotation(
                        self.x, self.y, self.z, self.yaw, self.pitch, true,
                    )
                }
            }
        };

        frame_packet(&packet)
    }

    /// 批量编码隧道帧为单一 TCP 写入缓冲区（高性能路径）
    pub fn embed_tunnel_batch(&mut self, data: &[u8], mode: SpeedMode) -> Vec<u8> {
        let mut batch = Vec::with_capacity(data.len() + data.len() / 2);

        for _ in 0..mode.leading_decoys() {
            batch.extend_from_slice(&self.next_decoy_packet());
        }

        let bulk = mode.bulk_chunk_size();
        let stego = mode.stego_chunk_size();
        let decoy_n = mode.decoy_every_n_chunks().max(1);

        if bulk > 0 {
            for (i, chunk) in data.chunks(bulk).enumerate() {
                let mut payload = Vec::with_capacity(2 + chunk.len());
                payload.extend_from_slice(&BULK_MAGIC);
                payload.extend_from_slice(chunk);
                batch.extend_from_slice(&frame_packet(&PlayPacket::plugin_message(
                    "minecraft:brand",
                    payload,
                )));
                if i % decoy_n == decoy_n - 1 {
                    batch.extend_from_slice(&self.next_decoy_packet());
                }
            }
        } else {
            for (i, chunk) in data.chunks(stego).enumerate() {
                let (ex, ey, ez, yaw, pitch) =
                    embed_bytes_in_coords(self.x, self.y, self.z, chunk);
                batch.extend_from_slice(&frame_packet(
                    &PlayPacket::player_position_and_rotation(ex, ey, ez, yaw, pitch, true),
                ));
                self.drift_position();
                if i % decoy_n == decoy_n - 1 {
                    batch.extend_from_slice(&self.next_decoy_packet());
                }
            }
        }

        for _ in 0..mode.trailing_decoys() {
            batch.extend_from_slice(&self.next_decoy_packet());
        }
        batch
    }

    /// 从任意隧道载体包提取数据
    pub fn try_extract_tunnel(payload: &[u8]) -> Option<Vec<u8>> {
        Self::try_extract_bulk(payload).or_else(|| Self::try_extract_stego(payload))
    }

    fn try_extract_bulk(payload: &[u8]) -> Option<Vec<u8>> {
        let (id, id_size) = camouflage::read_varint(payload).ok()?;
        if id != ids::SB_PLUGIN_MSG {
            return None;
        }
        let rest = &payload[id_size..];
        let (ch_len, ch_off) = camouflage::read_varint(rest).ok()?;
        let data_off = ch_off + ch_len as usize;
        if data_off > rest.len() {
            return None;
        }
        let data = &rest[data_off..];
        if data.len() < 2 || data[0..2] != BULK_MAGIC {
            return None;
        }
        Some(data[2..].to_vec())
    }

    /// 从位置包中提取隐写数据（默认出生点坐标）
    pub fn try_extract_stego(payload: &[u8]) -> Option<Vec<u8>> {
        Self::try_extract_stego_with_bases(payload, 64.5, 70.0, 64.5)
    }

    pub fn try_extract_stego_with_bases(
        payload: &[u8],
        base_x: f64,
        base_y: f64,
        base_z: f64,
    ) -> Option<Vec<u8>> {
        let (id, id_size) = camouflage::read_varint(payload).ok()?;
        if id != ids::SB_PLAYER_POS_ROT && id != ids::SB_PLAYER_POS {
            return None;
        }
        extract_bytes_from_coords(&payload[id_size..], base_x, base_y, base_z)
    }

    /// 处理服务端 keep-alive，返回客户端应答包
    pub fn handle_server_keepalive(&mut self, ka_id: i64) -> Vec<u8> {
        self.pending_ka_id = Some(ka_id);
        frame_packet(&PlayPacket::keep_alive(ka_id))
    }

    pub fn should_send_keepalive(&self) -> bool {
        self.tick - self.last_keepalive > 400 // ~20s at 20tps
    }

    pub fn client_keepalive_probe(&mut self) -> Vec<u8> {
        self.last_keepalive = self.tick;
        let id = self.tick as i64;
        frame_packet(&PlayPacket::keep_alive(id))
    }
}

/// 在坐标小数位嵌入数据（模拟论文中的字段级隐写）
fn embed_bytes_in_coords(
    base_x: f64,
    base_y: f64,
    base_z: f64,
    data: &[u8],
) -> (f64, f64, f64, f32, f32) {
    let bx = encode_byte_delta(data.first().copied().unwrap_or(0));
    let by = encode_byte_delta(STEGO_MARKER);
    let bz = encode_byte_delta(data.len() as u8);
    let ex = base_x + bx;
    let ey = base_y + by;
    let ez = base_z + bz;
    let yaw = 90.0 + (data.get(1).copied().unwrap_or(0) as f32) / 10_000.0;
    let pitch = (data.get(2).copied().unwrap_or(0) as f32) / 10_000.0;
    (ex, ey, ez, yaw, pitch)
}

fn encode_byte_delta(b: u8) -> f64 {
    (b as f64) / 1_000_000.0
}

fn decode_byte_delta(value: f64, base: f64) -> u8 {
    (((value - base) * 1_000_000.0).round() as i64 & 0xFF) as u8
}

fn extract_bytes_from_coords(
    data: &[u8],
    base_x: f64,
    base_y: f64,
    base_z: f64,
) -> Option<Vec<u8>> {
    if data.len() < 25 {
        return None;
    }
    let x = f64::from_be_bytes(data[0..8].try_into().ok()?);
    let y = f64::from_be_bytes(data[8..16].try_into().ok()?);
    let z = f64::from_be_bytes(data[16..24].try_into().ok()?);

    if decode_byte_delta(y, base_y) != STEGO_MARKER {
        return None;
    }

    let len = decode_byte_delta(z, base_z) as usize;
    if len == 0 || len > MAX_STEGO_PAYLOAD {
        return None;
    }

    let mut out = vec![decode_byte_delta(x, base_x)];
    if data.len() >= 28 && len > 1 {
        let yaw = f32::from_be_bytes(data[24..28].try_into().ok()?);
        out.push(((yaw - 90.0) * 10_000.0).round() as u8);
    }
    if data.len() >= 32 && len > 2 {
        let pitch = f32::from_be_bytes(data[28..32].try_into().ok()?);
        out.push((pitch * 10_000.0).round() as u8);
    }
    while out.len() < len {
        out.push(0);
    }
    out.truncate(len);
    Some(out)
}

/// 服务端游戏行为模拟器：定期发送逼真服务端包
pub struct ServerGameplay {
    tick: u64,
    time: i64,
    last_ka: u64,
}

impl Default for ServerGameplay {
    fn default() -> Self {
        Self {
            tick: 0,
            time: 6000,
            last_ka: 0,
        }
    }
}

impl ServerGameplay {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn next_noise_packets(&mut self) -> Vec<Vec<u8>> {
        self.tick += 1;
        self.time += 20;
        let mut out = Vec::new();

        if self.tick - self.last_ka > 200 {
            self.last_ka = self.tick;
            let ka_id = self.tick as i64;
            out.push(frame_packet(&PlayPacket::server_keep_alive(ka_id)));
        }

        if self.tick % 100 == 0 {
            out.push(frame_packet(&PlayPacket::update_time(self.time, true)));
        }

        if self.tick % 50 == 0 {
            out.push(frame_packet(&PlayPacket::sync_position(64.5, 70.0, 64.5, 90.0, 0.0)));
        }

        out
    }

    pub fn next_noise_batch(&mut self) -> Vec<u8> {
        let packets = self.next_noise_packets();
        let mut batch = Vec::new();
        for p in packets {
            batch.extend_from_slice(&p);
        }
        batch
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markov_generates_varied_packets() {
        let mut sim = GameplaySimulator::new();
        let mut sizes = std::collections::HashSet::new();
        for _ in 0..20 {
            sizes.insert(sim.next_decoy_packet().len());
        }
        assert!(sizes.len() > 1);
    }

    #[test]
    fn stego_roundtrip() {
        let sim = GameplaySimulator::new();
        let data = b"hello";
        let (x, y, z, yaw, pitch) = embed_bytes_in_coords(sim.x, sim.y, sim.z, data);
        let pkt = crate::camouflage::PlayPacket::player_position_and_rotation(
            x, y, z, yaw, pitch, true,
        );
        let encoded = pkt.encode();
        let extracted =
            GameplaySimulator::try_extract_stego_with_bases(&encoded, sim.x, sim.y, sim.z);
        assert!(extracted.is_some());
        assert_eq!(extracted.unwrap().len(), data.len());
    }

    #[test]
    fn bulk_roundtrip() {
        let mut sim = GameplaySimulator::new();
        let data = vec![0xABu8; 5000];
        let batch = sim.embed_tunnel_batch(&data, SpeedMode::Fast);
        let mut offset = 0;
        let mut recovered = Vec::new();
        while offset < batch.len() {
            if let Ok(Some((payload, consumed))) = camouflage::try_parse_packet(&batch[offset..]) {
                if let Some(chunk) = GameplaySimulator::try_extract_tunnel(&payload) {
                    recovered.extend_from_slice(&chunk);
                }
                offset += consumed;
            } else {
                break;
            }
        }
        assert_eq!(recovered, data);
    }
}
