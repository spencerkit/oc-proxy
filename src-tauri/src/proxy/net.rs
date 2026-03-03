use std::net::{IpAddr, UdpSocket};
use tokio::net::TcpListener;

fn normalized_host(host: &str) -> &str {
    host.trim().trim_start_matches('[').trim_end_matches(']')
}

fn format_bind_target(host: &str, port: u16) -> String {
    if host.contains(':') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    }
}

fn bind_candidates(host: &str) -> Vec<String> {
    let host = normalized_host(host);
    let mut candidates = match host {
        // Prefer IPv4 wildcard first for best compatibility on Windows.
        "0.0.0.0" => vec!["0.0.0.0".to_string(), "::".to_string()],
        "::" => vec!["::".to_string(), "0.0.0.0".to_string()],
        "localhost" => vec!["127.0.0.1".to_string(), "::1".to_string()],
        _ => vec![host.to_string()],
    };
    candidates.dedup();
    candidates
}

pub(super) fn public_host_for_status(bound_host: &str) -> String {
    match bound_host {
        "0.0.0.0" => "127.0.0.1".to_string(),
        "::" => "[::1]".to_string(),
        _ if bound_host.contains(':') => format!("[{bound_host}]"),
        _ => bound_host.to_string(),
    }
}

pub(super) fn detect_local_ipv4() -> Option<String> {
    // Use routing table resolution to infer the primary LAN IPv4 of this machine.
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    match socket.local_addr().ok()?.ip() {
        IpAddr::V4(ipv4) if !ipv4.is_loopback() => Some(ipv4.to_string()),
        _ => None,
    }
}

pub(super) async fn bind_proxy_listener(
    host: &str,
    port: u16,
) -> Result<(TcpListener, String), String> {
    let mut errors = Vec::new();
    for candidate in bind_candidates(host) {
        let target = format_bind_target(&candidate, port);
        match TcpListener::bind(&target).await {
            Ok(listener) => return Ok((listener, candidate)),
            Err(err) => errors.push(format!("{target}: {err}")),
        }
    }
    Err(format!("bind proxy server failed: {}", errors.join(" | ")))
}
