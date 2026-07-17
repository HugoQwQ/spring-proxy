//! String set with JSON serialization support.
//!
//! A thin wrapper around `std::collections::HashSet<String>` that supports
//! JSON array serialization and deserialization.

use std::collections::HashSet;

/// A set of strings, serialized as a JSON array.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StringSet {
    inner: HashSet<String>,
}

impl StringSet {
    /// Create an empty set.
    pub fn new() -> Self {
        Self {
            inner: HashSet::new(),
        }
    }

    /// Create a set from a slice of strings.
    pub fn from_slice(slice: &[String]) -> Self {
        let mut s = Self::new();
        for item in slice {
            s.add(item.clone());
        }
        s
    }

    /// Check if the set contains `item`.
    pub fn has(&self, item: &str) -> bool {
        self.inner.contains(item)
    }

    /// Add an item to the set.
    pub fn add(&mut self, item: impl Into<String>) {
        self.inner.insert(item.into());
    }

    /// Remove an item from the set.
    pub fn delete(&mut self, item: &str) {
        self.inner.remove(item);
    }

    /// Returns the number of items in the set.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Returns `true` if the set is empty.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Iterate over the set.
    pub fn iter(&self) -> impl Iterator<Item = &String> {
        self.inner.iter()
    }
}

// serde

impl serde::Serialize for StringSet {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let vec: Vec<&String> = self.inner.iter().collect();
        vec.serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for StringSet {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let vec = Vec::<String>::deserialize(deserializer)?;
        Ok(Self::from_slice(&vec))
    }
}

// Tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_basic() {
        let mut s = StringSet::new();
        assert!(!s.has("hello"));
        s.add("hello");
        assert!(s.has("hello"));
        s.delete("hello");
        assert!(!s.has("hello"));
    }

    #[test]
    fn set_from_slice() {
        let s = StringSet::from_slice(&["a".into(), "b".into(), "a".into()]);
        assert_eq!(s.len(), 2);
        assert!(s.has("a"));
        assert!(s.has("b"));
    }

    #[test]
    fn set_json_roundtrip() {
        let mut s = StringSet::new();
        s.add("foo");
        s.add("bar");
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"foo\""));
        assert!(json.contains("\"bar\""));

        let decoded: StringSet = serde_json::from_str(&json).unwrap();
        assert!(decoded.has("foo"));
        assert!(decoded.has("bar"));
    }
}
