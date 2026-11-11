use crate::error::{BrowserError, BrowserResult};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, ToSocketAddrs};

#[derive(Debug, Clone)]
pub struct BrowserSecurityPolicy {
    pub allow_loopback_for_tests: bool,
    pub allow_internal_blank: bool,
}

impl Default for BrowserSecurityPolicy {
    fn default() -> Self {
        Self {
            allow_loopback_for_tests: false,
            allow_internal_blank: true,
        }
    }
}

impl BrowserSecurityPolicy {
    pub fn test_policy() -> Self {
        Self {
            allow_loopback_for_tests: true,
            allow_internal_blank: true,
        }
    }
}

/// Checks if an IPv4 address falls into restricted/private/SSRF-prone CIDRs.
pub fn is_ipv4_blocked(ip: Ipv4Addr, allow_loopback: bool) -> Option<&'static str> {
    let octets = ip.octets();

    // Loopback: 127.0.0.0/8
    if octets[0] == 127 {
        if allow_loopback {
            return None;
        }
        return Some("Loopback (127.0.0.0/8)");
    }

    // Unspecified / Current network: 0.0.0.0/8
    if octets[0] == 0 {
        return Some("Unspecified / Current network (0.0.0.0/8)");
    }

    // Private Class A: 10.0.0.0/8
    if octets[0] == 10 {
        return Some("Private network (10.0.0.0/8)");
    }

    // Carrier-Grade NAT: 100.64.0.0/10 (100.64.0.0 - 100.127.255.255)
    if octets[0] == 100 && (octets[1] & 0xC0) == 64 {
        return Some("Carrier-Grade NAT (100.64.0.0/10)");
    }

    // Link-local / Cloud metadata: 169.254.0.0/16
    if octets[0] == 169 && octets[1] == 254 {
        return Some("Link-local / Cloud metadata (169.254.0.0/16)");
    }

    // Private Class B: 172.16.0.0/12 (172.16.0.0 - 172.31.255.255)
    if octets[0] == 172 && (16..=31).contains(&octets[1]) {
        return Some("Private network (172.16.0.0/12)");
    }

    // IETF Protocol / Documentation: 192.0.0.0/24, 192.0.2.0/24 (TEST-NET-1)
    if octets[0] == 192 && octets[1] == 0 && (octets[2] == 0 || octets[2] == 2) {
        return Some("Reserved / Test network (192.0.0.0/24 or 192.0.2.0/24)");
    }

    // Private Class C: 192.168.0.0/16
    if octets[0] == 192 && octets[1] == 168 {
        return Some("Private network (192.168.0.0/16)");
    }

    // Benchmark: 198.18.0.0/15 (198.18.0.0 - 198.19.255.255)
    if octets[0] == 198 && (octets[1] == 18 || octets[1] == 19) {
        return Some("Benchmark network (198.18.0.0/15)");
    }

    // TEST-NET-2: 198.51.100.0/24
    if octets[0] == 198 && octets[1] == 51 && octets[2] == 100 {
        return Some("Test network (198.51.100.0/24)");
    }

    // TEST-NET-3: 203.0.113.0/24
    if octets[0] == 203 && octets[1] == 0 && octets[2] == 113 {
        return Some("Test network (203.0.113.0/24)");
    }

    // Multicast: 224.0.0.0/4 (224.0.0.0 - 239.255.255.255)
    if octets[0] >= 224 && octets[0] <= 239 {
        return Some("Multicast (224.0.0.0/4)");
    }

    // Reserved / Future use: 240.0.0.0/4
    if octets[0] >= 240 {
        return Some("Reserved / Broadcast (240.0.0.0/4)");
    }

    None
}

/// Checks if an IPv6 address falls into restricted/private/SSRF-prone CIDRs.
pub fn is_ipv6_blocked(ip: Ipv6Addr, allow_loopback: bool) -> Option<&'static str> {
    if ip.is_loopback() {
        if allow_loopback {
            return None;
        }
        return Some("Loopback (::1)");
    }

    if ip.is_unspecified() {
        return Some("Unspecified (::)");
    }

    // Check for IPv4-mapped IPv6 address (::ffff:x.x.x.x)
    if let Some(mapped_v4) = ip.to_ipv4_mapped() {
        return is_ipv4_blocked(mapped_v4, allow_loopback);
    }

    let segments = ip.segments();

    // Link-local unicast: fe80::/10
    if (segments[0] & 0xffc0) == 0xfe80 {
        return Some("Link-local unicast (fe80::/10)");
    }

    // Unique Local Addresses (ULA): fc00::/7 (fc00 - fdff)
    if (segments[0] & 0xfe00) == 0xfc00 {
        return Some("Unique Local Address (fc00::/7)");
    }

    // Multicast: ff00::/8
    if (segments[0] & 0xff00) == 0xff00 {
        return Some("Multicast (ff00::/8)");
    }

    // Documentation: 2001:db8::/32
    if segments[0] == 0x2001 && segments[1] == 0x0db8 {
        return Some("Documentation address (2001:db8::/32)");
    }

    None
}

pub fn is_ip_blocked(ip: IpAddr, allow_loopback: bool) -> Option<&'static str> {
    match ip {
        IpAddr::V4(v4) => is_ipv4_blocked(v4, allow_loopback),
        IpAddr::V6(v6) => is_ipv6_blocked(v6, allow_loopback),
    }
}

/// Parses and validates a navigation or redirect URL according to security policy.
pub fn validate_navigation_url(raw_url: &str, policy: &BrowserSecurityPolicy) -> BrowserResult<String> {
    let trimmed = raw_url.trim();
    if trimmed.is_empty() {
        return Err(BrowserError::InvalidUrl("Empty URL".to_string()));
    }

    if trimmed == "about:blank" {
        if policy.allow_internal_blank {
            return Ok("about:blank".to_string());
        } else {
            return Err(BrowserError::BlockedUrlScheme("about:blank not allowed".to_string()));
        }
    }

    // Check scheme
    let scheme_end = trimmed.find(':').ok_or_else(|| {
        BrowserError::InvalidUrl(format!("URL missing scheme separator: {}", trimmed))
    })?;

    let scheme = &trimmed[..scheme_end].to_ascii_lowercase();
    if scheme != "http" && scheme != "https" {
        return Err(BrowserError::BlockedUrlScheme(format!(
            "Forbidden scheme '{}' in URL: {}. Only http:// and https:// are permitted.",
            scheme, trimmed
        )));
    }

    // Check format
    if !trimmed[scheme_end..].starts_with("://") {
        return Err(BrowserError::InvalidUrl(format!(
            "Malformed URL scheme delimiter: {}",
            trimmed
        )));
    }

    let rest = &trimmed[scheme_end + 3..];
    // Extract authority (host[:port]) before any path / query / fragment
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..authority_end];

    if authority.is_empty() {
        return Err(BrowserError::InvalidUrl("Missing host in URL".to_string()));
    }

    // Strip credentials if present (user:pass@host)
    let host_port = if let Some(at_idx) = authority.rfind('@') {
        &authority[at_idx + 1..]
    } else {
        authority
    };

    // Extract host and port
    let (host, port) = if host_port.starts_with('[') {
        // IPv6 bracket notation [fe80::1]:8080
        if let Some(close_bracket) = host_port.find(']') {
            let h = &host_port[1..close_bracket];
            let p = if host_port[close_bracket..].starts_with("]:") {
                host_port[close_bracket + 2..].parse::<u16>().ok()
            } else {
                None
            };
            (h, p.unwrap_or(if scheme == "https" { 443 } else { 80 }))
        } else {
            return Err(BrowserError::InvalidUrl("Unmatched IPv6 bracket".to_string()));
        }
    } else if let Some(colon_idx) = host_port.find(':') {
        let h = &host_port[..colon_idx];
        let p = host_port[colon_idx + 1..]
            .parse::<u16>()
            .map_err(|_| BrowserError::InvalidUrl(format!("Invalid port in URL: {}", trimmed)))?;
        (h, p)
    } else {
        (host_port, if scheme == "https" { 443 } else { 80 })
    };

    let host_lower = host.to_ascii_lowercase();

    // Check known dangerous internal hostnames
    if host_lower == "localhost" || host_lower.ends_with(".localhost") {
        if !policy.allow_loopback_for_tests {
            return Err(BrowserError::SsrfBlocked(
                trimmed.to_string(),
                "Loopback address forbidden".to_string(),
            ));
        }
        return Ok(trimmed.to_string());
    }

    if host_lower == "metadata.google.internal"
        || host_lower.ends_with(".internal")
        || host_lower.ends_with(".local")
        || host_lower.ends_with(".corp")
    {
        return Err(BrowserError::SsrfBlocked(
            trimmed.to_string(),
            format!("Internal host domain forbidden: {}", host_lower),
        ));
    }

    // Check if host is direct IP address
    if let Ok(ip) = host.parse::<IpAddr>() {
        if let Some(reason) = is_ip_blocked(ip, policy.allow_loopback_for_tests) {
            return Err(BrowserError::SsrfBlocked(
                trimmed.to_string(),
                format!("IP {} blocked: {}", ip, reason),
            ));
        }
        return Ok(trimmed.to_string());
    }

    // Try resolving host to IP addresses if possible
    if let Ok(addrs) = (host, port).to_socket_addrs() {
        for socket_addr in addrs {
            let ip = socket_addr.ip();
            if let Some(reason) = is_ip_blocked(ip, policy.allow_loopback_for_tests) {
                return Err(BrowserError::SsrfBlocked(
                    trimmed.to_string(),
                    format!("Resolved IP {} blocked: {}", ip, reason),
                ));
            }
        }
    }

    Ok(trimmed.to_string())
}
