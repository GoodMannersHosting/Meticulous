//! Cursor-based pagination extractor.
//!
//! Extracts pagination parameters from query string:
//! - `cursor`: Opaque cursor string for continuation
//! - `limit` or `per_page`: Page size (`per_page` is accepted as an alias for clients that use that name)
//!
//! Default and maximum page size come from [`crate::config::ApiConfig`] (also set via `MetConfig.http`
//! in TOML or `MET_HTTP__PAGINATION_*` environment variables).

use crate::error::ApiError;
use crate::state::AppState;
use axum::{
    extract::{FromRequestParts, Query},
    http::request::Parts,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Pagination parameters extracted from query string.
#[derive(Debug, Clone)]
pub struct Pagination {
    /// Opaque cursor for the next page (None for first page).
    pub cursor: Option<String>,
    /// Number of items to return.
    pub limit: u32,
}

impl Pagination {
    /// Get the SQL LIMIT value (limit + 1 to detect if there's a next page).
    pub fn sql_limit(&self) -> i64 {
        i64::from(self.limit) + 1
    }
}

/// `cursor` for offset-based list endpoints: a non-negative SQL `OFFSET` as a decimal string.
///
/// Invalid, empty, or negative values clamp to `0`.
#[must_use]
pub fn parse_sql_offset_cursor(cursor: Option<&str>) -> i64 {
    let Some(raw) = cursor else {
        return 0;
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return 0;
    }
    trimmed.parse::<i64>().ok().filter(|&o| o >= 0).unwrap_or(0)
}

/// Query parameters for pagination.
#[derive(Debug, Deserialize)]
struct PaginationQuery {
    cursor: Option<String>,
    limit: Option<u32>,
    #[serde(rename = "per_page")]
    per_page: Option<u32>,
}

impl FromRequestParts<AppState> for Pagination {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let Query(query) = Query::<PaginationQuery>::from_request_parts(parts, state)
            .await
            .map_err(|e| ApiError::bad_request(format!("invalid pagination parameters: {e}")))?;

        let cfg = state.config();
        let max = cfg.pagination_max_limit.max(1);
        let default = cfg.pagination_default_limit.max(1).min(max);

        let requested = query.limit.or(query.per_page);
        let limit = requested.unwrap_or(default).max(1).min(max);

        Ok(Pagination {
            cursor: query.cursor,
            limit,
        })
    }
}

/// Response wrapper for paginated results.
#[derive(Debug, Serialize, ToSchema)]
pub struct PaginatedResponse<T> {
    /// The items in this page.
    pub data: Vec<T>,
    /// Pagination metadata.
    pub pagination: PaginationMeta,
}

/// Pagination metadata included in responses.
#[derive(Debug, Serialize, ToSchema)]
pub struct PaginationMeta {
    /// Cursor for the next page (null if no more pages).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    /// Whether there are more items.
    pub has_more: bool,
    /// Number of items in this response.
    pub count: usize,
}

impl<T> PaginatedResponse<T> {
    /// Create a paginated response from items.
    ///
    /// If `items.len() > limit`, there's a next page and we use the last item's
    /// cursor function to generate the next cursor.
    pub fn new<F>(mut items: Vec<T>, limit: u32, cursor_fn: F) -> Self
    where
        F: Fn(&T) -> String,
    {
        let has_more = items.len() > limit as usize;
        if has_more {
            items.pop();
        }

        let next_cursor = if has_more {
            items.last().map(&cursor_fn)
        } else {
            None
        };

        let count = items.len();

        Self {
            data: items,
            pagination: PaginationMeta {
                next_cursor,
                has_more,
                count,
            },
        }
    }

    /// Create an empty paginated response.
    pub fn empty() -> Self {
        Self {
            data: Vec::new(),
            pagination: PaginationMeta {
                next_cursor: None,
                has_more: false,
                count: 0,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sql_limit() {
        let page = Pagination {
            cursor: None,
            limit: 25,
        };
        assert_eq!(page.sql_limit(), 26);
    }

    #[test]
    fn test_paginated_response_with_more() {
        let items: Vec<i32> = (1..=26).collect();
        let response = PaginatedResponse::new(items, 25, |i| i.to_string());

        assert_eq!(response.data.len(), 25);
        assert!(response.pagination.has_more);
        assert_eq!(response.pagination.next_cursor, Some("25".to_string()));
    }

    #[test]
    fn test_paginated_response_no_more() {
        let items: Vec<i32> = (1..=20).collect();
        let response = PaginatedResponse::new(items, 25, |i| i.to_string());

        assert_eq!(response.data.len(), 20);
        assert!(!response.pagination.has_more);
        assert!(response.pagination.next_cursor.is_none());
    }

    #[test]
    fn sql_offset_cursor_none_and_empty() {
        assert_eq!(parse_sql_offset_cursor(None), 0);
        assert_eq!(parse_sql_offset_cursor(Some("")), 0);
        assert_eq!(parse_sql_offset_cursor(Some("   ")), 0);
    }

    #[test]
    fn sql_offset_cursor_valid() {
        assert_eq!(parse_sql_offset_cursor(Some("0")), 0);
        assert_eq!(parse_sql_offset_cursor(Some("42 ")), 42);
    }

    #[test]
    fn sql_offset_cursor_invalid_clamps_to_zero() {
        assert_eq!(parse_sql_offset_cursor(Some("-1")), 0);
        assert_eq!(parse_sql_offset_cursor(Some("not-a-number")), 0);
        assert_eq!(parse_sql_offset_cursor(Some("999999999999999999999")), 0);
    }
}
