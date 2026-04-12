//! Admin routes for Meticulous Apps (integrations).

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, patch, post},
};
use met_core::ids::{AppInstallationId, ProjectId};
use met_core::models::{MeticulousApp, MeticulousAppInstallation};
use met_store::repos::{MeticulousAppRepo, ProjectRepo};
use rand::rngs::OsRng;
use rsa::RsaPrivateKey;
use rsa::pkcs8::{EncodePrivateKey, EncodePublicKey, LineEnding};
use serde::{Deserialize, Serialize};
use tracing::instrument;
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::extractors::Auth;
use crate::routes::admin::require_admin;
use crate::state::AppState;

/// Routes under `/api/v1/admin/meticulous-apps` (nested under admin router).
pub fn admin_router() -> Router<AppState> {
    Router::new()
        .route("/meticulous-apps", get(list_apps).post(create_app))
        .route(
            "/meticulous-apps/{application_id}",
            get(get_app).patch(patch_app_flags),
        )
        .route("/meticulous-apps/{application_id}/keys", post(add_app_key))
        .route(
            "/meticulous-apps/{application_id}/keys/{key_id}/revoke",
            post(revoke_app_key),
        )
        .route(
            "/meticulous-apps/{application_id}/installations",
            get(list_installations).post(create_installation),
        )
        .route(
            "/meticulous-apps/{application_id}/installations/{installation_id}/revoke",
            post(revoke_installation),
        )
}

fn generate_rsa_key_material() -> Result<(String, String, String), String> {
    let mut rng = OsRng;
    let private_key = RsaPrivateKey::new(&mut rng, 2048).map_err(|e| e.to_string())?;
    let public_key = rsa::RsaPublicKey::from(&private_key);
    let private_pem = private_key
        .to_pkcs8_pem(LineEnding::LF)
        .map_err(|e| e.to_string())?;
    let public_pem = public_key
        .to_public_key_pem(LineEnding::LF)
        .map_err(|e| e.to_string())?;
    let kid = format!("kid_{:x}", Uuid::now_v7().as_u128());
    Ok((kid, private_pem.to_string(), public_pem))
}

#[derive(Debug, Serialize)]
pub struct MeticulousAppSummary {
    pub id: String,
    pub application_id: String,
    pub name: String,
    pub description: Option<String>,
    pub enabled: bool,
    pub created_at: String,
}

impl From<&MeticulousApp> for MeticulousAppSummary {
    fn from(a: &MeticulousApp) -> Self {
        Self {
            id: a.id.to_string(),
            application_id: a.application_id.clone(),
            name: a.name.clone(),
            description: a.description.clone(),
            enabled: a.enabled,
            created_at: a.created_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct PatchMeticulousAppRequest {
    #[serde(default)]
    pub enabled: Option<bool>,
}

#[instrument(skip(state, body))]
async fn patch_app_flags(
    State(state): State<AppState>,
    Auth(admin): Auth,
    Path(application_id): Path<String>,
    Json(body): Json<PatchMeticulousAppRequest>,
) -> ApiResult<Json<MeticulousAppSummary>> {
    require_admin(&admin)?;
    let repo = MeticulousAppRepo::new(state.db());
    let app = repo.get_by_application_id(&application_id).await?;
    let app = if let Some(en) = body.enabled {
        repo.set_enabled(app.id, en).await?
    } else {
        app
    };
    Ok(Json(MeticulousAppSummary::from(&app)))
}

#[derive(Debug, Deserialize)]
pub struct CreateMeticulousAppRequest {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CreateMeticulousAppResponse {
    pub app: MeticulousAppSummary,
    pub key_id: String,
    /// PKCS#8 PEM private key — shown only at creation / rotation.
    pub private_key_pem: String,
}

#[instrument(skip(state, req))]
async fn create_app(
    State(state): State<AppState>,
    Auth(admin): Auth,
    Json(req): Json<CreateMeticulousAppRequest>,
) -> ApiResult<Json<CreateMeticulousAppResponse>> {
    require_admin(&admin)?;

    let name = req.name.trim();
    if name.is_empty() {
        return Err(ApiError::bad_request("name is required"));
    }

    let (kid, private_pem, public_pem) = generate_rsa_key_material().map_err(ApiError::internal)?;

    let repo = MeticulousAppRepo::new(state.db());
    let (app, _key) = repo
        .create_app_with_initial_key(
            name,
            req.description.as_deref(),
            admin.user_id,
            &kid,
            &public_pem,
        )
        .await?;

    Ok(Json(CreateMeticulousAppResponse {
        app: MeticulousAppSummary::from(&app),
        key_id: kid,
        private_key_pem: private_pem,
    }))
}

#[instrument(skip(state))]
async fn list_apps(
    State(state): State<AppState>,
    Auth(admin): Auth,
) -> ApiResult<Json<Vec<MeticulousAppSummary>>> {
    require_admin(&admin)?;
    let apps = MeticulousAppRepo::new(state.db()).list_apps().await?;
    Ok(Json(apps.iter().map(MeticulousAppSummary::from).collect()))
}

async fn app_for_application_id(
    db: &met_store::PgPool,
    application_id: &str,
) -> ApiResult<MeticulousApp> {
    MeticulousAppRepo::new(db)
        .get_by_application_id(application_id)
        .await
        .map_err(|_| ApiError::not_found("Meticulous App not found"))
}

#[instrument(skip(state))]
async fn get_app(
    State(state): State<AppState>,
    Auth(admin): Auth,
    Path(application_id): Path<String>,
) -> ApiResult<Json<MeticulousAppSummary>> {
    require_admin(&admin)?;
    let app = app_for_application_id(state.db(), &application_id).await?;
    Ok(Json(MeticulousAppSummary::from(&app)))
}

#[derive(Debug, Serialize)]
pub struct AppKeySummary {
    pub key_id: String,
    pub created_at: String,
}

#[instrument(skip(state))]
async fn add_app_key(
    State(state): State<AppState>,
    Auth(admin): Auth,
    Path(application_id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    require_admin(&admin)?;
    let app = app_for_application_id(state.db(), &application_id).await?;
    let (kid, private_pem, public_pem) = generate_rsa_key_material().map_err(ApiError::internal)?;
    MeticulousAppRepo::new(state.db())
        .add_key(app.id, &kid, &public_pem)
        .await?;

    Ok(Json(serde_json::json!({
        "key_id": kid,
        "private_key_pem": private_pem,
    })))
}

#[instrument(skip(state))]
async fn revoke_app_key(
    State(state): State<AppState>,
    Auth(admin): Auth,
    Path((application_id, key_id)): Path<(String, String)>,
) -> ApiResult<Json<serde_json::Value>> {
    require_admin(&admin)?;
    let app = app_for_application_id(state.db(), &application_id).await?;
    let n = MeticulousAppRepo::new(state.db())
        .revoke_key(app.id, &key_id)
        .await?;
    if n == 0 {
        return Err(ApiError::not_found("active key not found"));
    }
    Ok(Json(serde_json::json!({ "message": "key revoked" })))
}

#[derive(Debug, Serialize)]
pub struct InstallationSummary {
    pub id: String,
    pub project_id: String,
    pub permissions: Vec<String>,
    pub created_at: String,
    pub revoked_at: Option<String>,
}

impl From<&MeticulousAppInstallation> for InstallationSummary {
    fn from(i: &MeticulousAppInstallation) -> Self {
        Self {
            id: i.id.to_string(),
            project_id: i.project_id.to_string(),
            permissions: i.permissions.clone(),
            created_at: i.created_at.to_rfc3339(),
            revoked_at: i.revoked_at.map(|t| t.to_rfc3339()),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateInstallationBody {
    pub project_id: ProjectId,
    #[serde(default)]
    pub permissions: Vec<String>,
}

fn normalize_install_permissions(perms: &[String]) -> ApiResult<Vec<String>> {
    const ALLOWED: &[&str] = &[
        "join_tokens:create",
        "join_tokens:revoke",
        "agents:delete",
        "*",
    ];
    let mut out: Vec<String> = Vec::new();
    for p in perms {
        let p = p.trim();
        if p.is_empty() {
            continue;
        }
        if !ALLOWED.contains(&p) {
            return Err(ApiError::bad_request(format!(
                "unknown permission '{p}', allowed: join_tokens:create, join_tokens:revoke, agents:delete, *"
            )));
        }
        if !out.contains(&p.to_string()) {
            out.push(p.to_string());
        }
    }
    if out.is_empty() {
        out.push("join_tokens:create".to_string());
    }
    Ok(out)
}

#[instrument(skip(state, body))]
async fn create_installation(
    State(state): State<AppState>,
    Auth(admin): Auth,
    Path(application_id): Path<String>,
    Json(body): Json<CreateInstallationBody>,
) -> ApiResult<Json<InstallationSummary>> {
    require_admin(&admin)?;
    let app = app_for_application_id(state.db(), &application_id).await?;
    let perms = normalize_install_permissions(&body.permissions)?;
    ProjectRepo::new(state.db())
        .get(body.project_id)
        .await
        .map_err(|_| ApiError::bad_request("project not found"))?;

    let inst = MeticulousAppRepo::new(state.db())
        .create_installation(app.id, body.project_id, &perms)
        .await
        .map_err(|e| {
            if e.is_unique_violation() {
                ApiError::conflict("installation already exists for this app and project")
            } else {
                e.into()
            }
        })?;

    Ok(Json(InstallationSummary::from(&inst)))
}

#[instrument(skip(state))]
async fn list_installations(
    State(state): State<AppState>,
    Auth(admin): Auth,
    Path(application_id): Path<String>,
) -> ApiResult<Json<Vec<InstallationSummary>>> {
    require_admin(&admin)?;
    let app = app_for_application_id(state.db(), &application_id).await?;
    let items = MeticulousAppRepo::new(state.db())
        .list_installations_for_app(app.id)
        .await?;
    Ok(Json(items.iter().map(InstallationSummary::from).collect()))
}

#[instrument(skip(state))]
async fn revoke_installation(
    State(state): State<AppState>,
    Auth(admin): Auth,
    Path((application_id, installation_id)): Path<(String, AppInstallationId)>,
) -> ApiResult<Json<serde_json::Value>> {
    require_admin(&admin)?;
    let app = app_for_application_id(state.db(), &application_id).await?;
    let inst = MeticulousAppRepo::new(state.db())
        .get_installation(installation_id)
        .await
        .map_err(|_| ApiError::not_found("installation not found"))?;
    if inst.app_id != app.id {
        return Err(ApiError::not_found("installation not found"));
    }
    MeticulousAppRepo::new(state.db())
        .revoke_installation(installation_id)
        .await?;
    Ok(Json(
        serde_json::json!({ "message": "installation revoked" }),
    ))
}
