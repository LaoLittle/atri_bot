//! Authenticated loopback-only HTTP/CBOR management transport.
use crate::{Runtime, runtime::Inner, storage::read_control_metadata};
use atri_core::*;
use axum::{
    Router,
    body::Bytes,
    extract::{DefaultBodyLimit, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::post,
};
use serde::{Deserialize, Serialize};
use std::{
    net::SocketAddr,
    path::Path,
    sync::{Arc, Weak},
    time::Duration,
};
use tokio::{net::TcpListener, task::JoinHandle};
use tokio_util::sync::CancellationToken;
#[derive(Clone, Serialize, Deserialize)]
struct Endpoint {
    address: String,
    token: String,
    session: String,
}
#[derive(Clone)]
struct ServerState {
    runtime: Weak<Inner>,
    token: Arc<str>,
}
pub(crate) struct Server {
    cancel: CancellationToken,
    job: JoinHandle<()>,
}
impl Server {
    pub fn finish_response(self) {
        self.cancel.cancel();
        // The bounded reaper joins the server after the stop request has sent its
        // actual result. Awaiting that join inside the handler would self-deadlock.
        tokio::spawn(async move {
            self.stop().await;
        });
    }
    pub async fn stop(mut self) {
        self.cancel.cancel();
        if tokio::time::timeout(Duration::from_secs(5), &mut self.job)
            .await
            .is_err()
        {
            self.job.abort();
            let _ = self.job.await;
        }
    }
}
pub(crate) async fn serve(runtime: Runtime) -> Result<Server, ApiError> {
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .map_err(ApiError::internal)?;
    let endpoint = Endpoint {
        address: listener
            .local_addr()
            .map_err(ApiError::internal)?
            .to_string(),
        token: hex::encode(rand::random::<[u8; 32]>()),
        session: uuid::Uuid::new_v4().to_string(),
    };
    let state = ServerState {
        runtime: Arc::downgrade(&runtime.inner),
        token: Arc::from(endpoint.token.as_str()),
    };
    let router = Router::new()
        .route(&format!("/v1/{}", endpoint.session), post(handle))
        .layer(DefaultBodyLimit::max(MAX_CALL_BYTES))
        .with_state(state);
    let cancel = CancellationToken::new();
    let stop = cancel.clone();
    let job = tokio::spawn(async move {
        let _ = axum::serve(listener, router)
            .with_graceful_shutdown(stop.cancelled_owned())
            .await;
    });
    if let Err(error) = runtime
        .inner
        .storage
        .set_control_metadata(Some(to_value(&endpoint)?))
        .await
    {
        cancel.cancel();
        let _ = job.await;
        return Err(error);
    }
    Ok(Server { cancel, job })
}
fn authorized(headers: &HeaderMap, token: &str) -> bool {
    let Some(value) = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
    else {
        return false;
    };
    if value.len() != token.len() {
        return false;
    }
    value
        .bytes()
        .zip(token.bytes())
        .fold(0u8, |difference, (a, b)| difference | (a ^ b))
        == 0
}
async fn handle(
    State(state): State<ServerState>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    if !authorized(&headers, &state.token) {
        return (
            StatusCode::UNAUTHORIZED,
            [("content-type", "application/cbor")],
            Vec::new(),
        );
    }
    let result = match decode::<Request>(&body) {
        Err(error) => Err(error),
        Ok(request) => match state.runtime.upgrade() {
            None => Err(ApiError::cancelled()),
            Some(inner) => {
                let runtime = Runtime { inner };
                if request.method == "runtime.stop" {
                    runtime.shutdown_from_control().await.map(|()| Value::Null)
                } else {
                    runtime.control(request).await
                }
            }
        },
    };
    match encode(&result) {
        Ok(bytes) => (
            StatusCode::OK,
            [("content-type", "application/cbor")],
            bytes,
        ),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            [("content-type", "application/cbor")],
            Vec::new(),
        ),
    }
}
#[derive(Clone)]
pub struct ControlClient {
    client: reqwest::Client,
    endpoint: Endpoint,
}
impl ControlClient {
    pub async fn connect(bot_dir: &Path) -> Result<Option<Self>, ApiError> {
        let Some(value) = read_control_metadata(bot_dir).await? else {
            return Ok(None);
        };
        let endpoint: Endpoint = from_value(&value)?;
        let address: SocketAddr = endpoint.address.parse().map_err(ApiError::internal)?;
        if !address.ip().is_loopback()
            || endpoint.token.len() != 64
            || endpoint.session.is_empty()
            || !endpoint
                .session
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err(ApiError::new(
                "invalid_control_endpoint",
                "control endpoint is not a private loopback session",
            ));
        }
        let client = reqwest::Client::builder()
            .no_proxy()
            .connect_timeout(Duration::from_secs(2))
            .timeout(Duration::from_secs(180))
            .build()
            .map_err(ApiError::internal)?;
        let client = Self { client, endpoint };
        match client
            .request(Request::new("runtime.status", empty_map())?)
            .await
        {
            Ok(_) => Ok(Some(client)),
            Err(error) if error.code == "control_unreachable" => Ok(None),
            Err(error) => Err(error),
        }
    }
    pub async fn request(&self, request: Request) -> Result<Value, ApiError> {
        let url = format!(
            "http://{}/v1/{}",
            self.endpoint.address, self.endpoint.session
        );
        let mut response = self
            .client
            .post(url)
            .bearer_auth(&self.endpoint.token)
            .header("content-type", "application/cbor")
            .body(encode(&request)?)
            .send()
            .await
            .map_err(|e| {
                ApiError::new(
                    if e.is_connect() {
                        "control_unreachable"
                    } else {
                        "control_transport"
                    },
                    e.to_string(),
                )
            })?;
        if response.status() != reqwest::StatusCode::OK {
            return Err(ApiError::new(
                "control_http",
                format!("control server returned {}", response.status()),
            ));
        }
        if response
            .content_length()
            .is_some_and(|len| len > MAX_CALL_BYTES as u64)
        {
            return Err(ApiError::new(
                "payload_too_large",
                "control response exceeds limit",
            ));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(ApiError::internal)? {
            if bytes
                .len()
                .checked_add(chunk.len())
                .is_none_or(|len| len > MAX_CALL_BYTES)
            {
                return Err(ApiError::new(
                    "payload_too_large",
                    "control response exceeds limit",
                ));
            }
            bytes.extend_from_slice(&chunk);
        }
        decode::<Result<Value, ApiError>>(&bytes)?
    }
}
