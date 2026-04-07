use crate::OutputFormat;
use crate::api_client::{ApiClient, ApiError, Result};
use crate::auth;
use crate::output::{build_table, format_timestamp, print_serialized, print_success, print_table};
use comfy_table::Cell;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct AuthStatus {
    pub authenticated: bool,
    pub user: Option<UserInfo>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UserInfo {
    pub id: String,
    pub username: String,
    pub email: Option<String>,
    pub org_id: Option<String>,
    pub org_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ApiToken {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub prefix: String,
    pub scopes: Vec<String>,
    pub expires_at: Option<String>,
    pub created_at: String,
    pub last_used_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ApiTokenList {
    pub data: Vec<ApiToken>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateTokenResponse {
    pub id: String,
    pub token: String,
    pub name: String,
}

pub async fn login(client: &ApiClient, server_url: &str) -> Result<()> {
    let token = auth::browser_login(server_url).await?;
    auth::store_token(&token)?;
    client.set_token(token);

    match status(client, OutputFormat::Table).await {
        Ok(()) => {}
        Err(_) => {
            print_success("Token stored, but could not verify. The server may be unreachable.");
        }
    }

    Ok(())
}

pub async fn logout() -> Result<()> {
    auth::clear_token()?;
    print_success("Logged out. Stored credentials have been cleared.");
    Ok(())
}

pub async fn status(client: &ApiClient, format: OutputFormat) -> Result<()> {
    if !client.has_token() {
        println!("Not authenticated. Run `met auth login` to sign in.");
        return Ok(());
    }

    let info: AuthStatus = client.get("/auth/status").await?;

    match format {
        OutputFormat::Table => {
            if info.authenticated {
                print_success("Authenticated");
                if let Some(ref user) = info.user {
                    println!("  User:    {} ({})", user.username, user.id);
                    if let Some(ref email) = user.email {
                        println!("  Email:   {}", email);
                    }
                    if let Some(ref org) = user.org_name {
                        println!("  Org:     {}", org);
                    }
                }
            } else {
                println!("Not authenticated. Token may be expired.");
            }
        }
        _ => print_serialized(&info, format)?,
    }
    Ok(())
}

pub async fn token_create(
    client: &ApiClient,
    name: &str,
    description: Option<&str>,
    scopes: &[String],
    expires_in_days: Option<u32>,
    format: OutputFormat,
) -> Result<()> {
    #[derive(Serialize)]
    struct Request<'a> {
        name: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<&'a str>,
        scopes: &'a [String],
        #[serde(skip_serializing_if = "Option::is_none")]
        expires_in_days: Option<u32>,
    }

    let resp: CreateTokenResponse = client
        .post(
            "/auth/tokens",
            &Request {
                name,
                description,
                scopes,
                expires_in_days,
            },
        )
        .await?;

    match format {
        OutputFormat::Table => {
            print_success(&format!("Token '{}' created", resp.name));
            println!();
            println!("  Token: {}", resp.token);
            println!();
            println!("  Save this token now — it will not be shown again.");
        }
        _ => print_serialized(&resp, format)?,
    }
    Ok(())
}

pub async fn token_list(client: &ApiClient, format: OutputFormat) -> Result<()> {
    let resp: ApiTokenList = client.get("/auth/tokens").await?;

    match format {
        OutputFormat::Table => {
            if resp.data.is_empty() {
                println!("No API tokens found.");
                return Ok(());
            }
            let mut table = build_table(&["Name", "Prefix", "Scopes", "Expires", "Last Used"]);
            for t in &resp.data {
                table.add_row(vec![
                    Cell::new(&t.name),
                    Cell::new(&t.prefix),
                    Cell::new(t.scopes.join(", ")),
                    Cell::new(
                        t.expires_at
                            .as_deref()
                            .map(format_timestamp)
                            .unwrap_or_else(|| "never".to_string()),
                    ),
                    Cell::new(
                        t.last_used_at
                            .as_deref()
                            .map(format_timestamp)
                            .unwrap_or_else(|| "never".to_string()),
                    ),
                ]);
            }
            print_table(&table);
        }
        _ => print_serialized(&resp, format)?,
    }
    Ok(())
}

pub async fn token_revoke(client: &ApiClient, id: &str, format: OutputFormat) -> Result<()> {
    client.delete(&format!("/auth/tokens/{}", id)).await?;

    match format {
        OutputFormat::Table => {
            print_success(&format!("Token '{}' revoked", id));
        }
        _ => {
            let msg = serde_json::json!({"revoked": id});
            print_serialized(&msg, format)?;
        }
    }
    Ok(())
}
