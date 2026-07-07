//! Minecraft 协议 VarInt 编解码与数据包封装/解析

pub const PROTOCOL_VERSION: i32 = 763; // 1.20.1

/// Login 状态 Login Success
pub const LOGIN_SUCCESS_ID: i32 = 0x02;

pub fn write_varint(buf: &mut Vec<u8>, mut value: i32) {
    loop {
        let mut byte = (value & 0x7F) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        buf.push(byte);
        if value == 0 {
            break;
        }
    }
}

pub fn write_varlong(buf: &mut Vec<u8>, mut value: i64) {
    loop {
        let mut byte = (value & 0x7F) as i64;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        buf.push(byte as u8);
        if value == 0 {
            break;
        }
    }
}

pub fn read_varint(data: &[u8]) -> anyhow::Result<(i32, usize)> {
    let mut value = 0i32;
    let mut shift = 0u32;
    for (i, &byte) in data.iter().enumerate() {
        value |= ((byte & 0x7F) as i32) << shift;
        if byte & 0x80 == 0 {
            return Ok((value, i + 1));
        }
        shift += 7;
        if shift > 35 {
            anyhow::bail!("VarInt 过长");
        }
    }
    anyhow::bail!("VarInt 不完整")
}

fn write_string(buf: &mut Vec<u8>, s: &str) {
    write_varint(buf, s.len() as i32);
    buf.extend_from_slice(s.as_bytes());
}

fn write_identifier(buf: &mut Vec<u8>, id: &str) {
    write_string(buf, id);
}

/// Play 状态数据包构建器
pub enum PlayPacket {
    KeepAlive(i64),
    PlayerPosition(f64, f64, f64, bool),
    PlayerPositionAndRotation(f64, f64, f64, f32, f32, bool),
    PlayerRotation(f32, f32, bool),
    SwingArm,
    PluginMessage(String, Vec<u8>),
    ServerKeepAlive(i64),
    SyncPosition(f64, f64, f64, f32, f32),
    UpdateTime(i64, bool),
    LoginPlay,
}

impl PlayPacket {
    pub fn keep_alive(id: i64) -> Self {
        Self::KeepAlive(id)
    }
    pub fn player_position(x: f64, y: f64, z: f64, on_ground: bool) -> Self {
        Self::PlayerPosition(x, y, z, on_ground)
    }
    pub fn player_position_and_rotation(
        x: f64,
        y: f64,
        z: f64,
        yaw: f32,
        pitch: f32,
        on_ground: bool,
    ) -> Self {
        Self::PlayerPositionAndRotation(x, y, z, yaw, pitch, on_ground)
    }
    pub fn player_rotation(yaw: f32, pitch: f32, on_ground: bool) -> Self {
        Self::PlayerRotation(yaw, pitch, on_ground)
    }
    pub fn swing_arm() -> Self {
        Self::SwingArm
    }
    pub fn plugin_message(channel: &str, data: Vec<u8>) -> Self {
        Self::PluginMessage(channel.to_string(), data)
    }
    pub fn server_keep_alive(id: i64) -> Self {
        Self::ServerKeepAlive(id)
    }
    pub fn sync_position(x: f64, y: f64, z: f64, yaw: f32, pitch: f32) -> Self {
        Self::SyncPosition(x, y, z, yaw, pitch)
    }
    pub fn update_time(time: i64, tick: bool) -> Self {
        Self::UpdateTime(time, tick)
    }
    pub fn login_play() -> Self {
        Self::LoginPlay
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut payload = Vec::new();
        match self {
            Self::KeepAlive(id) => {
                write_varint(&mut payload, crate::gameplay::ids::SB_KEEP_ALIVE);
                write_varlong(&mut payload, *id);
            }
            Self::PlayerPosition(x, y, z, g) => {
                write_varint(&mut payload, crate::gameplay::ids::SB_PLAYER_POS);
                payload.extend_from_slice(&x.to_be_bytes());
                payload.extend_from_slice(&y.to_be_bytes());
                payload.extend_from_slice(&z.to_be_bytes());
                payload.push(*g as u8);
            }
            Self::PlayerPositionAndRotation(x, y, z, yaw, pitch, g) => {
                write_varint(&mut payload, crate::gameplay::ids::SB_PLAYER_POS_ROT);
                payload.extend_from_slice(&x.to_be_bytes());
                payload.extend_from_slice(&y.to_be_bytes());
                payload.extend_from_slice(&z.to_be_bytes());
                payload.extend_from_slice(&yaw.to_be_bytes());
                payload.extend_from_slice(&pitch.to_be_bytes());
                payload.push(*g as u8);
            }
            Self::PlayerRotation(yaw, pitch, g) => {
                write_varint(&mut payload, crate::gameplay::ids::SB_PLAYER_ROT);
                payload.extend_from_slice(&yaw.to_be_bytes());
                payload.extend_from_slice(&pitch.to_be_bytes());
                payload.push(*g as u8);
            }
            Self::SwingArm => {
                write_varint(&mut payload, crate::gameplay::ids::SB_SWING_ARM);
                write_varint(&mut payload, 0); // main hand
            }
            Self::PluginMessage(ch, data) => {
                write_varint(&mut payload, crate::gameplay::ids::SB_PLUGIN_MSG);
                write_identifier(&mut payload, ch);
                payload.extend_from_slice(data);
            }
            Self::ServerKeepAlive(id) => {
                write_varint(&mut payload, crate::gameplay::ids::CB_KEEP_ALIVE);
                write_varlong(&mut payload, *id);
            }
            Self::SyncPosition(x, y, z, yaw, pitch) => {
                write_varint(&mut payload, crate::gameplay::ids::CB_SYNC_POS);
                payload.extend_from_slice(&x.to_be_bytes());
                payload.extend_from_slice(&y.to_be_bytes());
                payload.extend_from_slice(&z.to_be_bytes());
                payload.extend_from_slice(&yaw.to_be_bytes());
                payload.extend_from_slice(&pitch.to_be_bytes());
                payload.push(0); // flags
                write_varint(&mut payload, 1); // teleport id
            }
            Self::UpdateTime(time, tick) => {
                write_varint(&mut payload, crate::gameplay::ids::CB_UPDATE_TIME);
                write_varlong(&mut payload, *time);
                write_varlong(&mut payload, if *tick { *time } else { 0 });
                payload.push(*tick as u8);
            }
            Self::LoginPlay => {
                write_varint(&mut payload, crate::gameplay::ids::CB_LOGIN_PLAY);
                payload.extend_from_slice(&1i32.to_be_bytes()); // entity id
                payload.push(0); // not hardcore
                write_varint(&mut payload, 1); // 1 dimension name
                write_identifier(&mut payload, "minecraft:overworld");
                write_varint(&mut payload, 20); // max players
                write_varint(&mut payload, 10); // view distance
                write_varint(&mut payload, 10); // simulation distance
                payload.push(0); // reduced debug
                payload.push(1); // enable respawn
                payload.push(0); // limited crafting
                write_varint(&mut payload, 0); // dimension type
                write_identifier(&mut payload, "minecraft:overworld");
                payload.extend_from_slice(&0i64.to_be_bytes()); // hashed seed
                payload.push(0); // survival
                payload.push(0xffu8); // previous gamemode undefined
                payload.push(0); // not debug
                payload.push(0); // not flat
                payload.push(0); // no death location
                write_varint(&mut payload, 0); // portal cooldown
                write_varint(&mut payload, 63); // sea level
                payload.push(0); // no secure chat
            }
        }
        payload
    }
}

/// 将 payload 封装为 MC 长度前缀包
pub fn frame_packet(packet: &PlayPacket) -> Vec<u8> {
    let payload = packet.encode();
    let mut framed = Vec::new();
    write_varint(&mut framed, payload.len() as i32);
    framed.extend_from_slice(&payload);
    framed
}

/// 构建 MC 握手 + Login Start
pub fn build_handshake(host: &str, port: u16) -> Vec<u8> {
    let mut hs = Vec::new();
    write_varint(&mut hs, 0x00);
    write_varint(&mut hs, PROTOCOL_VERSION);
    write_string(&mut hs, host);
    hs.extend_from_slice(&port.to_be_bytes());
    write_varint(&mut hs, 2);

    let mut login = Vec::new();
    write_varint(&mut login, 0x00);
    write_string(&mut login, "Steve");

    let mut out = Vec::new();
    write_varint(&mut out, hs.len() as i32);
    out.extend_from_slice(&hs);
    write_varint(&mut out, login.len() as i32);
    out.extend_from_slice(&login);
    out
}

/// 客户端登录后应答包（brand + 位置确认）
pub fn build_client_login_response() -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&frame_packet(&PlayPacket::plugin_message(
        "minecraft:brand",
        b"vanilla".to_vec(),
    )));
    out.extend_from_slice(&frame_packet(&PlayPacket::player_position_and_rotation(
        64.5, 70.0, 64.5, 90.0, 0.0, true,
    )));
    out
}

/// 服务端完整登录序列（Login Success → Join Game → Sync Position → Time）
pub fn build_server_login_sequence() -> Vec<u8> {
    let mut login_success = Vec::new();
    write_varint(&mut login_success, LOGIN_SUCCESS_ID);
    write_string(
        &mut login_success,
        "00000000-0000-0000-0000-000000000001",
    );
    write_string(&mut login_success, "Steve");
    write_varint(&mut login_success, 0); // no properties

    let mut out = Vec::new();
    write_varint(&mut out, login_success.len() as i32);
    out.extend_from_slice(&login_success);

    out.extend_from_slice(&frame_packet(&PlayPacket::login_play()));
    out.extend_from_slice(&frame_packet(&PlayPacket::sync_position(
        64.5, 70.0, 64.5, 90.0, 0.0,
    )));
    out.extend_from_slice(&frame_packet(&PlayPacket::update_time(6000, true)));
    out.extend_from_slice(&frame_packet(&PlayPacket::server_keep_alive(1)));
    out
}

/// 将隧道帧编码为可隐写的原始字节（带长度前缀）
pub fn encode_tunnel_frame(data: &[u8]) -> Vec<u8> {
    let mut frame = Vec::new();
    write_varint(&mut frame, data.len() as i32);
    frame.extend_from_slice(data);
    frame
}

/// 从隐写分片解析全部完整隧道帧，返回剩余未完整字节
pub fn split_tunnel_frames(chunks: &[Vec<u8>]) -> (Vec<Vec<u8>>, Vec<u8>) {
    let mut raw = Vec::new();
    for c in chunks {
        raw.extend_from_slice(c);
    }
    let mut frames = Vec::new();
    let mut offset = 0usize;
    while offset < raw.len() {
        let slice = &raw[offset..];
        let (len, hdr) = match read_varint(slice) {
            Ok(v) => v,
            Err(_) => break,
        };
        if len < 0 {
            break;
        }
        let total = hdr + len as usize;
        if raw.len() < offset + total {
            break;
        }
        frames.push(raw[offset + hdr..offset + total].to_vec());
        offset += total;
    }
    (frames, raw[offset..].to_vec())
}

/// 从隐写分片重组隧道帧（可能含多帧，返回第一帧）
pub fn decode_tunnel_chunks(chunks: &[Vec<u8>]) -> anyhow::Result<Vec<u8>> {
    let (frames, _) = split_tunnel_frames(chunks);
    frames
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("隧道帧为空"))
}

/// 从隐写分片解析全部完整隧道帧
pub fn decode_tunnel_frames(chunks: &[Vec<u8>]) -> anyhow::Result<Vec<Vec<u8>>> {
    Ok(split_tunnel_frames(chunks).0)
}

/// 从缓冲区解析一个完整的 MC 数据包
pub fn try_parse_packet(buf: &[u8]) -> anyhow::Result<Option<(Vec<u8>, usize)>> {
    if buf.is_empty() {
        return Ok(None);
    }
    let (packet_len, len_size) = match read_varint(buf) {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };
    if packet_len < 0 || packet_len > 2 * 1024 * 1024 {
        anyhow::bail!("非法数据包长度");
    }
    let total = len_size + packet_len as usize;
    if buf.len() < total {
        return Ok(None);
    }
    Ok(Some((buf[len_size..total].to_vec(), total)))
}

/// 检测 keep-alive 包并返回 ID
pub fn parse_server_keepalive(payload: &[u8]) -> Option<i64> {
    let (id, id_size) = read_varint(payload).ok()?;
    if id != crate::gameplay::ids::CB_KEEP_ALIVE {
        return None;
    }
    let rest = &payload[id_size..];
    if rest.len() < 8 {
        return None;
    }
    Some(i64::from_be_bytes(rest[0..8].try_into().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varint_roundtrip() {
        let mut buf = Vec::new();
        write_varint(&mut buf, 300);
        let (v, n) = read_varint(&buf).unwrap();
        assert_eq!(v, 300);
        assert_eq!(n, 2);
    }

    #[test]
    fn login_sequence_has_multiple_packets() {
        let seq = build_server_login_sequence();
        assert!(seq.len() > 50);
    }
}
