//! Framed IPC protocol between `met-output` and the agent.
//!
//! Spec: `design/workflow-invocation-outputs.md`

/// Magic bytes on every frame (`"MOUT"`).
pub const OUTPUT_IPC_MAGIC: [u8; 4] = *b"MOUT";

/// Current wire version (`design/workflow-invocation-outputs.md`).
pub const OUTPUT_IPC_VERSION: u8 = 1;

/// Maximum UTF-8 length of a single VAR or SECRET **value** (plaintext on IPC for secrets).
pub const OUTPUT_VALUE_MAX_BYTES: usize = 16 * 1024 * 1024;

/// Maximum **stored footprint** aggregated per job run (public UTF-8 lengths + secret envelope lengths).
pub const OUTPUT_AGGREGATE_MAX_BYTES: usize = 256 * 1024 * 1024;

/// Header (16 bytes) plus this margin for key (`key_len` is capped implicitly by max frame).
pub const OUTPUT_FRAME_HEADER_SIZE: usize = 16;

/// `value_len + key_len + header` must not exceed this cap.
pub const OUTPUT_FRAME_MAX_BYTES: usize = OUTPUT_VALUE_MAX_BYTES + 4096;

/// VAR message type.
pub const OUTPUT_MSG_VAR: u8 = 1;

/// SECRET message type (plaintext on IPC inside the agent boundary only).
pub const OUTPUT_MSG_SECRET: u8 = 2;

/// Max key length (UTF-8 bytes).
pub const OUTPUT_KEY_MAX_BYTES: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputIpcDecodeError {
    TooShort,
    BadMagic,
    UnsupportedVersion,
    ReservedNonZero,
    KeyLengthZero,
    KeyTooLong,
    FrameTooLarge,
    ValueTooLarge,
    TruncatedPayload,
}

/// Encode a single frame.
pub fn encode_frame(
    msg_type: u8,
    key: &str,
    value: &[u8],
) -> Result<Vec<u8>, OutputIpcDecodeError> {
    validate_key(key)?;
    if value.len() > OUTPUT_VALUE_MAX_BYTES {
        return Err(OutputIpcDecodeError::ValueTooLarge);
    }
    let key_bytes = key.as_bytes();
    let kl = key_bytes.len();
    let vl = value.len();
    let frame_body = OUTPUT_FRAME_HEADER_SIZE + kl + vl;
    if frame_body > OUTPUT_FRAME_MAX_BYTES {
        return Err(OutputIpcDecodeError::FrameTooLarge);
    }

    let mut out = Vec::with_capacity(frame_body);
    out.extend_from_slice(&OUTPUT_IPC_MAGIC);
    out.push(OUTPUT_IPC_VERSION);
    out.push(msg_type);
    out.extend_from_slice(&0u16.to_be_bytes());
    out.extend_from_slice(&(kl as u32).to_be_bytes());
    out.extend_from_slice(&(vl as u32).to_be_bytes());
    out.extend_from_slice(key_bytes);
    out.extend_from_slice(value);
    Ok(out)
}

/// Validate output key grammar and reserved names (`design/workflow-invocation-outputs.md`).
pub fn validate_key(key: &str) -> Result<(), OutputIpcDecodeError> {
    let b = key.as_bytes();
    if b.is_empty() || b.len() > OUTPUT_KEY_MAX_BYTES {
        return Err(OutputIpcDecodeError::KeyTooLong);
    }
    let first = b[0];
    if !first.is_ascii_alphabetic() && first != b'_' {
        return Err(OutputIpcDecodeError::KeyTooLong);
    }
    for &c in &b[1..] {
        if !c.is_ascii_alphanumeric() && c != b'_' {
            return Err(OutputIpcDecodeError::KeyTooLong);
        }
    }
    const RESERVED: &[&str] = &["PATH", "PWD", "HOME", "USER"];
    if RESERVED.contains(&key) || key.starts_with("MET_OUTPUT_RESERVED_") {
        return Err(OutputIpcDecodeError::KeyTooLong);
    }
    Ok(())
}

/// Decode one frame from the front of `buf`. Returns `(msg_type, key, value, consumed)`.
pub fn decode_frame(buf: &[u8]) -> Result<(u8, String, Vec<u8>, usize), OutputIpcDecodeError> {
    if buf.len() < OUTPUT_FRAME_HEADER_SIZE {
        return Err(OutputIpcDecodeError::TooShort);
    }
    if buf[..4] != OUTPUT_IPC_MAGIC {
        return Err(OutputIpcDecodeError::BadMagic);
    }
    if buf[4] != OUTPUT_IPC_VERSION {
        return Err(OutputIpcDecodeError::UnsupportedVersion);
    }
    if u16::from_be_bytes([buf[6], buf[7]]) != 0 {
        return Err(OutputIpcDecodeError::ReservedNonZero);
    }
    let msg_type = buf[5];
    let kl = u32::from_be_bytes([buf[8], buf[9], buf[10], buf[11]]) as usize;
    let vl = u32::from_be_bytes([buf[12], buf[13], buf[14], buf[15]]) as usize;
    if kl == 0 {
        return Err(OutputIpcDecodeError::KeyLengthZero);
    }
    if kl > OUTPUT_KEY_MAX_BYTES {
        return Err(OutputIpcDecodeError::KeyTooLong);
    }
    if vl > OUTPUT_VALUE_MAX_BYTES {
        return Err(OutputIpcDecodeError::ValueTooLarge);
    }
    let total = OUTPUT_FRAME_HEADER_SIZE + kl + vl;
    if total > OUTPUT_FRAME_MAX_BYTES {
        return Err(OutputIpcDecodeError::FrameTooLarge);
    }
    if buf.len() < total {
        return Err(OutputIpcDecodeError::TruncatedPayload);
    }
    let key = std::str::from_utf8(&buf[OUTPUT_FRAME_HEADER_SIZE..OUTPUT_FRAME_HEADER_SIZE + kl])
        .map_err(|_| OutputIpcDecodeError::TruncatedPayload)?;
    validate_key(key)?;
    let value = buf[OUTPUT_FRAME_HEADER_SIZE + kl..total].to_vec();
    Ok((msg_type, key.to_string(), value, total))
}

/// Parse `KEY=value` for a single argv (first `=` separates key from value; value may contain `=`).
pub fn parse_key_value_arg(arg: &str) -> Result<(&str, &str), ()> {
    let Some((k, v)) = arg.split_once('=') else {
        return Err(());
    };
    if k.is_empty() {
        return Err(());
    }
    Ok((k, v))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_var() {
        let f = encode_frame(OUTPUT_MSG_VAR, "IMAGE_URI", b"ghcr.io/x:y").unwrap();
        let (t, k, v, n) = decode_frame(&f).unwrap();
        assert_eq!(n, f.len());
        assert_eq!(t, OUTPUT_MSG_VAR);
        assert_eq!(k, "IMAGE_URI");
        assert_eq!(v, b"ghcr.io/x:y");
    }
}
