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
    extract::{Path, State},
    routing::{get, post},
};
use met_core::ids::UserId;
use met_core::models::{CreateOrganization, User};
use met_store::repos::{AuthProviderRepo, OrganizationRepo, UserRepo};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Build the auth router.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/auth/login", post(login))
        .route("/auth/providers", get(list_auth_providers))
        .route("/auth/me", get(me))
        .route("/auth/logout", post(logout))
        .route("/auth/change-password", post(change_password))
        .route("/auth/setup", get(setup_status))
        .route("/auth/setup", post(setup))
        .route("/admin/users/{id}/reset-password", post(admin_reset_password))
}

/// Public auth provider info (for login page).
#[derive(Debug, Serialize, ToSchema)]
pub struct PublicAuthProvider {
    pub id: String,
    pub name: String,
    pub provider_type: String,
}

/// Auth providers response.
#[derive(Debug, Serialize, ToSchema)]
pub struct AuthProvidersResponse {
    /// Whether password authentication is enabled.
    pub password_enabled: bool,
    /// List of enabled SSO providers.
    pub providers: Vec<PublicAuthProvider>,
}

/// List enabled auth providers (public endpoint for login page).
#[utoipa::path(
    get,
    path = "/auth/providers",
    responses(
        (status = 200, description = "Auth providers", body = AuthProvidersResponse),
    ),
    tag = "auth",
)]
async fn list_auth_providers(
    State(state): State<AppState>,
) -> ApiResult<Json<AuthProvidersResponse>> {
    let org_repo = OrganizationRepo::new(state.db());
    let provider_repo = AuthProviderRepo::new(state.db());

    // Get the default organization
    let orgs = org_repo.list(1, 0).await?;

    let providers = if let Some(org) = orgs.first() {
        // Get all enabled providers for this org
        let all_providers = provider_repo.list(org.id).await?;
        all_providers
            .into_iter()
            .filter(|p| p.enabled)
            .map(|p| PublicAuthProvider {
                id: p.id.to_string(),
                name: p.name,
                provider_type: p.provider_type,
            })
            .collect()
    } else {
        vec![]
    };

    Ok(Json(AuthProvidersResponse {
        password_enabled: true, // TODO: Make this configurable
        providers,
    }))
}

/// Login request body.
#[derive(Debug, Deserialize, ToSchema)]
pub struct LoginRequest {
    /// Username or email.
    pub username: String,
    /// Password.
    pub password: String,
}

/// Login response body.
#[derive(Debug, Serialize, ToSchema)]
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
#[derive(Debug, Serialize, ToSchema)]
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
#[utoipa::path(
    post,
    path = "/auth/login",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Login successful", body = LoginResponse),
        (status = 401, description = "Invalid credentials"),
    ),
    tag = "auth",
)]
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
#[utoipa::path(
    get,
    path = "/auth/me",
    responses(
        (status = 200, description = "Current user info", body = MeResponse),
        (status = 401, description = "Unauthorized"),
    ),
    tag = "auth",
)]
async fn me(Auth(user): Auth) -> ApiResult<Json<MeResponse>> {
    // Determine role based on permissions - if they have "*" they're an admin
    let role = if user.permissions.contains("*") {
        "admin".to_string()
    } else {
        "user".to_string()
    };

    // Use display name, or derive from email if not set
    let name = user.name.unwrap_or_else(|| {
        user.email.split('@').next().unwrap_or(&user.email).to_string()
    });

    Ok(Json(MeResponse {
        id: user.user_id.to_string(),
        email: user.email.clone(),
        name,
        org_id: user.org_id.to_string(),
        role,
        created_at: chrono::Utc::now().to_rfc3339(),
    }))
}

/// Current user response.
#[derive(Debug, Serialize, ToSchema)]
pub struct MeResponse {
    pub id: String,
    pub email: String,
    pub name: String,
    pub org_id: String,
    pub role: String,
    pub created_at: String,
}

/// Logout (client-side token invalidation).
#[utoipa::path(
    post,
    path = "/auth/logout",
    responses(
        (status = 200, description = "Logged out", body = LogoutResponse),
    ),
    tag = "auth",
)]
async fn logout() -> ApiResult<Json<LogoutResponse>> {
    // JWT tokens are stateless, so logout is handled client-side.
    // In a production system, you might want to add the token to a blocklist.
    Ok(Json(LogoutResponse {
        message: "logged out successfully".to_string(),
    }))
}

/// Logout response.
#[derive(Debug, Serialize, ToSchema)]
pub struct LogoutResponse {
    pub message: String,
}

/// Setup status response.
#[derive(Debug, Serialize, ToSchema)]
pub struct SetupStatusResponse {
    /// Whether initial setup is required.
    pub setup_required: bool,
}

/// Check if initial setup is required.
#[utoipa::path(
    get,
    path = "/auth/setup",
    responses(
        (status = 200, description = "Setup status", body = SetupStatusResponse),
    ),
    tag = "auth",
)]
async fn setup_status(State(state): State<AppState>) -> ApiResult<Json<SetupStatusResponse>> {
    let user_repo = UserRepo::new(state.db());
    let has_users = user_repo.any_users_exist().await?;

    Ok(Json(SetupStatusResponse {
        setup_required: !has_users,
    }))
}

/// Initial setup request.
#[derive(Debug, Deserialize, ToSchema)]
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
#[derive(Debug, Serialize, ToSchema)]
pub struct SetupResponse {
    pub message: String,
    pub token: String,
    pub token_type: String,
    pub expires_in: u64,
    pub user: UserResponse,
}

/// Perform initial system setup.
#[utoipa::path(
    post,
    path = "/auth/setup",
    request_body = SetupRequest,
    responses(
        (status = 200, description = "Setup completed", body = SetupResponse),
        (status = 400, description = "Bad request"),
        (status = 409, description = "Setup already completed"),
    ),
    tag = "auth",
)]
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

/// Change password request.
#[derive(Debug, Deserialize, ToSchema)]
pub struct ChangePasswordRequest {
    /// Current password for verification.
    pub current_password: String,
    /// New password.
    pub new_password: String,
}

/// Change password response.
#[derive(Debug, Serialize, ToSchema)]
pub struct ChangePasswordResponse {
    pub message: String,
}

/// Change the current user's password.
#[utoipa::path(
    post,
    path = "/auth/change-password",
    request_body = ChangePasswordRequest,
    responses(
        (status = 200, description = "Password changed", body = ChangePasswordResponse),
        (status = 400, description = "Bad request"),
        (status = 401, description = "Current password incorrect"),
    ),
    tag = "auth",
)]
async fn change_password(
    State(state): State<AppState>,
    Auth(current_user): Auth,
    Json(req): Json<ChangePasswordRequest>,
) -> ApiResult<Json<ChangePasswordResponse>> {
    let user_repo = UserRepo::new(state.db());

    // Validate new password
    if req.new_password.len() < 8 {
        return Err(ApiError::bad_request("new password must be at least 8 characters"));
    }

    // Get the user to verify current password
    let user = user_repo.get(current_user.user_id).await?;

    // Verify current password
    let password_hash = user
        .password_hash
        .as_ref()
        .ok_or_else(|| ApiError::bad_request("password login not configured for this user"))?;

    verify_password(&req.current_password, password_hash)
        .map_err(|_| ApiError::unauthorized("current password is incorrect"))?;

    // Hash and update new password
    let new_hash = hash_password(&req.new_password)
        .map_err(|e| ApiError::internal(format!("failed to hash password: {e}")))?;

    user_repo.update_password(current_user.user_id, &new_hash).await?;

    tracing::info!(user_id = %current_user.user_id, "user changed password");

    Ok(Json(ChangePasswordResponse {
        message: "password changed successfully".to_string(),
    }))
}

/// Admin reset password request.
#[derive(Debug, Deserialize, ToSchema)]
pub struct AdminResetPasswordRequest {
    /// New password for the user.
    pub new_password: String,
}

/// Admin reset password response.
#[derive(Debug, Serialize, ToSchema)]
pub struct AdminResetPasswordResponse {
    pub message: String,
}

/// Admin endpoint to reset a user's password.
#[utoipa::path(
    post,
    path = "/admin/users/{id}/reset-password",
    params(("id" = String, Path, description = "User ID")),
    request_body = AdminResetPasswordRequest,
    responses(
        (status = 200, description = "Password reset", body = AdminResetPasswordResponse),
        (status = 400, description = "Bad request"),
        (status = 403, description = "Admin access required"),
    ),
    tag = "auth",
)]
async fn admin_reset_password(
    State(state): State<AppState>,
    Auth(admin): Auth,
    Path(user_id): Path<UserId>,
    Json(req): Json<AdminResetPasswordRequest>,
) -> ApiResult<Json<AdminResetPasswordResponse>> {
    // Check admin permission
    if !admin.has_permission("*") {
        return Err(ApiError::forbidden("admin access required"));
    }

    let user_repo = UserRepo::new(state.db());

    // Validate new password
    if req.new_password.len() < 8 {
        return Err(ApiError::bad_request("password must be at least 8 characters"));
    }

    // Verify the target user exists
    let target_user = user_repo.get(user_id).await?;

    // Ensure admin is in the same organization as the target user
    if target_user.org_id != admin.org_id {
        return Err(ApiError::forbidden("cannot reset password for users in other organizations"));
    }

    // Hash and update password
    let new_hash = hash_password(&req.new_password)
        .map_err(|e| ApiError::internal(format!("failed to hash password: {e}")))?;

    user_repo.update_password(user_id, &new_hash).await?;

    tracing::info!(
        admin_id = %admin.user_id,
        target_user_id = %user_id,
        "admin reset user password"
    );

    Ok(Json(AdminResetPasswordResponse {
        message: "password reset successfully".to_string(),
    }))
}
