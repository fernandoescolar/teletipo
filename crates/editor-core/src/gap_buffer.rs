/// A gap buffer for efficient insertions and deletions near the cursor.
///
/// Internally the buffer looks like:
/// ```text
/// [ text_left ][ gap_bytes ][ text_right ]
///              ^            ^
///           gap_start     gap_end
/// ```
///
/// After every mutation the content is the concatenation of
/// `buf[..gap_start]` and `buf[gap_end..]`.
#[derive(Debug)]
pub struct GapBuffer {
    buf: Vec<u8>,
    /// First byte of the gap (equals the byte length of the left content).
    gap_start: usize,
    /// First byte of the right content (equals `buf.len()` when there is no
    /// right content, or the gap reaches the physical end of the buffer).
    gap_end: usize,
}

impl GapBuffer {
    /// Minimum gap size allocated after a grow.
    const MIN_GAP: usize = 64;

    pub fn new() -> Self {
        let gap = Self::MIN_GAP;
        let buf = vec![0; gap];
        GapBuffer {
            buf,
            gap_start: 0,
            gap_end: gap,
        }
    }

    /// Byte length of the logical text (gap bytes excluded).
    pub fn len(&self) -> usize {
        self.buf.len() - (self.gap_end - self.gap_start)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Translate a logical byte offset into a physical buffer index.
    #[inline]
    fn physical(&self, logical: usize) -> usize {
        if logical < self.gap_start {
            logical
        } else {
            logical + (self.gap_end - self.gap_start)
        }
    }

    /// Returns the byte at logical position `pos`.
    fn byte_at(&self, pos: usize) -> u8 {
        self.buf[self.physical(pos)]
    }

    /// Shift the gap to `new_pos` (a logical byte offset).
    fn move_gap_to(&mut self, new_pos: usize) {
        let new_pos = new_pos.min(self.len());
        if new_pos == self.gap_start {
            return; // already there
        }
        let gap_size = self.gap_end - self.gap_start;
        if new_pos < self.gap_start {
            // Move gap left: copy bytes from left of gap to after the gap.
            let n = self.gap_start - new_pos;
            self.buf
                .copy_within(new_pos..self.gap_start, self.gap_end - n);
            self.gap_start = new_pos;
            self.gap_end = new_pos + gap_size;
        } else {
            // Move gap right: copy bytes from right of gap to before it.
            let n = new_pos - self.gap_start;
            let src_start = self.gap_end;
            self.buf
                .copy_within(src_start..src_start + n, self.gap_start);
            self.gap_start = new_pos;
            self.gap_end = new_pos + gap_size;
        }
    }

    /// Ensure the gap is at least `needed` bytes wide.
    fn ensure_gap(&mut self, needed: usize) {
        let current = self.gap_end - self.gap_start;
        if current >= needed {
            return;
        }
        let extra = (needed - current).max(Self::MIN_GAP);
        // Insert `extra` zeroed bytes at the physical gap position.
        let insert_at = self.gap_start;
        let new_len = self.buf.len() + extra;
        self.buf.resize(new_len, 0u8);
        // Shift right side of the buffer rightward by `extra` bytes.
        self.buf
            .copy_within(insert_at..new_len - extra, insert_at + extra);
        self.gap_end += extra;
    }

    /// Insert the UTF-8 bytes of `s` at logical byte offset `at`.
    pub fn insert_str(&mut self, at: usize, s: &str) {
        let bytes = s.as_bytes();
        self.move_gap_to(at);
        self.ensure_gap(bytes.len());
        let end = self.gap_start + bytes.len();
        self.buf[self.gap_start..end].copy_from_slice(bytes);
        self.gap_start = end;
    }

    /// Remove the `char` whose first byte is at logical offset `at`.
    /// Returns the removed character.
    ///
    /// # Panics
    /// Panics if `at` is not on a char boundary.
    pub fn remove_char(&mut self, at: usize) -> char {
        // Determine char length by peeking at the byte.
        let first = self.byte_at(at);
        let char_len = char_len_from_first_byte(first);
        let mut bytes = [0u8; 4];
        for (i, slot) in bytes.iter_mut().take(char_len).enumerate() {
            *slot = self.byte_at(at + i);
        }
        let ch = std::str::from_utf8(&bytes[..char_len])
            .ok()
            .and_then(|s| s.chars().next())
            .expect("valid UTF-8 char in gap buffer");
        self.delete_range(at, at + char_len);
        ch
    }

    /// Delete the logical byte range `[start, end)`.  Returns the removed text.
    pub fn delete_range(&mut self, start: usize, end: usize) -> String {
        if start == end {
            return String::new();
        }
        // Collect the removed bytes.
        let mut removed_bytes: Vec<u8> = Vec::with_capacity(end - start);
        for i in start..end {
            removed_bytes.push(self.byte_at(i));
        }
        // Move gap to `start`, then widen the gap to consume [start, end).
        self.move_gap_to(start);
        // Right-expanding the gap: simply advance gap_end by (end - start).
        // Since the gap is now at `start`, the right content starts at
        // gap_end. We need to consume `end - start` bytes from the right side.
        let consume = end - start;
        self.gap_end += consume;
        String::from_utf8(removed_bytes).expect("UTF-8 content")
    }

    /// Reset the buffer to an empty state and return the previous content.
    pub fn clear(&mut self) -> String {
        let s = self.to_owned_string();
        self.gap_start = 0;
        self.gap_end = self.buf.len();
        s
    }

    /// Returns `true` if `idx` falls on a UTF-8 character boundary.
    pub fn is_char_boundary(&self, idx: usize) -> bool {
        if idx == 0 || idx == self.len() {
            return true;
        }
        if idx > self.len() {
            return false;
        }
        let b = self.byte_at(idx);
        // A byte is the start of a char if it is not a UTF-8 continuation byte.
        b & 0xC0 != 0x80
    }

    /// Materialise the logical content as an owned `String`.
    pub fn to_owned_string(&self) -> String {
        let mut s = Vec::with_capacity(self.len());
        s.extend_from_slice(&self.buf[..self.gap_start]);
        s.extend_from_slice(&self.buf[self.gap_end..]);
        String::from_utf8(s).expect("UTF-8 content in gap buffer")
    }
}

impl Default for GapBuffer {
    fn default() -> Self {
        Self::new()
    }
}

/// Determine the byte length of a UTF-8 character from its first byte.
fn char_len_from_first_byte(b: u8) -> usize {
    if b & 0x80 == 0 {
        1
    } else if b & 0xE0 == 0xC0 {
        2
    } else if b & 0xF0 == 0xE0 {
        3
    } else {
        4
    }
}

#[cfg(test)]
mod tests {
    use super::GapBuffer;

    #[test]
    fn insert_and_materialize() {
        let mut g = GapBuffer::new();
        g.insert_str(0, "hello");
        g.insert_str(5, " world");
        assert_eq!(g.to_owned_string(), "hello world");
    }

    #[test]
    fn insert_in_middle() {
        let mut g = GapBuffer::new();
        g.insert_str(0, "helo");
        g.insert_str(3, "l");
        assert_eq!(g.to_owned_string(), "hello");
    }

    #[test]
    fn delete_range() {
        let mut g = GapBuffer::new();
        g.insert_str(0, "hello world");
        let removed = g.delete_range(5, 11);
        assert_eq!(removed, " world");
        assert_eq!(g.to_owned_string(), "hello");
    }

    #[test]
    fn remove_char() {
        let mut g = GapBuffer::new();
        g.insert_str(0, "héllo");
        // 'é' is 2 bytes at offset 1
        let ch = g.remove_char(1);
        assert_eq!(ch, 'é');
        assert_eq!(g.to_owned_string(), "hllo");
    }

    #[test]
    fn clear_returns_content() {
        let mut g = GapBuffer::new();
        g.insert_str(0, "abc");
        let s = g.clear();
        assert_eq!(s, "abc");
        assert!(g.is_empty());
    }

    #[test]
    fn is_char_boundary() {
        let mut g = GapBuffer::new();
        g.insert_str(0, "héllo");
        // 'h' at 0 (boundary), 'é' starts at 1 (boundary), continuation at 2 (NOT boundary)
        assert!(g.is_char_boundary(0));
        assert!(g.is_char_boundary(1));
        assert!(!g.is_char_boundary(2));
        assert!(g.is_char_boundary(3)); // 'l' starts here
    }

    #[test]
    fn large_insert_grows_gap() {
        let mut g = GapBuffer::new();
        let big = "x".repeat(512);
        g.insert_str(0, &big);
        assert_eq!(g.len(), 512);
        assert_eq!(g.to_owned_string(), big);
    }
}
