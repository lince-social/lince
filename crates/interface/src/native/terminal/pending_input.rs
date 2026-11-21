//! Byte-bounded input awaiting delivery to a Terminal session.

use std::{borrow::Cow, collections::VecDeque};

const LIMIT: usize = 1024 * 1024;

struct Chunk {
    bytes: Vec<u8>,
    // Consumption advances by at most the remaining length.
    consumed: usize,
}

impl Chunk {
    fn remaining(&self) -> &[u8] {
        &self.bytes[self.consumed..]
    }
}

#[derive(Default)]
pub(super) struct Input {
    chunks: VecDeque<Chunk>,
    bytes: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct QueueFull;

impl Input {
    pub(super) fn len_bytes(&self) -> usize {
        self.bytes
    }

    pub(super) fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }

    pub(super) fn fits(&self, additional: usize) -> bool {
        self.length_with(additional).is_some()
    }

    fn length_with(&self, additional: usize) -> Option<usize> {
        self.bytes
            .checked_add(additional)
            .filter(|&bytes| bytes <= LIMIT)
    }

    pub(super) fn push_back(&mut self, bytes: Vec<u8>) -> Result<(), QueueFull> {
        let length = self.length_with(bytes.len()).ok_or(QueueFull)?;
        self.chunks.push_back(Chunk { bytes, consumed: 0 });
        self.bytes = length;
        Ok(())
    }

    pub(super) fn append(&mut self, mut input: Self) -> Result<(), QueueFull> {
        let length = self.length_with(input.bytes).ok_or(QueueFull)?;
        self.chunks.append(&mut input.chunks);
        self.bytes = length;
        Ok(())
    }

    pub(super) fn front(&self) -> Option<&[u8]> {
        self.chunks.front().map(Chunk::remaining)
    }

    pub(super) fn pop_front(&mut self) {
        if let Some(chunk) = self.chunks.pop_front() {
            self.bytes -= chunk.remaining().len();
        }
    }

    pub(super) fn clear(&mut self) {
        self.chunks.clear();
        self.bytes = 0;
    }

    pub(super) fn prefix(&self, limit: usize) -> Cow<'_, [u8]> {
        let length = limit.min(self.bytes);
        let Some(first) = self.front() else {
            return Cow::Borrowed(&[]);
        };
        if first.len() >= length {
            return Cow::Borrowed(&first[..length]);
        }
        let mut bytes = Vec::with_capacity(length);
        for chunk in &self.chunks {
            let remaining = length - bytes.len();
            let data = chunk.remaining();
            bytes.extend_from_slice(&data[..remaining.min(data.len())]);
            if bytes.len() == length {
                break;
            }
        }
        nucleus::performance::add("terminal-input-prefix-copied-bytes", bytes.len() as u64);
        Cow::Owned(bytes)
    }

    /// Advances over a prefix already accepted by the input destination.
    pub(super) fn consume(&mut self, count: usize) {
        assert!(
            count <= self.bytes,
            "terminal input prefix exceeds its queue"
        );
        let mut remaining = count;
        while remaining != 0
            && let Some(front) = self.chunks.front_mut()
        {
            let taken = remaining.min(front.remaining().len());
            front.consumed += taken;
            remaining -= taken;
            if front.remaining().is_empty() {
                self.chunks.pop_front();
            }
        }
        debug_assert_eq!(remaining, 0, "queued bytes belong to a chunk");
        self.bytes -= count;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_byte_order_across_partial_reads_and_merges() {
        let mut input = Input::default();
        input.push_back(b"ab".to_vec()).expect("fits");
        input.push_back(b"cdef".to_vec()).expect("fits");
        assert!(matches!(input.prefix(2), Cow::Borrowed(b"ab")));
        assert_eq!(input.prefix(4), b"abcd".as_slice());
        input.consume(3);
        assert_eq!(input.front(), Some(b"def".as_slice()));

        let mut next = Input::default();
        next.push_back(b"fgh".to_vec()).expect("fits");
        next.consume(1);
        input.append(next).expect("fits after the partial read");
        assert_eq!(input.prefix(usize::MAX), b"defgh".as_slice());
        input.pop_front();
        assert_eq!(input.len_bytes(), 2);
        assert_eq!(input.front(), Some(b"gh".as_slice()));
        input.clear();
        assert!(input.is_empty());
    }

    #[test]
    fn rejects_overflow_without_mutating_the_queue() {
        let mut input = Input::default();
        input.push_back(vec![0; LIMIT]).expect("exact limit fits");
        assert!(!input.fits(usize::MAX));
        assert_eq!(input.push_back(vec![1]), Err(QueueFull));
        let mut incoming = Input::default();
        incoming.push_back(vec![2]).expect("one byte fits");
        assert_eq!(input.append(incoming), Err(QueueFull));
        assert_eq!(input.len_bytes(), LIMIT);
        assert_eq!(input.prefix(1), [0].as_slice());
        input.consume(1);
        input.push_back(vec![3]).expect("consumed bytes free space");
        assert_eq!(input.len_bytes(), LIMIT);
    }
}
