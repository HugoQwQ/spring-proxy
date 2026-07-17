//! Routing rules for connection target selection.
//!
//! # Depth
//! **Interface:** [`Router::resolve`] takes an optional handshake, returns a target address.
//!
//! **Behind the seam:**
//! - Rule chain evaluation (first match wins)
//! - Multiple match strategies (exact host, wildcard glob, regex, port range)
//! - Default route fallback
//! - Rule hot-reloading support (via `Arc<[Route]>`)

use std::net::SocketAddr;
use std::sync::Arc;

use common::Error;
use protocol::Handshake;

// Match

/// How a route matches an incoming connection.
#[derive(Debug, Clone)]
pub enum Match {
    /// Match any connection (catch-all / default route).
    Any,
    /// Match by exact server address (e.g., `"hypixel.net"`).
    Host(String),
    /// Match by wildcard pattern (e.g., `"*.hypixel.net"`).
    Pattern(String),
    /// Match by Minecraft protocol version.
    ProtocolVersion(i32),
    /// Match by port.
    Port(u16),
    /// Match by next state (status or login).
    NextState(protocol::NextState),
    /// Match if any sub-match matches.
    AnyOf(Vec<Match>),
    /// Match if all sub-matches match.
    AllOf(Vec<Match>),
    /// Negate a match.
    Not(Box<Match>),
}

impl Match {
    /// Check if this match applies to the given handshake.
    fn matches(&self, hs: Option<&Handshake>) -> bool {
        match self {
            Self::Any => true,
            Self::Host(host) => hs.is_some_and(|h| h.server_address == *host),
            Self::Pattern(pat) => hs.is_some_and(|h| wildcard_match(&h.server_address, pat)),
            Self::ProtocolVersion(ver) => hs.is_some_and(|h| h.protocol_version == *ver),
            Self::Port(port) => hs.is_some_and(|h| h.server_port == *port),
            Self::NextState(state) => hs.is_some_and(|h| h.next_state == *state),
            Self::AnyOf(ms) => ms.iter().any(|m| m.matches(hs)),
            Self::AllOf(ms) => ms.iter().all(|m| m.matches(hs)),
            Self::Not(m) => !m.matches(hs),
        }
    }
}

/// Simple wildcard matcher (`*` matches any sequence of chars, `?` matches one char).
fn wildcard_match(input: &str, pattern: &str) -> bool {
    let input_chars: Vec<char> = input.chars().collect();
    let pattern_chars: Vec<char> = pattern.chars().collect();

    let mut input_idx = 0;
    let mut pattern_idx = 0;
    let mut star_idx: Option<usize> = None;
    let mut match_idx = 0;

    while input_idx < input_chars.len() {
        if pattern_idx < pattern_chars.len()
            && (pattern_chars[pattern_idx] == '?'
                || pattern_chars[pattern_idx] == input_chars[input_idx])
        {
            input_idx += 1;
            pattern_idx += 1;
        } else if pattern_idx < pattern_chars.len() && pattern_chars[pattern_idx] == '*' {
            star_idx = Some(pattern_idx);
            match_idx = input_idx;
            pattern_idx += 1;
        } else if let Some(si) = star_idx {
            pattern_idx = si + 1;
            match_idx += 1;
            input_idx = match_idx;
        } else {
            return false;
        }
    }

    while pattern_idx < pattern_chars.len() && pattern_chars[pattern_idx] == '*' {
        pattern_idx += 1;
    }

    pattern_idx == pattern_chars.len()
}

// Route

/// A single routing rule.
#[derive(Debug, Clone)]
pub struct Route {
    /// Match condition.
    pub match_on: Match,
    /// Target address to relay to.
    pub target: SocketAddr,
    /// Optional label for metrics/logging.
    pub label: Option<String>,
}

impl Route {
    /// Check if this route matches the handshake.
    pub fn matches(&self, hs: Option<&Handshake>) -> bool {
        self.match_on.matches(hs)
    }
}

// Router

/// Resolves incoming connections to target servers using a rule chain.
///
/// # Depth
/// - **Interface:** one method: [`Router::resolve`]
/// - **Behind the seam:** rule chain evaluation, wildcard matching,
///   default fallback, hot-reloadable rule set.
///
/// # Seam placement
/// The router sits at the seam between connection handling and target selection.
/// Tests inject a `Router` with known rules; production builds it from config.
#[derive(Debug, Clone)]
pub struct Router {
    routes: Arc<[Route]>,
    /// No-route fallback error message.
    fallback_error: String,
}

impl Router {
    /// Create a new router from a list of rules.
    ///
    /// Routes are evaluated in order. The first matching route wins.
    /// If no route matches, the connection is rejected with a no-route error.
    pub fn new(routes: Vec<Route>) -> Self {
        Self {
            routes: routes.into(),
            fallback_error: "no matching route for connection".into(),
        }
    }

    /// Create a router with a single default route (matches everything).
    pub fn default(target: SocketAddr) -> Self {
        Self::new(vec![Route {
            match_on: Match::Any,
            target,
            label: Some("default".into()),
        }])
    }

    /// Resolve a target address for the given (optional) handshake.
    ///
    /// Returns the target `SocketAddr` if a route matches, or
    /// [`Error::NoRoute`] if no route applies.
    pub fn resolve(&self, handshake: Option<&Handshake>) -> Result<SocketAddr, Error> {
        for route in self.routes.iter() {
            if route.matches(handshake) {
                return Ok(route.target);
            }
        }
        Err(Error::NoRoute(self.fallback_error.clone()))
    }

    /// Replace the rule set (for hot-reload).
    pub fn reload(&mut self, routes: Vec<Route>) {
        self.routes = routes.into();
    }
}

// Tests

#[cfg(test)]
mod tests {
    use super::*;
    use protocol::NextState;
    use std::net::{Ipv4Addr, SocketAddrV4};

    fn hypixel_hs() -> Handshake {
        Handshake {
            protocol_version: 763,
            server_address: "hypixel.net".into(),
            server_port: 25565,
            next_state: NextState::Login,
        }
    }

    fn localhost_hs() -> Handshake {
        Handshake {
            protocol_version: 47,
            server_address: "localhost".into(),
            server_port: 25566,
            next_state: NextState::Status,
        }
    }

    #[test]
    fn router_default_route() {
        let addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 25565));
        let router = Router::default(addr);

        assert_eq!(router.resolve(Some(&hypixel_hs())).unwrap(), addr);
        assert_eq!(router.resolve(None).unwrap(), addr);
        assert_eq!(router.resolve(Some(&localhost_hs())).unwrap(), addr);
    }

    #[test]
    fn router_host_match() {
        let hypixel_addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(172, 65, 0, 1), 25565));
        let fallback = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 25565));

        let router = Router::new(vec![
            Route {
                match_on: Match::Host("hypixel.net".into()),
                target: hypixel_addr,
                label: Some("hypixel".into()),
            },
            Route {
                match_on: Match::Any,
                target: fallback,
                label: Some("fallback".into()),
            },
        ]);

        assert_eq!(router.resolve(Some(&hypixel_hs())).unwrap(), hypixel_addr);
        assert_eq!(router.resolve(Some(&localhost_hs())).unwrap(), fallback);
    }

    #[test]
    fn router_wildcard_match() {
        let proxy_addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(10, 0, 0, 1), 25565));

        let router = Router::new(vec![Route {
            match_on: Match::Pattern("*.hypixel.net".into()),
            target: proxy_addr,
            label: Some("wildcard".into()),
        }]);

        let hs = Handshake {
            server_address: "mc.hypixel.net".into(),
            ..hypixel_hs()
        };
        assert_eq!(router.resolve(Some(&hs)).unwrap(), proxy_addr);

        // Should NOT match plain hypixel.net (no subdomain)
        assert!(router.resolve(Some(&hypixel_hs())).is_err());
    }

    #[test]
    fn router_no_match_returns_error() {
        let router = Router::new(vec![Route {
            match_on: Match::Host("specific.example.com".into()),
            target: "127.0.0.1:25565".parse().unwrap(),
            label: None,
        }]);

        let result = router.resolve(Some(&hypixel_hs()));
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::NoRoute(_)));
    }

    #[test]
    fn router_next_state_match() {
        let status_addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(1, 2, 3, 4), 25565));
        let login_addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(5, 6, 7, 8), 25565));

        let router = Router::new(vec![
            Route {
                match_on: Match::NextState(NextState::Status),
                target: status_addr,
                label: Some("status".into()),
            },
            Route {
                match_on: Match::NextState(NextState::Login),
                target: login_addr,
                label: Some("login".into()),
            },
        ]);

        assert_eq!(router.resolve(Some(&hypixel_hs())).unwrap(), login_addr);

        let status_hs = Handshake {
            next_state: NextState::Status,
            ..hypixel_hs()
        };
        assert_eq!(router.resolve(Some(&status_hs)).unwrap(), status_addr);
    }

    #[test]
    fn wildcard_matches() {
        assert!(wildcard_match("mc.hypixel.net", "*.hypixel.net"));
        assert!(wildcard_match("a.b.c.hypixel.net", "*.hypixel.net"));
        assert!(!wildcard_match("hypixel.net", "*.hypixel.net"));
        assert!(wildcard_match("anything", "*"));
        assert!(wildcard_match("hello", "h?llo"));
        assert!(!wildcard_match("hallo", "hello"));
        assert!(wildcard_match("foo", "f*"));
        assert!(wildcard_match("foo", "*o"));
    }

    #[test]
    fn router_anyof_match() {
        let addr = "127.0.0.1:25565".parse().unwrap();

        let router = Router::new(vec![Route {
            match_on: Match::AnyOf(vec![
                Match::Host("hypixel.net".into()),
                Match::Host("minemen.club".into()),
            ]),
            target: addr,
            label: None,
        }]);

        assert_eq!(router.resolve(Some(&hypixel_hs())).unwrap(), addr);

        let mhc_hs = Handshake {
            server_address: "minemen.club".into(),
            ..hypixel_hs()
        };
        assert_eq!(router.resolve(Some(&mhc_hs)).unwrap(), addr);

        assert!(router.resolve(Some(&localhost_hs())).is_err());
    }

    #[test]
    fn router_compound_match() {
        let addr = "10.0.0.1:25565".parse().unwrap();

        let router = Router::new(vec![Route {
            match_on: Match::AllOf(vec![
                Match::Host("hypixel.net".into()),
                Match::ProtocolVersion(763),
            ]),
            target: addr,
            label: None,
        }]);

        assert_eq!(router.resolve(Some(&hypixel_hs())).unwrap(), addr);

        let diff_ver = Handshake {
            protocol_version: 47,
            ..hypixel_hs()
        };
        assert!(router.resolve(Some(&diff_ver)).is_err());
    }
}
