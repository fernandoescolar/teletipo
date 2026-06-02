/// Intern table for OSC 8 hyperlink URIs.
///
/// Each unique URI is stored once and identified by a compact `u16` index so
/// that [`crate::cell::Cell`] can hold it without becoming heap-allocated.
/// Index `0` is reserved as "no link"; valid IDs start at 1.
#[derive(Debug, Clone, Default)]
pub struct HyperlinkInterner {
    uris: Vec<String>,
}

impl HyperlinkInterner {
    /// Intern `uri` and return its stable ID (1-based, never 0).
    /// Returns the same ID for duplicate URIs.
    pub fn intern(&mut self, uri: &str) -> u16 {
        if let Some(pos) = self.uris.iter().position(|u| u == uri) {
            // Offset by 1 because ID 0 means "no link".
            (pos + 1) as u16
        } else {
            self.uris.push(uri.to_owned());
            self.uris.len() as u16 // already 1-based after the push
        }
    }

    /// Resolve a link ID back to its URI string. Returns `None` for ID 0 or
    /// out-of-range IDs.
    pub fn resolve(&self, id: u16) -> Option<&str> {
        if id == 0 {
            return None;
        }
        self.uris.get((id as usize) - 1).map(|s| s.as_str())
    }

    /// Remove all interned URIs.  Called when the alternate screen is cleared
    /// so stale IDs do not reference the wrong URI after switching back.
    pub fn clear(&mut self) {
        self.uris.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intern_returns_one_based_ids() {
        let mut h = HyperlinkInterner::default();
        let id1 = h.intern("https://example.com");
        let id2 = h.intern("https://rust-lang.org");
        assert_eq!(id1, 1);
        assert_eq!(id2, 2);
    }

    #[test]
    fn duplicate_uri_returns_same_id() {
        let mut h = HyperlinkInterner::default();
        let id_a = h.intern("https://example.com");
        let id_b = h.intern("https://example.com");
        assert_eq!(id_a, id_b);
    }

    #[test]
    fn resolve_works_for_valid_ids() {
        let mut h = HyperlinkInterner::default();
        let id = h.intern("https://example.com");
        assert_eq!(h.resolve(id), Some("https://example.com"));
    }

    #[test]
    fn resolve_zero_returns_none() {
        let h = HyperlinkInterner::default();
        assert_eq!(h.resolve(0), None);
    }

    #[test]
    fn clear_makes_ids_unresolvable() {
        let mut h = HyperlinkInterner::default();
        let id = h.intern("https://example.com");
        h.clear();
        assert_eq!(h.resolve(id), None);
    }
}
