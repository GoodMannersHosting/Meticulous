//! OAuth2/OIDC authentication routes.
//!
//! Provides endpoints for:
//! - Initiating OAuth login flow (redirect to provider)
//! - Handling OAuth callback (exchange code for tokens)
//! - GitHub OAuth support
//!
//! Uses the `openidconnect` crate for OIDC and `oauth2` for GitHub.

use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    response::Redirect,
    routing::get,
    Router,
};
use met_core::ids::AuthProviderId;
use met_store::repos::{AuthProviderRepo, GroupRepo, UserRepo};
use oauth2::{
    basic::BasicClient, AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken,
    PkceCodeChallenge, PkceCodeVerifier, RedirectUrl, Scope, TokenResponse as OAuth2TokenResponse,
    TokenUrl,
};
use openidconnect::{
    core::{CoreAuthenticationFlow, CoreClient, CoreIdToken, CoreProviderMetadata},
    IssuerUrl, Nonce, TokenResponse as OidcTokenResponse,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::instrument;

use crate::{
    auth::create_jwt,
    error::{ApiError, ApiResult},
    state::AppState,
};


/// In-memory storage for OAuth state (CSRF tokens, PKCE verifiers, nonces).
/// In production, this should be stored in Redis or the database with expiration.
type OAuthStateStore = Arc<RwLock<HashMap<String, OAuthPendingState>>>;

#[derive(Clone)]
struct OAuthPendingState {
    provider_id: AuthProviderId,
    pkce_verifier: Option<String>,
    nonce: Option<String>,
    redirect_uri: String,
}

/// Build the OAuth router.
pub fn router() -> Router<AppState> {
    let state_store: OAuthStateStore = Arc::new(RwLock::new(HashMap::new()));

    Router::new()
        .route("/auth/oauth/{provider_id}/login", get(oauth_login))
        // Single callback endpoint - provider ID is stored in the state parameter
        .route("/auth/oauth/callback", get(oauth_callback))
        .layer(axum::Extension(state_store))
}

#[derive(Debug, Deserialize)]
pub struct LoginQuery {
    /// Where to redirect after successful login.
    #[serde(default = "default_redirect")]
    redirect_uri: String,
}

fn default_redirect() -> String {
    "/".to_string()
}

#[derive(Debug, Serialize)]
pub struct OAuthLoginResponse {
    pub redirect_url: String,
}

/// Initiate OAuth login flow by redirecting to the provider.
#[instrument(skip(state, state_store, headers))]
async fn oauth_login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(provider_id): Path<AuthProviderId>,
    Query(query): Query<LoginQuery>,
    axum::Extension(state_store): axum::Extension<OAuthStateStore>,
) -> ApiResult<Redirect> {
    let repo = AuthProviderRepo::new(state.db());
    let provider = repo.get(provider_id).await?;

    if !provider.enabled {
        return Err(ApiError::bad_request("this auth provider is not enabled"));
    }

    // Build the callback URL using the backend's own URL from Host header
    let host = headers
        .get("host")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("localhost:8080");

    let scheme = headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_else(|| if host.contains(":443") { "https" } else { "http" });

    // Use a simple, predictable callback URL without provider ID in path
    // Provider ID is stored in the state parameter for the callback to retrieve
    let callback_url = format!(
        "{}://{}/auth/oauth/callback",
        scheme, host
    );

    let (auth_url, csrf_state, pkce_verifier, nonce) = match provider.provider_type.as_str() {
        "oidc" => build_oidc_auth_url(&provider, &callback_url).await?,
        "github" => build_github_auth_url(&provider, &callback_url)?,
        _ => return Err(ApiError::bad_request("unsupported provider type")),
    };

    // Store state for callback verification
    let pending = OAuthPendingState {
        provider_id,
        pkce_verifier: pkce_verifier.map(|v| v.secret().to_string()),
        nonce: nonce.map(|n| n.secret().to_string()),
        redirect_uri: query.redirect_uri,
    };

    {
        let mut store = state_store.write().await;
        store.insert(csrf_state.secret().to_string(), pending);
    }

    // Redirect to the provider
    Ok(Redirect::temporary(auth_url.as_str()))
}

async fn build_oidc_auth_url(
    provider: &met_core::models::AuthProvider,
    callback_url: &str,
) -> ApiResult<(url::Url, CsrfToken, Option<PkceCodeVerifier>, Option<Nonce>)> {
    let issuer_url = provider
        .issuer_url
        .as_ref()
        .ok_or_else(|| ApiError::bad_request("OIDC provider missing issuer URL"))?;

    // Build HTTP client for OIDC discovery
    let http_client = openidconnect::reqwest::ClientBuilder::new()
        .redirect(openidconnect::reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| ApiError::internal(format!("failed to build HTTP client: {e}")))?;

    // Discover OIDC provider metadata
    let issuer = IssuerUrl::new(issuer_url.clone())
        .map_err(|e| ApiError::internal(format!("invalid issuer URL: {e}")))?;

    let metadata = CoreProviderMetadata::discover_async(issuer, &http_client)
        .await
        .map_err(|e| ApiError::internal(format!("OIDC discovery failed: {e}")))?;

    // Create OIDC client
    let client = CoreClient::from_provider_metadata(
        metadata,
        ClientId::new(provider.client_id.clone()),
        Some(ClientSecret::new(provider.client_secret_ref.clone())),
    )
    .set_redirect_uri(
        RedirectUrl::new(callback_url.to_string())
            .map_err(|e| ApiError::internal(format!("invalid redirect URL: {e}")))?,
    );

    // Generate PKCE challenge
    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

    // Build authorization URL with standard OIDC scopes
    // Note: "openid" is required and added automatically by AuthorizationCode flow
    let (auth_url, csrf_state, nonce) = client
        .authorize_url(
            CoreAuthenticationFlow::AuthorizationCode,
            CsrfToken::new_random,
            Nonce::new_random,
        )
        .add_scope(openidconnect::Scope::new("email".to_string()))
        .add_scope(openidconnect::Scope::new("profile".to_string()))
        .add_scope(openidconnect::Scope::new("groups".to_string()))
        .set_pkce_challenge(pkce_challenge)
        .url();

    Ok((auth_url, csrf_state, Some(pkce_verifier), Some(nonce)))
}

fn build_github_auth_url(
    provider: &met_core::models::AuthProvider,
    callback_url: &str,
) -> ApiResult<(url::Url, CsrfToken, Option<PkceCodeVerifier>, Option<Nonce>)> {
    let client = BasicClient::new(ClientId::new(provider.client_id.clone()))
        .set_client_secret(ClientSecret::new(provider.client_secret_ref.clone()))
        .set_auth_uri(
            AuthUrl::new("https://github.com/login/oauth/authorize".to_string())
                .map_err(|e| ApiError::internal(format!("invalid auth URL: {e}")))?,
        )
        .set_token_uri(
            TokenUrl::new("https://github.com/login/oauth/access_token".to_string())
                .map_err(|e| ApiError::internal(format!("invalid token URL: {e}")))?,
        )
        .set_redirect_uri(
            RedirectUrl::new(callback_url.to_string())
                .map_err(|e| ApiError::internal(format!("invalid redirect URL: {e}")))?,
        );

    let (auth_url, csrf_state) = client
        .authorize_url(CsrfToken::new_random)
        .add_scope(Scope::new("read:user".to_string()))
        .add_scope(Scope::new("user:email".to_string()))
        .url();

    Ok((auth_url, csrf_state, None, None))
}

#[derive(Debug, Deserialize)]
pub struct CallbackQuery {
    code: String,
    state: String,
}

#[derive(Debug, Serialize)]
pub struct OAuthCallbackResponse {
    pub token: String,
    pub token_type: String,
    pub expires_in: u64,
    pub user: OAuthUser,
}

#[derive(Debug, Serialize)]
pub struct OAuthUser {
    pub id: String,
    pub email: String,
    pub name: Option<String>,
}

/// Handle OAuth callback - exchange code for tokens and create/update user.
#[instrument(skip(state, state_store, headers))]
async fn oauth_callback(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<CallbackQuery>,
    axum::Extension(state_store): axum::Extension<OAuthStateStore>,
) -> ApiResult<Redirect> {
    // Verify CSRF state and get pending state (which includes provider_id)
    let pending = {
        let mut store = state_store.write().await;
        store.remove(&query.state)
    }
    .ok_or_else(|| ApiError::bad_request("invalid or expired OAuth state"))?;

    let provider_id = pending.provider_id;

    let repo = AuthProviderRepo::new(state.db());
    let provider = repo.get(provider_id).await?;

    // Build callback URL for token exchange (must match the one used in the login request)
    let host = headers
        .get("host")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("localhost:8080");

    let scheme = headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_else(|| if host.contains(":443") { "https" } else { "http" });

    // Use the same simple callback URL as in login
    let callback_url = format!(
        "{}://{}/auth/oauth/callback",
        scheme, host
    );

    // Exchange code for tokens and get user info
    let (email, name, _external_id, oidc_groups) = match provider.provider_type.as_str() {
        "oidc" => exchange_oidc_code(&provider, &query.code, &callback_url, &pending).await?,
        "github" => {
            let (email, name, external_id) = exchange_github_code(&provider, &query.code, &callback_url).await?;
            (email, name, external_id, Vec::new())
        }
        _ => return Err(ApiError::bad_request("unsupported provider type")),
    };

    // Create or update user
    let user_repo = UserRepo::new(state.db());

    // Try to find existing user by email (including soft-deleted users)
    let user = if let Some(existing) = user_repo.get_by_email(provider.org_id, &email).await? {
        // Active user found
        existing
    } else if let Some(deleted_user) = user_repo.get_by_email_including_deleted(provider.org_id, &email).await? {
        // User was soft-deleted, restore them
        tracing::info!(
            user_id = %deleted_user.id,
            email = %email,
            "Restoring soft-deleted user via OAuth login"
        );
        user_repo.restore(deleted_user.id).await?
    } else {
        // Create new user
        let username = email.split('@').next().unwrap_or(&email).to_string();
        user_repo
            .create(
                provider.org_id,
                &username,
                &email,
                name.as_deref(),
                None, // No password for OAuth users
                false,
            )
            .await?
    };

    // Sync OIDC group memberships if any groups were returned
    if !oidc_groups.is_empty() {
        let auth_repo = AuthProviderRepo::new(state.db());
        let group_repo = GroupRepo::new(state.db());
        
        // Find mappings for the user's OIDC groups
        let mappings = auth_repo
            .find_mappings_for_claims(provider_id, &oidc_groups)
            .await?;
        
        // Add user to each mapped group
        for mapping in mappings {
            if let Err(e) = group_repo
                .add_member(mapping.meticulous_group_id, user.id, mapping.role)
                .await
            {
                tracing::warn!(
                    user_id = %user.id,
                    group_id = %mapping.meticulous_group_id,
                    oidc_group = %mapping.oidc_group_claim,
                    error = %e,
                    "Failed to add user to group from OIDC mapping"
                );
            }
        }
        
        tracing::info!(
            user_id = %user.id,
            oidc_groups = ?oidc_groups,
            "Synced OIDC group memberships"
        );
    }

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

    tracing::info!(
        user_id = %user.id,
        provider_id = %provider_id,
        email = %email,
        "OAuth login successful"
    );

    // Redirect to frontend with token
    let redirect_url = format!(
        "{}?token={}&token_type=Bearer",
        pending.redirect_uri,
        urlencoding::encode(&token)
    );

    Ok(Redirect::temporary(&redirect_url))
}

async fn exchange_oidc_code(
    provider: &met_core::models::AuthProvider,
    code: &str,
    callback_url: &str,
    pending: &OAuthPendingState,
) -> ApiResult<(String, Option<String>, String, Vec<String>)> {
    let issuer_url = provider
        .issuer_url
        .as_ref()
        .ok_or_else(|| ApiError::bad_request("OIDC provider missing issuer URL"))?;

    // Build HTTP client
    let http_client = openidconnect::reqwest::ClientBuilder::new()
        .redirect(openidconnect::reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| ApiError::internal(format!("failed to build HTTP client: {e}")))?;

    let issuer = IssuerUrl::new(issuer_url.clone())
        .map_err(|e| ApiError::internal(format!("invalid issuer URL: {e}")))?;

    let metadata = CoreProviderMetadata::discover_async(issuer, &http_client)
        .await
        .map_err(|e| ApiError::internal(format!("OIDC discovery failed: {e}")))?;

    let client = CoreClient::from_provider_metadata(
        metadata,
        ClientId::new(provider.client_id.clone()),
        Some(ClientSecret::new(provider.client_secret_ref.clone())),
    )
    .set_redirect_uri(
        RedirectUrl::new(callback_url.to_string())
            .map_err(|e| ApiError::internal(format!("invalid redirect URL: {e}")))?,
    );

    // Exchange code for tokens
    let mut token_request = client
        .exchange_code(AuthorizationCode::new(code.to_string()))
        .map_err(|e| ApiError::internal(format!("failed to create token request: {e}")))?;

    if let Some(ref verifier) = pending.pkce_verifier {
        token_request = token_request.set_pkce_verifier(PkceCodeVerifier::new(verifier.clone()));
    }

    let token_response = token_request
        .request_async(&http_client)
        .await
        .map_err(|e| ApiError::internal(format!("token exchange failed: {e}")))?;

    // Get ID token and extract claims
    let id_token: &CoreIdToken = token_response
        .id_token()
        .ok_or_else(|| ApiError::internal("no ID token in response"))?;

    // Extract groups from the ID token by parsing the raw JWT payload
    // This handles the "groups" claim which is not part of standard OIDC claims
    let groups = extract_groups_from_id_token(id_token);

    // Verify and extract standard claims
    let nonce_verifier = if let Some(ref n) = pending.nonce {
        Nonce::new(n.clone())
    } else {
        Nonce::new_random()
    };

    let id_token_verifier = client.id_token_verifier();
    let claims = id_token
        .claims(&id_token_verifier, &nonce_verifier)
        .map_err(|e| ApiError::internal(format!("ID token verification failed: {e}")))?;

    let email = claims
        .email()
        .map(|e| e.as_str().to_string())
        .ok_or_else(|| ApiError::bad_request("email claim missing from ID token"))?;

    let name = claims
        .name()
        .and_then(|n| n.get(None))
        .map(|n| n.as_str().to_string());

    let subject = claims.subject().as_str().to_string();

    Ok((email, name, subject, groups))
}

/// Extract groups claim from an ID token by parsing the JWT payload.
fn extract_groups_from_id_token(id_token: &CoreIdToken) -> Vec<String> {
    // The ID token is a JWT. We need to decode the payload to get custom claims.
    // The token format is: header.payload.signature (base64url encoded)
    // CoreIdToken can be serialized to get the JWT string
    let token_str = match serde_json::to_string(id_token) {
        Ok(s) => s.trim_matches('"').to_string(),
        Err(_) => return Vec::new(),
    };
    
    let parts: Vec<&str> = token_str.split('.').collect();
    
    if parts.len() != 3 {
        return Vec::new();
    }
    
    // Decode the payload (second part)
    use base64::Engine;
    let payload = match base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(parts[1]) {
        Ok(p) => p,
        Err(_) => return Vec::new(),
    };
    
    // Parse as JSON and extract groups
    #[derive(Deserialize)]
    struct TokenPayload {
        #[serde(default)]
        groups: Vec<String>,
    }
    
    match serde_json::from_slice::<TokenPayload>(&payload) {
        Ok(p) => p.groups,
        Err(_) => Vec::new(),
    }
}

async fn exchange_github_code(
    provider: &met_core::models::AuthProvider,
    code: &str,
    callback_url: &str,
) -> ApiResult<(String, Option<String>, String)> {
    let client = BasicClient::new(ClientId::new(provider.client_id.clone()))
        .set_client_secret(ClientSecret::new(provider.client_secret_ref.clone()))
        .set_auth_uri(
            AuthUrl::new("https://github.com/login/oauth/authorize".to_string()).unwrap(),
        )
        .set_token_uri(
            TokenUrl::new("https://github.com/login/oauth/access_token".to_string()).unwrap(),
        )
        .set_redirect_uri(RedirectUrl::new(callback_url.to_string()).unwrap());

    // Build HTTP client for oauth2
    let http_client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| ApiError::internal(format!("failed to build HTTP client: {e}")))?;

    let token_response = client
        .exchange_code(AuthorizationCode::new(code.to_string()))
        .request_async(&http_client)
        .await
        .map_err(|e| ApiError::internal(format!("GitHub token exchange failed: {e}")))?;

    let access_token = token_response.access_token().secret();

    // Fetch user info from GitHub API
    let user_response: GitHubUser = http_client
        .get("https://api.github.com/user")
        .header("Authorization", format!("Bearer {}", access_token))
        .header("User-Agent", "meticulous-ci")
        .send()
        .await
        .map_err(|e| ApiError::internal(format!("GitHub API request failed: {e}")))?
        .json()
        .await
        .map_err(|e| ApiError::internal(format!("GitHub API response parse failed: {e}")))?;

    // GitHub might not return email in user endpoint, fetch from emails endpoint
    let email = if let Some(email) = user_response.email {
        email
    } else {
        let emails: Vec<GitHubEmail> = http_client
            .get("https://api.github.com/user/emails")
            .header("Authorization", format!("Bearer {}", access_token))
            .header("User-Agent", "meticulous-ci")
            .send()
            .await
            .map_err(|e| ApiError::internal(format!("GitHub emails API failed: {e}")))?
            .json()
            .await
            .map_err(|e| ApiError::internal(format!("GitHub emails parse failed: {e}")))?;

        emails
            .into_iter()
            .find(|e| e.primary && e.verified)
            .map(|e| e.email)
            .ok_or_else(|| ApiError::bad_request("no verified email found on GitHub account"))?
    };

    Ok((email, user_response.name, user_response.id.to_string()))
}

#[derive(Debug, Deserialize)]
struct GitHubUser {
    id: u64,
    #[allow(dead_code)]
    login: String,
    name: Option<String>,
    email: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GitHubEmail {
    email: String,
    primary: bool,
    verified: bool,
}
