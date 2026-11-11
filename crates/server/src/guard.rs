//! Request checks that keep other web pages and other machines out: the `Host` header must be
//! one of ours (DNS rebinding), and a write that names an `Origin` must come from our own page.

use std::net::IpAddr;

use axum::extract::{Request, State};
use axum::http::Method;
use axum::http::header::{HOST, ORIGIN};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::AppState;
use crate::error::ApiError;

/// The `Host` values a request may name: loopback, the names the server was bound to or told
/// about, and, when it listens on every interface, any IP literal.
#[derive(Debug, Clone, Default)]
pub(crate) struct AllowedHosts {
    /// Bound to `0.0.0.0` or `::`: the socket answers on every address, so any IP literal is ours.
    pub any_ip: bool,
    /// Names besides `localhost`, compared without case.
    pub names: Vec<String>,
}

impl AllowedHosts {
    /// The rules for a server listening on `bound`, asked for by name `asked`, plus `allow` names.
    /// An unspecified bind is never itself a name: `0.0.0.0` is not a `Host` a browser sends.
    pub fn for_bind(bound: IpAddr, asked: &str, allow: &[String]) -> Self {
        let mut names = allow.to_vec();
        if !bound.is_unspecified() {
            names.push(asked.to_string());
            names.push(bound.to_string());
        }
        AllowedHosts {
            any_ip: bound.is_unspecified(),
            names,
        }
    }
}

/// The host part of a `Host` value or URL authority: no port, no IPv6 brackets.
fn hostname(authority: &str) -> &str {
    if let Some(rest) = authority.strip_prefix('[') {
        return rest.split(']').next().unwrap_or(rest);
    }
    authority.rsplit_once(':').map_or(authority, |(host, _)| host)
}

/// `localhost`, a loopback IP literal, an IP literal when the server listens on every interface,
/// or a listed name. A hostname that is not listed never passes: that is the DNS-rebinding guard.
pub(crate) fn host_allowed(host: &str, allowed: &AllowedHosts) -> bool {
    let host = hostname(host).to_ascii_lowercase();
    if host == "localhost" {
        return true;
    }
    match host.parse::<IpAddr>() {
        Ok(ip) if ip.is_loopback() || allowed.any_ip => true,
        _ => allowed.names.iter().any(|name| name.eq_ignore_ascii_case(&host)),
    }
}

fn is_write(method: &Method) -> bool {
    !matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS)
}

pub(crate) async fn guard(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let host = req
        .headers()
        .get(HOST)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
        .or_else(|| req.uri().authority().map(|a| a.to_string()));
    let Some(host) = host else {
        return ApiError::forbidden("host not allowed").into_response();
    };
    if !host_allowed(&host, &state.hosts) {
        return ApiError::forbidden("host not allowed").into_response();
    }
    if is_write(req.method())
        && let Some(origin) = req.headers().get(ORIGIN)
    {
        // The page that writes must be ours by the same rules the `Host` passed.
        let same_site = origin
            .to_str()
            .ok()
            .and_then(|origin| origin.split_once("://"))
            .is_some_and(|(_, authority)| host_allowed(authority, &state.hosts));
        if !same_site {
            return ApiError::forbidden("cross-site writes are not allowed").into_response();
        }
    }
    next.run(req).await
}
