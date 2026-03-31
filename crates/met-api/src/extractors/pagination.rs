//! Cursor-based pagination extractor.
//!
//! Extracts pagination parameters from query string:
//! - `cursor`: Opaque cursor string for continuation
//! - `limit`: Number of items per page (default 25, max 100)

use crate::error::ApiError;
use axum::{
    extract::{FromRequestParts, Query},
    http::request::Parts,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Default number of items per page.
const DEFAULT_LIMIT: u32 = 25;

/// Maximum allowed items per page.
const MAX_LIMIT: u32 = 100;

/// Pagination parameters extracted from query string.
#[derive(Debug, Clone)]
pub struct Pagination {
    /// Opaque cursor for the next page (None for first page).
    pub cursor: Option<String>,
    /// Number of items to return.
    pub limit: u32,
}

impl Pagination {
    /// Create pagination for the first page with default limit.
    pub fn first_page() -> Self {
        Self {
            cursor: None,
            limit: DEFAULT_LIMIT,
        }
    }

    /// Create pagination with a specific limit.
    pub fn with_limit(limit: u32) -> Self {
        Self {
            cursor: None,
            limit: limit.min(MAX_LIMIT),
        }
    }

    /// Get the SQL LIMIT value (limit + 1 to detect if there's a next page).
    pub fn sql_limit(&self) -> i64 {
        i64::from(self.limit) + 1
    }
}

/// Query parameters for pagination.
#[derive(Debug, Deserialize)]
struct PaginationQuery {
    cursor: Option<String>,
    limit: Option<u32>,
}

impl<S> FromRequestParts<S> for Pagination
where
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let Query(query) = Query::<PaginationQuery>::from_request_parts(parts, state)
            .await
            .map_err(|e| ApiError::bad_request(format!("invalid pagination parameters: {e}")))?;

        let limit = query.limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT);

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
    fn test_pagination_defaults() {
        let page = Pagination::first_page();
        assert_eq!(page.limit, DEFAULT_LIMIT);
        assert!(page.cursor.is_none());
    }

    #[test]
    fn test_pagination_max_limit() {
        let page = Pagination::with_limit(500);
        assert_eq!(page.limit, MAX_LIMIT);
    }

    #[test]
    fn test_sql_limit() {
        let page = Pagination::with_limit(25);
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
}
