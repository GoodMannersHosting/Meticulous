//! Decode `met-output` frames from a byte buffer (one job step or aggregated read).

use met_core::output_ipc::{decode_frame, OUTPUT_AGGREGATE_MAX_BYTES, OUTPUT_MSG_SECRET, OUTPUT_MSG_VAR};
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct DrainedOutputs {
    pub public: HashMap<String, String>,
    /// Plain secret values as bytes (UTF-8) before wrapping for the controller.
    pub secret_plain: HashMap<String, Vec<u8>>,
}

#[derive(Debug)]
pub enum DrainError {
    MalformedFrame,
    AggregateLimit,
}

/// Decode frames sequentially; **last-wins** per key within each of `public` / `secret_plain`.
pub fn decode_output_bytes(data: &[u8]) -> Result<DrainedOutputs, DrainError> {
    let mut out = DrainedOutputs::default();
    let mut pos = 0usize;
    let mut agg = 0usize;

    while pos < data.len() {
        let slice = &data[pos..];
        let (msg_ty, key, value, consumed) = decode_frame(slice).map_err(|_| DrainError::MalformedFrame)?;
        pos += consumed;

        let add_len = match msg_ty {
            OUTPUT_MSG_VAR => value.len(),
            OUTPUT_MSG_SECRET => {
                // budget counts eventual envelope size — approximate with plaintext + fixed overhead
                value.len() + 32 + 12 + 16
            }
            _ => return Err(DrainError::MalformedFrame),
        };
        agg = agg.saturating_add(add_len);
        if agg > OUTPUT_AGGREGATE_MAX_BYTES {
            return Err(DrainError::AggregateLimit);
        }

        match msg_ty {
            OUTPUT_MSG_VAR => {
                let s = String::from_utf8(value).map_err(|_| DrainError::MalformedFrame)?;
                out.public.insert(key, s);
            }
            OUTPUT_MSG_SECRET => {
                out.secret_plain.insert(key, value);
            }
            _ => return Err(DrainError::MalformedFrame),
        }
    }

    Ok(out)
}
