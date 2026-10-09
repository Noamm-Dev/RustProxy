use std::io::{Error, ErrorKind, Result};

use tokio::io::{AsyncRead, AsyncReadExt};

const MAX_PACKET_SIZE: usize = 2_097_151;

pub async fn read_var_int<R: AsyncRead + Unpin>(stream: &mut R) -> Result<(i32, Vec<u8>)> {
    let mut num = 0u32;
    let mut shift = 0;
    let mut raw = Vec::with_capacity(5);

    loop {
        let b = stream.read_u8().await?;
        raw.push(b);
        num |= ((b & 0x7F) as u32) << shift;
        if b & 0x80 == 0 { break; }
        shift += 7;
        if shift > 28 { return Err(Error::new(ErrorKind::InvalidData, "VarInt too large")); }
    }

    Ok((num as i32, raw))
}

pub fn write_var_int(value: i32) -> Vec<u8> {
    let mut out = Vec::with_capacity(5);
    let mut v = value as u32;
    loop {
        if v & !0x7F == 0 {
            out.push(v as u8);
            return out;
        }
        out.push((v & 0x7F) as u8 | 0x80);
        v >>= 7;
    }
}

pub fn read_var_int_from(input: &mut &[u8]) -> Option<i32> {
    let mut num = 0u32;
    let mut shift = 0;
    loop {
        let (&b, rest) = input.split_first()?;
        *input = rest;
        num |= ((b & 0x7F) as u32) << shift;
        if b & 0x80 == 0 { return Some(num as i32); }
        shift += 7;
        if shift > 28 { return None; }
    }
}

pub fn read_string_from(input: &mut &[u8]) -> Option<String> {
    let len = usize::try_from(read_var_int_from(input)?).ok()?;
    if input.len() < len { return None; }
    let (bytes, rest) = input.split_at(len);
    *input = rest;
    Some(String::from_utf8_lossy(bytes).into_owned())
}

fn read_unsigned_short_from(input: &mut &[u8]) -> Option<u16> {
    let (&[hi, lo], rest) = input.split_first_chunk::<2>()?;
    *input = rest;
    Some(u16::from_be_bytes([hi, lo]))
}

pub fn write_string(s: &str) -> Vec<u8> {
    let mut out = write_var_int(s.len() as i32);
    out.extend_from_slice(s.as_bytes());
    out
}

pub async fn read_raw_packet<R: AsyncRead + Unpin>(stream: &mut R) -> Result<Vec<u8>> {
    let (length, mut packet) = read_var_int(stream).await?;
    let length = usize::try_from(length)
        .ok()
        .filter(|&len| len <= MAX_PACKET_SIZE)
        .ok_or_else(|| Error::new(ErrorKind::InvalidData, format!("Invalid packet length {length}")))?;

    let header = packet.len();
    packet.resize(header + length, 0);
    stream.read_exact(&mut packet[header..]).await?;
    Ok(packet)
}

pub fn rewrite_handshake_hostname(raw: &[u8], new_host: &str) -> Option<Vec<u8>> {
    let mut input = raw;

    read_var_int_from(&mut input)?;
    let packet_id = read_var_int_from(&mut input)?;
    if packet_id != 0x00 { return Some(raw.to_vec()); }

    let proto_version = read_var_int_from(&mut input)?;
    read_string_from(&mut input)?;
    let port = read_unsigned_short_from(&mut input)?;
    let next_state = read_var_int_from(&mut input)?;

    let mut body = write_var_int(packet_id);
    body.extend(write_var_int(proto_version));
    body.extend(write_string(new_host));
    body.extend(port.to_be_bytes());
    body.extend(write_var_int(next_state));

    Some(frame(body))
}

pub fn get_next_state(raw: &[u8]) -> Option<i32> {
    let mut input = raw;
    read_var_int_from(&mut input)?;
    read_var_int_from(&mut input)?;
    read_var_int_from(&mut input)?;
    read_string_from(&mut input)?;
    read_unsigned_short_from(&mut input)?;
    read_var_int_from(&mut input)
}

pub fn create_disconnect_packet(reason: &str) -> Vec<u8> {
    let json = format!(r#"{{"text":"{reason}"}}"#);
    let mut body = write_var_int(0x00);
    body.extend(write_string(&json));
    frame(body)
}

pub fn frame(body: Vec<u8>) -> Vec<u8> {
    let mut packet = write_var_int(body.len() as i32);
    packet.extend(body);
    packet
}

#[cfg(test)]
mod tests {
    use super::*;

    fn handshake(host: &str, next_state: i32) -> Vec<u8> {
        let mut body = write_var_int(0x00);
        body.extend(write_var_int(767));
        body.extend(write_string(host));
        body.extend(25565u16.to_be_bytes());
        body.extend(write_var_int(next_state));
        frame(body)
    }

    #[test]
    fn var_int_round_trips() {
        for value in [0, 1, 127, 128, 255, 25565, 2_097_151, i32::MAX, -1, i32::MIN] {
            let bytes = write_var_int(value);
            assert_eq!(read_var_int_from(&mut bytes.as_slice()), Some(value));
        }
    }

    #[test]
    fn var_int_rejects_overlong() {
        assert_eq!(read_var_int_from(&mut [0xFF; 6].as_slice()), None);
    }

    #[test]
    fn rewrites_handshake_host() {
        let rewritten = rewrite_handshake_hostname(&handshake("proxy.example", 2), "mc.hypixel.net").unwrap();
        assert_eq!(rewritten, handshake("mc.hypixel.net", 2));
        assert_eq!(get_next_state(&rewritten), Some(2));
    }

    #[test]
    fn rejects_truncated_handshake() {
        let raw = handshake("proxy.example", 1);
        assert_eq!(get_next_state(&raw[..raw.len() - 1]), None);
    }

    #[tokio::test]
    async fn reads_raw_packet_with_length_prefix() {
        let raw = handshake("proxy.example", 1);
        let mut stream = raw.as_slice();
        assert_eq!(read_raw_packet(&mut stream).await.unwrap(), raw);
    }

    #[tokio::test]
    async fn rejects_oversized_packet() {
        let mut stream = write_var_int(i32::MAX);
        stream.push(0);
        assert!(read_raw_packet(&mut stream.as_slice()).await.is_err());
    }
}
