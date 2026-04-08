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
use met_core::models::{CreateOrganization, GroupRole, User};
use met_store::repos::{AuthProviderRepo, GroupRepo, OrganizationRepo, UserRepo};

/// Matches the documented bootstrap account username for default-credential UI hints.
pub(crate) const BOOTSTRAP_CREDENTIALS_USERNAME: &str = "admin";

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
    /// When true, the login UI may show that default bootstrap credentials apply (until changed).
    pub show_bootstrap_credentials_hint: bool,
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

    let user_repo = UserRepo::new(state.db());
    let show_bootstrap_credentials_hint = state.config.auth.password_enabled
        && user_repo
            .bootstrap_admin_pending_password_change(BOOTSTRAP_CREDENTIALS_USERNAME)
            .await?;

    Ok(Json(AuthProvidersResponse {
        password_enabled: state.config.auth.password_enabled,
        show_bootstrap_credentials_hint,
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
    /// When true, the client must complete a password change before using the rest of the API.
    pub password_must_change: bool,
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
    #[serde(default)]
    pub service_account: bool,
    pub password_must_change: bool,
}

impl From<&User> for UserResponse {
    fn from(user: &User) -> Self {
        Self {
            id: user.id.to_string(),
            username: user.username.clone(),
            email: user.email.clone(),
            display_name: user.display_name.clone(),
            is_admin: user.is_admin,
            service_account: user.service_account,
            password_must_change: user.password_must_change,
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
        (status = 403, description = "Password authentication disabled"),
    ),
    tag = "auth",
)]
async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> ApiResult<Json<LoginResponse>> {
    // Check if password authentication is enabled
    if !state.config.auth.password_enabled {
        return Err(ApiError::forbidden(
            "password authentication is disabled - use SSO to login",
        ));
    }

    let user_repo = UserRepo::new(state.db());
    let org_repo = OrganizationRepo::new(state.db());

    // For now, we use a default organization. In a multi-tenant setup,
    // you'd determine the org from the request (subdomain, header, etc.)
    let orgs = org_repo.list(1, 0).await?;
    let org = orgs
        .first()
        .ok_or_else(|| ApiError::unauthorized("no organization configured - run setup first"))?;

    // Try to find user by username or email (scoped to "default" org: newest by created_at).
    let user = match user_repo
        .get_by_username(org.id, &req.username)
        .await?
    {
        Some(u) => u,
        None => match user_repo
            .get_by_email(org.id, &req.username)
            .await?
        {
            Some(u) => u,
            None => return Err(ApiError::unauthorized("invalid credentials")),
        },
    };

    // Check if user is active
    if !user.is_active {
        return Err(ApiError::unauthorized("account is disabled"));
    }

    if user.service_account {
        return Err(ApiError::forbidden(
            "service accounts cannot sign in with a password; use an API token",
        ));
    }

    // Verify password
    let password_hash = user
        .password_hash
        .as_ref()
        .ok_or_else(|| ApiError::unauthorized("password login not configured for this user"))?;

    verify_password(&req.password, password_hash)
        .map_err(|_| ApiError::unauthorized("invalid credentials"))?;

    user_repo
        .record_last_login(user.id)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;

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
        password_must_change: user.password_must_change,
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
async fn me(State(state): State<AppState>, Auth(user): Auth) -> ApiResult<Json<MeResponse>> {
    // Determine role based on permissions - if they have "*" they're an admin
    let role = if user.permissions.contains("*") {
        "admin".to_string()
    } else {
        "user".to_string()
    };

    // Use display name, or derive from email if not set
    let name = user.name.clone().unwrap_or_else(|| {
        user.email
            .split('@')
            .next()
            .unwrap_or(&user.email)
            .to_string()
    });

    let group_rows = GroupRepo::new(state.db())
        .list_groups_for_user_in_org(user.org_id, user.user_id)
        .await?;
    let groups: Vec<MeGroup> = group_rows
        .into_iter()
        .map(|g| MeGroup {
            id: g.group_id.to_string(),
            name: g.name,
            role: match g.role {
                GroupRole::Member => "member".to_string(),
                GroupRole::Maintainer => "maintainer".to_string(),
                GroupRole::Owner => "owner".to_string(),
            },
        })
        .collect();

    Ok(Json(MeResponse {
        id: user.user_id.to_string(),
        email: user.email.clone(),
        name,
        org_id: user.org_id.to_string(),
        role,
        created_at: chrono::Utc::now().to_rfc3339(),
        password_must_change: user.password_must_change,
        groups,
    }))
}

/// Group membership summary on the current user profile.
#[derive(Debug, Serialize, ToSchema)]
pub struct MeGroup {
    pub id: String,
    pub name: String,
    pub role: String,
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
    pub password_must_change: bool,
    #[serde(default)]
    pub groups: Vec<MeGroup>,
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

    // Validate password using configured minimum length
    let min_length = state.config.auth.min_password_length;
    if req.password.len() < min_length {
        return Err(ApiError::bad_request(format!(
            "password must be at least {} characters",
            min_length
        )));
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
            false, // service_account
            false,
        )
        .await?;

    user_repo
        .record_last_login(user.id)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;

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
        (status = 403, description = "Password authentication disabled"),
    ),
    tag = "auth",
)]
async fn change_password(
    State(state): State<AppState>,
    Auth(current_user): Auth,
    Json(req): Json<ChangePasswordRequest>,
) -> ApiResult<Json<ChangePasswordResponse>> {
    // Check if password authentication is enabled
    if !state.config.auth.password_enabled {
        return Err(ApiError::forbidden("password authentication is disabled"));
    }

    let user_repo = UserRepo::new(state.db());

    // Validate new password using configured minimum length
    let min_length = state.config.auth.min_password_length;
    if req.new_password.len() < min_length {
        return Err(ApiError::bad_request(format!(
            "new password must be at least {} characters",
            min_length
        )));
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

    user_repo
        .update_password(current_user.user_id, &new_hash)
        .await?;

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
