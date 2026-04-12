//! Capped object listing for rough usage estimates (not a full inventory).

use crate::traits::{ListOptions, ObjectStore};

/// Result of scanning object metadata up to configured limits.
#[derive(Debug, Clone, Default)]
pub struct ObjectStoreUsageEstimate {
    /// Sum of `Content-Length` / listed sizes for scanned objects.
    pub bytes_summed: u64,
    /// Number of object rows returned by list.
    pub objects_scanned: u64,
    /// Number of ListObjectsV2 (or equivalent) calls.
    pub list_pages: u32,
    /// True when more objects may exist beyond the scan cap.
    pub truncated: bool,
}

/// List objects under `prefix` until `max_objects` or `max_pages`, summing sizes.
///
/// Intended for **small** buckets or **sampled** estimates. Large production buckets should use
/// provider metrics (e.g. CloudWatch) instead of exhaustive listing.
pub async fn estimate_prefix_size(
    store: &(dyn ObjectStore + Send + Sync),
    prefix: &str,
    max_objects: u64,
    max_pages: u32,
    page_size: i32,
) -> crate::Result<ObjectStoreUsageEstimate> {
    if max_pages == 0 || max_objects == 0 {
        return Ok(ObjectStoreUsageEstimate::default());
    }
    let page_size = page_size.clamp(1, 1000);

    let mut out = ObjectStoreUsageEstimate::default();
    let mut token: Option<String> = None;

    for _ in 0..max_pages {
        if out.objects_scanned >= max_objects {
            out.truncated = true;
            break;
        }

        let remaining = max_objects.saturating_sub(out.objects_scanned);
        let this_page = (remaining as i32).min(page_size).max(1);

        let list_result = store
            .list_objects_with_options(
                prefix,
                ListOptions {
                    max_keys: Some(this_page),
                    continuation_token: token.take(),
                    delimiter: None,
                },
            )
            .await?;

        out.list_pages = out.list_pages.saturating_add(1);

        for obj in list_result.objects {
            out.bytes_summed = out.bytes_summed.saturating_add(obj.size);
            out.objects_scanned = out.objects_scanned.saturating_add(1);
            if out.objects_scanned >= max_objects {
                out.truncated = list_result.is_truncated
                    || list_result.next_continuation_token.is_some();
                return Ok(out);
            }
        }

        if list_result.is_truncated {
            token = list_result.next_continuation_token;
            if token.is_none() {
                break;
            }
        } else {
            break;
        }
    }

    if token.is_some() {
        out.truncated = true;
    }

    Ok(out)
}

impl ObjectStoreUsageEstimate {
    /// Validate caps to keep server work bounded.
    pub fn sanitize_caps(max_objects: u64, max_pages: u32) -> (u64, u32) {
        (max_objects.clamp(1, 50_000), max_pages.clamp(1, 100))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_caps_clamps() {
        let (o, p) = ObjectStoreUsageEstimate::sanitize_caps(999_999, 999);
        assert_eq!(o, 50_000);
        assert_eq!(p, 100);
    }
}
