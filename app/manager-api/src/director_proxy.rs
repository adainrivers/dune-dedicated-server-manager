use anyhow::{anyhow, Context};
use axum::{
    body::{Body, Bytes},
    http::{header, Method, StatusCode},
    response::Response,
    Json,
};
use k8s_openapi::api::core::v1::Service;
use kube::{api::ListParams, Api};
use serde_json::{json, Value};

use crate::{
    errors::{ApiError, ApiResponse},
    security::redact_json,
    state::AppState,
};

pub async fn director_get_json(state: &AppState, path: &str) -> Result<Value, ApiError> {
    let base_url = resolve_director_base_url(state).await?;
    let value = state
        .http
        .get(format!("{base_url}{path}"))
        .send()
        .await
        .context("failed to call Director")?
        .error_for_status()
        .context("Director returned an error")?
        .json::<Value>()
        .await
        .context("failed to parse Director response")?;
    Ok(value)
}

pub async fn proxy_director_json(
    state: &AppState,
    method: Method,
    path: &str,
    query: Option<String>,
    body: Bytes,
) -> ApiResponse<Value> {
    let response = proxy_director_request(state, method, path, query, body).await?;
    let status = response.status();
    let bytes = response
        .bytes()
        .await
        .context("failed to read Director response")?;
    if !status.is_success() {
        return Err(ApiError {
            status: StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY),
            message: String::from_utf8_lossy(&bytes).to_string(),
        });
    }
    if bytes.is_empty() {
        return Ok(Json(json!({ "ok": true })));
    }
    let value = serde_json::from_slice::<Value>(&bytes).unwrap_or_else(|_| {
        json!({
            "ok": true,
            "body": String::from_utf8_lossy(&bytes)
        })
    });
    Ok(Json(redact_json(value)))
}

pub async fn proxy_director_response(
    state: &AppState,
    method: Method,
    path: &str,
    query: Option<String>,
    body: Bytes,
    set_auth_cookie: Option<&str>,
) -> Result<Response, ApiError> {
    let response = proxy_director_request(state, method, path, query, body).await?;
    let status =
        StatusCode::from_u16(response.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let content_type = response.headers().get(header::CONTENT_TYPE).cloned();
    let cache_control = response.headers().get(header::CACHE_CONTROL).cloned();
    let bytes = response
        .bytes()
        .await
        .context("failed to read Director response body")?;

    let mut builder = Response::builder().status(status);
    if let Some(value) = content_type {
        builder = builder.header(header::CONTENT_TYPE, value);
    }
    if let Some(value) = cache_control {
        builder = builder.header(header::CACHE_CONTROL, value);
    }
    if let Some(token) = set_auth_cookie {
        builder = builder.header(
            header::SET_COOKIE,
            format!("dune_manager_token={token}; HttpOnly; SameSite=Lax; Path=/; Max-Age=86400"),
        );
    }

    builder
        .body(Body::from(bytes))
        .map_err(|err| ApiError::from(anyhow!(err)))
}

pub async fn resolve_director_base_url(state: &AppState) -> Result<String, ApiError> {
    if let Some(url) = state.director_base_url.as_deref() {
        return Ok(url.to_string());
    }
    discover_director_base_url(state).await
}

async fn proxy_director_request(
    state: &AppState,
    method: Method,
    path: &str,
    query: Option<String>,
    body: Bytes,
) -> Result<reqwest::Response, ApiError> {
    let base_url = resolve_director_base_url(state).await?;
    let mut url = format!("{base_url}{path}");
    if let Some(query) = query.filter(|value| !value.is_empty()) {
        url.push('?');
        url.push_str(&query);
    }

    let reqwest_method = reqwest::Method::from_bytes(method.as_str().as_bytes())
        .map_err(|_| ApiError::bad_request("unsupported HTTP method"))?;
    let mut request = state.http.request(reqwest_method, url);
    if !body.is_empty() {
        request = request
            .header(header::CONTENT_TYPE.as_str(), "application/json")
            .body(body);
    }
    request
        .send()
        .await
        .context("failed to proxy Director request")
        .map_err(ApiError::from)
}

async fn discover_director_base_url(state: &AppState) -> Result<String, ApiError> {
    let services: Api<Service> = Api::namespaced(state.client.clone(), &state.namespace);
    let list = services
        .list(&ListParams::default())
        .await
        .context("failed to list services for Director discovery")?;

    for service in list {
        let name = service.metadata.name.clone().unwrap_or_default();
        let Some(spec) = service.spec else {
            continue;
        };
        let Some(ports) = spec.ports else {
            continue;
        };
        for port in ports {
            let is_director = port.port == 11717
                || port
                    .name
                    .as_deref()
                    .unwrap_or_default()
                    .contains("director")
                || name.contains("director");
            if !is_director {
                continue;
            }
            if let Some(node_port) = port.node_port {
                return Ok(format!("http://127.0.0.1:{node_port}"));
            }
            if let Some(cluster_ip) = spec.cluster_ip.as_deref().filter(|value| !value.is_empty()) {
                return Ok(format!("http://{cluster_ip}:{}", port.port));
            }
            if !name.is_empty() {
                return Ok(format!(
                    "http://{name}.{}.svc.cluster.local:{}",
                    state.namespace, port.port
                ));
            }
        }
    }
    Err(ApiError::bad_gateway(
        "DIRECTOR_BASE_URL is not configured and Director service discovery failed",
    ))
}
