//! Fast domain matcher supporting exact and suffix matching.
//!
//! Uses a straightforward but correct approach:
//! * Exact domains are stored in a hash set.
//! * Domain suffixes are checked with proper boundary validation
//!   (e.g. `.example.com` matches `www.example.com` but not `xexample.com`).

use std::collections::HashSet;

/// Matches domains against a set of exact domains and domain suffixes.
#[derive(Debug, Clone, Default)]
pub struct Matcher {
    exact: HashSet<String>,
    suffixes: Vec<String>,
    root_suffixes: Vec<String>,
}

impl Matcher {
    /// Create a new matcher from lists of exact domains and suffixes.
    ///
    /// A suffix starting with `.` (e.g. `.example.com`) only matches
    /// subdomains, not the root domain itself.
    /// A suffix without `.` (e.g. `example.com`) matches both the root
    /// domain and its subdomains.
    pub fn new(domains: &[String], suffixes: &[String]) -> Self {
        let mut exact = HashSet::new();
        let mut sfx = Vec::new();
        let mut root_sfx = Vec::new();

        for domain in domains {
            exact.insert(domain.to_lowercase());
        }

        for suffix in suffixes {
            let s = suffix.to_lowercase();
            if let Some(stripped) = s.strip_prefix('.') {
                sfx.push(stripped.to_string());
            } else {
                root_sfx.push(s);
            }
        }

        Self {
            exact,
            suffixes: sfx,
            root_suffixes: root_sfx,
        }
    }

    /// Check if `domain` matches any of the configured domains or suffixes.
    pub fn matches(&self, domain: &str) -> bool {
        let domain = domain.to_lowercase();

        // Exact match
        if self.exact.contains(&domain) {
            return true;
        }

        // Root suffix match: `example.com` matches both `example.com` and `www.example.com`
        for root in &self.root_suffixes {
            if domain == *root {
                return true;
            }
            if domain.ends_with(root) {
                // Must be a proper subdomain boundary
                let idx = domain.len() - root.len();
                if idx > 0 && domain.as_bytes()[idx - 1] == b'.' {
                    return true;
                }
            }
        }

        // Suffix match: `.example.com` matches `www.example.com` but NOT `example.com`
        for sfx in &self.suffixes {
            if domain.ends_with(sfx) {
                let idx = domain.len() - sfx.len();
                if idx > 0 && domain.as_bytes()[idx - 1] == b'.' {
                    return true;
                }
            }
        }

        false
    }
}

// Tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matcher_exact_domain() {
        let m = Matcher::new(&["example.com".into()], &[]);
        assert!(m.matches("example.com"));
        assert!(!m.matches("www.example.com"));
        assert!(!m.matches("other.com"));
    }

    #[test]
    fn matcher_suffix() {
        let m = Matcher::new(&[], &[".example.com".into()]);
        assert!(m.matches("www.example.com"));
        assert!(m.matches("sub.www.example.com"));
        assert!(!m.matches("example.com"));
        assert!(!m.matches("other.com"));
        assert!(!m.matches("xexample.com"));
    }

    #[test]
    fn matcher_root_suffix() {
        let m = Matcher::new(&[], &["example.com".into()]);
        assert!(m.matches("www.example.com"));
        assert!(m.matches("example.com"));
        assert!(!m.matches("other.com"));
        assert!(!m.matches("xexample.com"));
    }

    #[test]
    fn matcher_combined() {
        let m = Matcher::new(&["exact.org".into()], &[".example.com".into()]);
        assert!(m.matches("exact.org"));
        assert!(m.matches("www.example.com"));
        assert!(!m.matches("www.exact.org"));
    }

    #[test]
    fn matcher_case_insensitive() {
        let m = Matcher::new(&["EXAMPLE.COM".into()], &[]);
        assert!(m.matches("example.com"));
        assert!(m.matches("EXAMPLE.COM"));
    }
}
