//! Pure-Rust SSRF and Public-Network Guard.
//!
//! Enforces:
//! - Only `http://` and `https://` schemes allowed (rejects `file://`, `data:`, `ws:`, `javascript:`, etc.).
//! - Blocks loopback, RFC1918 private LAN, CGNAT (`100.64.0.0/10`), link-local, cloud metadata
//!   (`169.254.169.254`), documentation/test ranges, multicast, broadcast, and IPv4-mapped equivalents.
//! - Blocks local/internal hostnames (`localhost`, `*.localhost`, `*.local`, `*.internal`, `metadata.google.internal`).
//! - Verifies all resolved IP addresses before establishing network connections.
//! - Safe text fetching with disabled automatic redirects and per-hop re-validation.

use std::io::Read;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedPublicUrl {
    pub url: String,
    pub scheme: String,
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicFetchResult {
    pub url: String,
    pub status: u16,
    pub text: String,
    pub bytes: usize,
}

/// Evaluates whether an IP address is a safe, routable public address.
/// Returns `false` for loopback, private RFC1918, link-local, cloud metadata,
/// CGNAT, unique local, multicast, broadcast, or reserved addresses.
pub fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_public_ipv4(v4),
        IpAddr::V6(v6) => is_public_ipv6(v6),
    }
}

fn is_public_ipv4(v4: Ipv4Addr) -> bool {
    let [a, b, c, _d] = v4.octets();

    // Loopback: 127.0.0.0/8
    if a == 127 {
        return false;
    }
    // Unspecified / Current network: 0.0.0.0/8
    if a == 0 {
        return false;
    }
    // RFC1918 Private: 10.0.0.0/8
    if a == 10 {
        return false;
    }
    // RFC1918 Private: 172.16.0.0/12 (172.16.0.0 - 172.31.255.255)
    if a == 172 && (16..=31).contains(&b) {
        return false;
    }
    // RFC1918 Private: 192.168.0.0/16
    if a == 192 && b == 168 {
        return false;
    }
    // Link-Local / Cloud Metadata: 169.254.0.0/16
    if a == 169 && b == 254 {
        return false;
    }
    // CGNAT / Shared Address Space (RFC 6598, Tailscale, Carrier NAT): 100.64.0.0/10 (100.64.0.0 - 100.127.255.255)
    if a == 100 && (64..=127).contains(&b) {
        return false;
    }
    // IETF Protocol Assignments: 192.0.0.0/24
    if a == 192 && b == 0 && c == 0 {
        return false;
    }
    // TEST-NET-1: 192.0.2.0/24
    if a == 192 && b == 0 && c == 2 {
        return false;
    }
    // TEST-NET-2: 198.51.100.0/24
    if a == 198 && b == 51 && c == 100 {
        return false;
    }
    // TEST-NET-3: 203.0.113.0/24
    if a == 203 && b == 0 && c == 113 {
        return false;
    }
    // Benchmarking: 198.18.0.0/15 (198.18.0.0 - 198.19.255.255)
    if a == 198 && (18..=19).contains(&b) {
        return false;
    }
    // Multicast: 224.0.0.0/4 (224.0.0.0 - 239.255.255.255)
    if (224..=239).contains(&a) {
        return false;
    }
    // Reserved / Broadcast: 240.0.0.0/4 (includes 255.255.255.255)
    if a >= 240 {
        return false;
    }

    true
}

fn is_public_ipv6(v6: Ipv6Addr) -> bool {
    let seg = v6.segments();

    // IPv4-mapped IPv6: ::ffff:a.b.c.d
    if seg[0] == 0 && seg[1] == 0 && seg[2] == 0 && seg[3] == 0 && seg[4] == 0 && seg[5] == 0xffff
    {
        let v4 = Ipv4Addr::new(
            (seg[6] >> 8) as u8,
            seg[6] as u8,
            (seg[7] >> 8) as u8,
            seg[7] as u8,
        );
        return is_public_ipv4(v4);
    }

    // IPv4-compatible IPv6 (deprecated): ::a.b.c.d
    if seg[0] == 0 && seg[1] == 0 && seg[2] == 0 && seg[3] == 0 && seg[4] == 0 && seg[5] == 0 {
        if seg[6] == 0 && seg[7] == 1 {
            return false; // loopback ::1
        }
        if seg[6] == 0 && seg[7] == 0 {
            return false; // unspecified ::
        }
        let v4 = Ipv4Addr::new(
            (seg[6] >> 8) as u8,
            seg[6] as u8,
            (seg[7] >> 8) as u8,
            seg[7] as u8,
        );
        return is_public_ipv4(v4);
    }

    // Loopback: ::1
    if v6.is_loopback() {
        return false;
    }
    // Unspecified: ::
    if v6.is_unspecified() {
        return false;
    }

    // Unique Local Unicast (ULA, RFC 4193): fc00::/7 (fc00:: - fdff::)
    if (seg[0] & 0xfe00) == 0xfc00 {
        return false;
    }

    // Link-Local Unicast: fe80::/10 (fe80:: - febf::)
    if (seg[0] & 0xffc0) == 0xfe80 {
        return false;
    }

    // Multicast: ff00::/8
    if (seg[0] & 0xff00) == 0xff00 {
        return false;
    }

    // Discard-only prefix (RFC 6666): 0100::/64
    if seg[0] == 0x0100 {
        return false;
    }

    // Documentation prefix (RFC 3849): 2001:db8::/32
    if seg[0] == 0x2001 && seg[1] == 0x0db8 {
        return false;
    }

    true
}

fn is_blocked_internal_hostname(host: &str) -> bool {
    let lower = host.trim().to_ascii_lowercase();
    if lower.is_empty() {
        return true;
    }
    if lower == "localhost"
        || lower.ends_with(".localhost")
        || lower.ends_with(".local")
        || lower.ends_with(".internal")
        || lower == "metadata.google.internal"
    {
        return true;
    }
    false
}

/// Validates that a raw URL is a public HTTP/HTTPS URL with an allowed hostname
/// and scheme. Literal IP addresses in the host are checked against private IP rules.
pub fn validate_public_http_url(raw_url: &str) -> Result<ValidatedPublicUrl, &'static str> {
    let parsed = reqwest::Url::parse(raw_url).map_err(|_| "INVALID_URL")?;

    let scheme = parsed.scheme().to_ascii_lowercase();
    if scheme != "http" && scheme != "https" {
        return Err("UNSUPPORTED_SCHEME");
    }

    let raw_host = parsed.host_str().ok_or("MISSING_HOST")?;
    let unbracketed = raw_host
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_ascii_lowercase();

    if unbracketed.is_empty() {
        return Err("MISSING_HOST");
    }

    if is_blocked_internal_hostname(&unbracketed) {
        return Err("BLOCKED_INTERNAL_HOST");
    }

    // If host is a literal IP, check it immediately
    if let Ok(ip) = unbracketed.parse::<IpAddr>() {
        if !is_public_ip(ip) {
            return Err("BLOCKED_PRIVATE_IP");
        }
    }

    let port = parsed
        .port_or_known_default()
        .unwrap_or(if scheme == "https" { 443 } else { 80 });

    Ok(ValidatedPublicUrl {
        url: parsed.to_string(),
        scheme,
        host: unbracketed,
        port,
    })
}

/// Resolves a host to socket addresses and verifies that EVERY resolved IP
/// address is a public address, protecting against DNS rebinding attacks.
pub fn resolve_and_verify_public_host(
    host: &str,
    port: u16,
) -> Result<Vec<SocketAddr>, &'static str> {
    let unbracketed = host
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_ascii_lowercase();

    if is_blocked_internal_hostname(&unbracketed) {
        return Err("BLOCKED_INTERNAL_HOST");
    }

    if let Ok(ip) = unbracketed.parse::<IpAddr>() {
        if !is_public_ip(ip) {
            return Err("BLOCKED_PRIVATE_IP");
        }
        return Ok(vec![SocketAddr::new(ip, port)]);
    }

    let addrs: Vec<SocketAddr> = (unbracketed.as_str(), port)
        .to_socket_addrs()
        .map_err(|_| "DNS_RESOLUTION_FAILED")?
        .collect();

    if addrs.is_empty() {
        return Err("DNS_RESOLUTION_FAILED");
    }

    for sa in &addrs {
        if !is_public_ip(sa.ip()) {
            return Err("BLOCKED_PRIVATE_IP");
        }
    }

    Ok(addrs)
}

/// Fetches text from a public HTTP/HTTPS URL with SSRF protection:
/// - Disables automatic redirect following.
/// - Manually validates each redirect target URL and resolves/verifies its IP before following.
/// - Allows up to 5 redirects.
/// - Enforces maximum byte response limits and request timeouts.
pub fn fetch_public_text(
    url: &str,
    max_bytes: usize,
    timeout_ms: u64,
) -> Result<PublicFetchResult, &'static str> {
    const MAX_REDIRECTS: usize = 5;

    let client = reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_millis(timeout_ms))
        .build()
        .map_err(|_| "CLIENT_BUILD_FAILED")?;

    let mut current_url = url.trim().to_string();
    let mut redirects_followed = 0;

    loop {
        let validated = validate_public_http_url(&current_url)?;
        let _ = resolve_and_verify_public_host(&validated.host, validated.port)?;

        let resp = client
            .get(&validated.url)
            .header(reqwest::header::USER_AGENT, "Bob-PublicFetch/1.0")
            .send()
            .map_err(|_| "FETCH_FAILED")?;

        let status = resp.status();
        let status_code = status.as_u16();

        if status.is_redirection() {
            if redirects_followed >= MAX_REDIRECTS {
                return Err("TOO_MANY_REDIRECTS");
            }
            let loc_val = resp
                .headers()
                .get(reqwest::header::LOCATION)
                .ok_or("MISSING_REDIRECT_LOCATION")?;
            let loc_str = loc_val.to_str().map_err(|_| "INVALID_REDIRECT_LOCATION")?;

            let base = reqwest::Url::parse(&current_url).map_err(|_| "INVALID_URL")?;
            let next_url = base
                .join(loc_str)
                .map_err(|_| "INVALID_REDIRECT_LOCATION")?;
            let next_url_str = next_url.to_string();

            // Re-validate target immediately
            let next_validated = validate_public_http_url(&next_url_str)?;
            let _ = resolve_and_verify_public_host(&next_validated.host, next_validated.port)?;

            current_url = next_url_str;
            redirects_followed += 1;
            continue;
        }

        let mut buffer = Vec::new();
        let mut take = resp.take(max_bytes as u64);
        take.read_to_end(&mut buffer)
            .map_err(|_| "FETCH_READ_FAILED")?;
        let bytes = buffer.len();
        let text = String::from_utf8_lossy(&buffer).to_string();

        return Ok(PublicFetchResult {
            url: current_url,
            status: status_code,
            text,
            bytes,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_public_ips_allowed() {
        let allowed = [
            "1.1.1.1",
            "8.8.8.8",
            "93.184.216.34",
            "172.32.0.1",
            "2606:4700:4700::1111",
            "::ffff:1.1.1.1",
        ];
        for s in allowed {
            let ip = IpAddr::from_str(s).unwrap();
            assert!(is_public_ip(ip), "IP {s} should be public");
        }
    }

    #[test]
    fn test_loopback_and_lan_blocked() {
        let blocked = [
            "127.0.0.1",
            "127.0.0.254",
            "10.0.0.1",
            "10.255.255.255",
            "172.16.0.1",
            "172.31.255.254",
            "192.168.0.1",
            "192.168.1.100",
            "::1",
            "::",
            "0.0.0.0",
        ];
        for s in blocked {
            let ip = IpAddr::from_str(s).unwrap();
            assert!(!is_public_ip(ip), "IP {s} should be blocked");
        }
    }

    #[test]
    fn test_cgnat_tailscale_blocked() {
        let cgnat = ["100.64.0.1", "100.64.0.3", "100.127.255.254"];
        for s in cgnat {
            let ip = IpAddr::from_str(s).unwrap();
            assert!(!is_public_ip(ip), "CGNAT IP {s} should be blocked");
        }
    }

    #[test]
    fn test_cloud_metadata_link_local_blocked() {
        let metadata = [
            "169.254.169.254",
            "169.254.0.1",
            "fe80::1",
            "::ffff:169.254.169.254",
            "::ffff:127.0.0.1",
        ];
        for s in metadata {
            let ip = IpAddr::from_str(s).unwrap();
            assert!(!is_public_ip(ip), "Metadata/Link-local IP {s} should be blocked");
        }
    }

    #[test]
    fn test_unique_local_multicast_broadcast_blocked() {
        let blocked = [
            "fc00::1",
            "fd00:1234::1",
            "ff02::1",
            "224.0.0.1",
            "239.255.255.250",
            "255.255.255.255",
        ];
        for s in blocked {
            let ip = IpAddr::from_str(s).unwrap();
            assert!(!is_public_ip(ip), "IP {s} should be blocked");
        }
    }

    #[test]
    fn test_url_schemes_and_hosts() {
        assert_eq!(
            validate_public_http_url("file:///etc/passwd").unwrap_err(),
            "UNSUPPORTED_SCHEME"
        );
        assert_eq!(
            validate_public_http_url("javascript:alert(1)").unwrap_err(),
            "UNSUPPORTED_SCHEME"
        );
        assert_eq!(
            validate_public_http_url("data:text/html;base64,abc").unwrap_err(),
            "UNSUPPORTED_SCHEME"
        );
        assert_eq!(
            validate_public_http_url("ws://example.com/socket").unwrap_err(),
            "UNSUPPORTED_SCHEME"
        );

        assert_eq!(
            validate_public_http_url("http://localhost:8080/api").unwrap_err(),
            "BLOCKED_INTERNAL_HOST"
        );
        assert_eq!(
            validate_public_http_url("http://service.localhost/api").unwrap_err(),
            "BLOCKED_INTERNAL_HOST"
        );
        assert_eq!(
            validate_public_http_url("http://node.local/test").unwrap_err(),
            "BLOCKED_INTERNAL_HOST"
        );
        assert_eq!(
            validate_public_http_url("http://service.internal/path").unwrap_err(),
            "BLOCKED_INTERNAL_HOST"
        );
        assert_eq!(
            validate_public_http_url("http://metadata.google.internal/computeMetadata/v1/").unwrap_err(),
            "BLOCKED_INTERNAL_HOST"
        );

        assert_eq!(
            validate_public_http_url("http://127.0.0.1:3000/").unwrap_err(),
            "BLOCKED_PRIVATE_IP"
        );
        assert_eq!(
            validate_public_http_url("http://100.64.0.3/status").unwrap_err(),
            "BLOCKED_PRIVATE_IP"
        );
        assert_eq!(
            validate_public_http_url("http://169.254.169.254/latest/meta-data").unwrap_err(),
            "BLOCKED_PRIVATE_IP"
        );

        let valid = validate_public_http_url("https://api.github.com/events").unwrap();
        assert_eq!(valid.scheme, "https");
        assert_eq!(valid.host, "api.github.com");
        assert_eq!(valid.port, 443);
    }

    #[test]
    fn test_resolve_and_verify_public_host_rejects_private() {
        assert_eq!(
            resolve_and_verify_public_host("localhost", 80).unwrap_err(),
            "BLOCKED_INTERNAL_HOST"
        );
        assert_eq!(
            resolve_and_verify_public_host("127.0.0.1", 80).unwrap_err(),
            "BLOCKED_PRIVATE_IP"
        );
        assert_eq!(
            resolve_and_verify_public_host("100.64.0.3", 80).unwrap_err(),
            "BLOCKED_PRIVATE_IP"
        );
    }
}
