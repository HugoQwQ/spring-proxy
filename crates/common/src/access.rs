//! Access control for connections.
//!
//! Provides [`AccessMode`] and the [`check`] function for evaluating
//! whether an item (IP, hostname, player name, etc.) passes access control.

/// Access control mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AccessMode {
    /// No access control (default / passthrough).
    #[default]
    Default,
    /// Allow only items present in the lists.
    Allow,
    /// Block items present in the lists.
    Block,
}

impl AccessMode {
    /// Parse an access mode from a string.
    pub fn from_str(s: &str) -> Self {
        match s {
            "allow" => Self::Allow,
            "block" => Self::Block,
            _ => Self::Default,
        }
    }
}

/// Check whether `item` passes access control.
///
/// * `lists` — The access control lists to check against.
/// * `mode` — The access control mode.
/// * `item` — The item to check (IP string, hostname, player name, etc.).
///
/// Returns `true` if the item is allowed, `false` if rejected.
///
/// # Panics
/// Panics if `mode` is not one of the valid variants.
pub fn check(lists: &[crate::set::StringSet], mode: AccessMode, item: &str) -> bool {
    let hit = lists.iter().any(|list| list.has(item));
    match mode {
        AccessMode::Default => true,
        AccessMode::Allow => hit,
        AccessMode::Block => !hit,
    }
}

// Tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::set::StringSet;

    #[test]
    fn check_allow_mode() {
        let mut set = StringSet::new();
        set.add("allowed-item");
        let lists = vec![set];

        assert!(check(&lists, AccessMode::Allow, "allowed-item"));
        assert!(!check(&lists, AccessMode::Allow, "other-item"));
    }

    #[test]
    fn check_block_mode() {
        let mut set = StringSet::new();
        set.add("blocked-item");
        let lists = vec![set];

        assert!(!check(&lists, AccessMode::Block, "blocked-item"));
        assert!(check(&lists, AccessMode::Block, "other-item"));
    }

    #[test]
    fn check_default_mode() {
        let lists: Vec<StringSet> = vec![];
        assert!(check(&lists, AccessMode::Default, "anything"));
    }

    #[test]
    fn check_multiple_lists() {
        let mut s1 = StringSet::new();
        s1.add("in-list-1");
        let mut s2 = StringSet::new();
        s2.add("in-list-2");
        let lists = vec![s1, s2];

        assert!(check(&lists, AccessMode::Allow, "in-list-1"));
        assert!(check(&lists, AccessMode::Allow, "in-list-2"));
        assert!(!check(&lists, AccessMode::Allow, "other"));
    }
}
