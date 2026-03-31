//! Authentication routes.
//!
//! Provides endpoints for:
//! - Login with username/password
//! - Get current user info
//! - Logout
//! - Initial system setup (first admin user)

use crate::auth::{create_jwt, hash_password, verify_password};
use crate::error::{ApiError, ApiResult};
use crate::extractors::Auth;
use crate::state::AppState;
use axum::{
    Json, Router,
    extract::State,
    routing::{get, post},
};
use met_core::models::{CreateOrganization, User};
use met_store::repos::{OrganizationRepo, UserRepo};
use serde::{Deserialize, Serialize};

/// Build the auth router.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/auth/login", post(login))
        .route("/auth/me", get(me))
        .route("/auth/logout", post(logout))
        .route("/auth/setup", get(setup_status))
        .route("/auth/setup", post(setup))
}

/// Login request body.
#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    /// Username or email.
    pub username: String,
    /// Password.
    pub password: String,
}

/// Login response body.
#[derive(Debug, Serialize)]
pub struct LoginResponse {
    /// JWT access token.
    pub token: String,
    /// Token type (always "Bearer").
    pub token_type: String,
    /// Token expiration in seconds.
    pub expires_in: u64,
    /// User information.
    pub user: UserResponse,
}

/// User response body (sanitized, no password hash).
#[derive(Debug, Serialize)]
pub struct UserResponse {
    pub id: String,
    pub username: String,
    pub email: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    pub is_admin: bool,
}

impl From<&User> for UserResponse {
    fn from(user: &User) -> Self {
        Self {
            id: user.id.to_string(),
            username: user.username.clone(),
            email: user.email.clone(),
            display_name: user.display_name.clone(),
            is_admin: user.is_admin,
        }
    }
}

/// Login with username and password.
async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> ApiResult<Json<LoginResponse>> {
    let user_repo = UserRepo::new(state.db());
    let org_repo = OrganizationRepo::new(state.db());

    // For now, we use a default organization. In a multi-tenant setup,
    // you'd determine the org from the request (subdomain, header, etc.)
    let orgs = org_repo.list(1, 0).await?;
    let org = orgs.first().ok_or_else(|| {
        ApiError::unauthorized("no organization configured - run setup first")
    })?;

    // Try to find user by username or email
    let user = user_repo
        .get_by_username(org.id, &req.username)
        .await?
        .or_else(|| None);

    let user = match user {
        Some(u) => u,
        None => {
            // Try by email
            user_repo
                .get_by_email(org.id, &req.username)
                .await?
                .ok_or_else(|| ApiError::unauthorized("invalid credentials"))?
        }
    };

    // Check if user is active
    if !user.is_active {
        return Err(ApiError::unauthorized("account is disabled"));
    }

    // Verify password
    let password_hash = user
        .password_hash
        .as_ref()
        .ok_or_else(|| ApiError::unauthorized("password login not configured for this user"))?;

    verify_password(&req.password, password_hash)
        .map_err(|_| ApiError::unauthorized("invalid credentials"))?;

    // Generate JWT
    let permissions = if user.is_admin {
        vec!["*".to_string()]
    } else {
        vec![
            "pipeline:read".to_string(),
            "run:read".to_string(),
            "agent:read".to_string(),
        ]
    };

    let token = create_jwt(
        &state.config.jwt,
        user.id,
        user.org_id,
        &user.email,
        user.display_name.as_deref(),
        permissions,
    )
    .map_err(|e| ApiError::internal(format!("failed to create token: {e}")))?;

    let expires_in = state.config.jwt.expiration.as_secs();

    Ok(Json(LoginResponse {
        token,
        token_type: "Bearer".to_string(),
        expires_in,
        user: UserResponse::from(&user),
    }))
}

/// Get current user information.
async fn me(Auth(user): Auth) -> ApiResult<Json<MeResponse>> {
    Ok(Json(MeResponse {
        id: user.user_id.to_string(),
        email: user.email,
        name: user.name,
        org_id: user.org_id.to_string(),
        permissions: user.permissions.into_iter().collect(),
    }))
}

/// Current user response.
#[derive(Debug, Serialize)]
pub struct MeResponse {
    pub id: String,
    pub email: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub org_id: String,
    pub permissions: Vec<String>,
}

/// Logout (client-side token invalidation).
async fn logout() -> ApiResult<Json<LogoutResponse>> {
    // JWT tokens are stateless, so logout is handled client-side.
    // In a production system, you might want to add the token to a blocklist.
    Ok(Json(LogoutResponse {
        message: "logged out successfully".to_string(),
    }))
}

/// Logout response.
#[derive(Debug, Serialize)]
pub struct LogoutResponse {
    pub message: String,
}

/// Setup status response.
#[derive(Debug, Serialize)]
pub struct SetupStatusResponse {
    /// Whether initial setup is required.
    pub setup_required: bool,
}

/// Check if initial setup is required.
async fn setup_status(State(state): State<AppState>) -> ApiResult<Json<SetupStatusResponse>> {
    let user_repo = UserRepo::new(state.db());
    let has_users = user_repo.any_users_exist().await?;

    Ok(Json(SetupStatusResponse {
        setup_required: !has_users,
    }))
}

/// Initial setup request.
#[derive(Debug, Deserialize)]
pub struct SetupRequest {
    /// Admin username.
    pub username: String,
    /// Admin email.
    pub email: String,
    /// Admin password.
    pub password: String,
    /// Organization name.
    #[serde(default = "default_org_name")]
    pub org_name: String,
}

fn default_org_name() -> String {
    "Default".to_string()
}

/// Initial setup response.
#[derive(Debug, Serialize)]
pub struct SetupResponse {
    pub message: String,
    pub token: String,
    pub token_type: String,
    pub expires_in: u64,
    pub user: UserResponse,
}

/// Perform initial system setup.
async fn setup(
    State(state): State<AppState>,
    Json(req): Json<SetupRequest>,
) -> ApiResult<Json<SetupResponse>> {
    let user_repo = UserRepo::new(state.db());
    let org_repo = OrganizationRepo::new(state.db());

    // Check if setup already completed
    if user_repo.any_users_exist().await? {
        return Err(ApiError::conflict("setup already completed"));
    }

    // Validate password
    if req.password.len() < 8 {
        return Err(ApiError::bad_request("password must be at least 8 characters"));
    }

    // Create slug from org name
    let slug = req
        .org_name
        .to_lowercase()
        .replace(' ', "-")
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-')
        .collect::<String>();

    // Create the organization
    let org = org_repo
        .create(&CreateOrganization {
            name: req.org_name.clone(),
            slug,
        })
        .await?;

    // Hash password
    let password_hash = hash_password(&req.password)
        .map_err(|e| ApiError::internal(format!("failed to hash password: {e}")))?;

    // Create admin user
    let user = user_repo
        .create(
            org.id,
            &req.username,
            &req.email,
            None,
            Some(&password_hash),
            true, // is_admin
        )
        .await?;

    // Generate JWT
    let token = create_jwt(
        &state.config.jwt,
        user.id,
        user.org_id,
        &user.email,
        user.display_name.as_deref(),
        vec!["*".to_string()],
    )
    .map_err(|e| ApiError::internal(format!("failed to create token: {e}")))?;

    let expires_in = state.config.jwt.expiration.as_secs();

    tracing::info!(
        user_id = %user.id,
        org_id = %org.id,
        "initial setup completed - admin user created"
    );

    Ok(Json(SetupResponse {
        message: "setup completed successfully".to_string(),
        token,
        token_type: "Bearer".to_string(),
        expires_in,
        user: UserResponse::from(&user),
    }))
}
