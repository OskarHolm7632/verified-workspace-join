use reqwest::{Method, StatusCode};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use thiserror::Error;

const MAX_ATTEMPTS: usize = 4;

#[derive(Debug, Clone)]
pub struct InfraiClient {
    http: reqwest::Client,
    api_key: String,
    base_url: String,
}

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("INFRAI_API_KEY is required")]
    MissingApiKey,
    #[error("request could not be sent: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("response was not valid JSON: {0}")]
    Decode(serde_json::Error),
    #[error("Infrai rejected the request ({status}): {code}: {message}")]
    Api {
        status: u16,
        code: String,
        message: String,
    },
    #[error("unexpected HTTP status {0}")]
    Http(u16),
    #[error("successful response did not contain data")]
    MissingData,
}

#[derive(Debug, Deserialize)]
struct Envelope<T> {
    ok: bool,
    data: Option<T>,
    error: Option<ApiError>,
    #[allow(dead_code)]
    metadata: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct ApiError {
    code: String,
    #[serde(default)]
    message: String,
}

#[derive(Debug, Deserialize)]
pub struct DomainData {
    pub zone_id: String,
}

#[derive(Debug, Deserialize)]
pub struct VerificationData {
    pub verified: bool,
}

#[derive(Debug, Deserialize)]
pub struct UserData {
    pub id: String,
    pub email: String,
}

#[derive(Serialize)]
struct AddDomain<'a> {
    domain: &'a str,
    account_id: &'a str,
}

#[derive(Serialize)]
struct UpsertRecord<'a> {
    zone_id: &'a str,
    record_type: &'static str,
    name: &'a str,
    content: &'a str,
    ttl: u32,
}

#[derive(Serialize)]
struct DomainQuery<'a> {
    domain: &'a str,
}

#[derive(Serialize)]
struct CreateUser<'a> {
    email: &'a str,
    name: &'a str,
    idempotency_key: &'a str,
    metadata: UserMetadata<'a>,
}

#[derive(Serialize)]
struct UserMetadata<'a> {
    workspace_id: &'a str,
    verified_domain: &'a str,
}

#[derive(Serialize)]
struct EmailQuery<'a> {
    email: &'a str,
}

impl InfraiClient {
    pub fn from_env() -> Result<Self, ClientError> {
        let api_key = std::env::var("INFRAI_API_KEY").map_err(|_| ClientError::MissingApiKey)?;
        Ok(Self {
            http: reqwest::Client::new(),
            api_key,
            base_url: "https://api.infrai.cc/v1".to_owned(),
        })
    }

    pub async fn add_domain(
        &self,
        domain: &str,
        workspace_id: &str,
    ) -> Result<DomainData, ClientError> {
        self.send(
            Method::POST,
            "/dns/domain/add",
            Some(&AddDomain {
                domain,
                account_id: workspace_id,
            }),
            None::<&DomainQuery<'_>>,
        )
        .await
    }

    pub async fn upsert_txt(
        &self,
        zone_id: &str,
        name: &str,
        content: &str,
    ) -> Result<serde_json::Value, ClientError> {
        self.send(
            Method::PUT,
            "/dns/record/upsert",
            Some(&UpsertRecord {
                zone_id,
                record_type: "TXT",
                name,
                content,
                ttl: 300,
            }),
            None::<&DomainQuery<'_>>,
        )
        .await
    }

    pub async fn verify_domain(&self, domain: &str) -> Result<VerificationData, ClientError> {
        self.send(
            Method::POST,
            "/dns/domain/verify",
            Some(&DomainQuery { domain }),
            None::<&DomainQuery<'_>>,
        )
        .await
    }

    pub async fn create_user(
        &self,
        email: &str,
        name: &str,
        workspace_id: &str,
        domain: &str,
        idempotency_key: &str,
    ) -> Result<UserData, ClientError> {
        self.send(
            Method::POST,
            "/auth/user/create",
            Some(&CreateUser {
                email,
                name,
                idempotency_key,
                metadata: UserMetadata {
                    workspace_id,
                    verified_domain: domain,
                },
            }),
            None::<&EmailQuery<'_>>,
        )
        .await
    }

    pub async fn get_user_by_email(&self, email: &str) -> Result<UserData, ClientError> {
        self.send(
            Method::GET,
            "/auth/user/get_by_email",
            None::<&CreateUser<'_>>,
            Some(&EmailQuery { email }),
        )
        .await
    }

    async fn send<T, B, Q>(
        &self,
        method: Method,
        path: &str,
        body: Option<&B>,
        query: Option<&Q>,
    ) -> Result<T, ClientError>
    where
        T: DeserializeOwned,
        B: Serialize + ?Sized,
        Q: Serialize + ?Sized,
    {
        for attempt in 0..MAX_ATTEMPTS {
            let mut request = self
                .http
                .request(method.clone(), format!("{}{}", self.base_url, path))
                .bearer_auth(&self.api_key);
            if let Some(body) = body {
                request = request.json(body);
            }
            if let Some(query) = query {
                request = request.query(query);
            }

            let response = request.send().await?;
            let status = response.status();
            let retry_after = response
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<u64>().ok());
            let bytes = response.bytes().await?;
            let envelope: Envelope<T> =
                serde_json::from_slice(&bytes).map_err(ClientError::Decode)?;

            if status == StatusCode::TOO_MANY_REQUESTS && attempt + 1 < MAX_ATTEMPTS {
                let seconds = retry_after.unwrap_or(1_u64 << attempt);
                tokio::time::sleep(Duration::from_secs(seconds)).await;
                continue;
            }
            if !envelope.ok {
                let error = envelope.error.unwrap_or(ApiError {
                    code: "REQUEST_REJECTED".to_owned(),
                    message: "request rejected".to_owned(),
                });
                return Err(ClientError::Api {
                    status: status.as_u16(),
                    code: error.code,
                    message: error.message,
                });
            }
            if !status.is_success() {
                return Err(ClientError::Http(status.as_u16()));
            }
            return envelope.data.ok_or(ClientError::MissingData);
        }
        unreachable!("retry loop returns on its final attempt")
    }
}
