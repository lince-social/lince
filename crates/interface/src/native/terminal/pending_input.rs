// non-lean-yet
// Scope: const LIMIT, struct Chunk, struct Input.
// BeautifulRust (baseline lines):
// - [review: §5 Panics] Prove runtime index bounds or use checked access on fallible input. Sites
//   12, 68, 74.
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (90).

use std::{borrow::Cow, collections::VecDeque};

const LIMIT: usize = 1024 * 1024;

struct Chunk {
    bytes: Vec<u8>,
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

impl Input {
    pub(super) fn len_bytes(&self) -> usize {
        self.bytes
    }

    pub(super) fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }

    pub(super) fn fits(&self, additional: usize) -> bool {
        self.bytes
            .checked_add(additional)
            .is_some_and(|bytes| bytes <= LIMIT)
    }

    pub(super) fn push_back(&mut self, bytes: Vec<u8>) {
        self.bytes += bytes.len();
        self.chunks.push_back(Chunk { bytes, consumed: 0 });
    }

    pub(super) fn append(&mut self, mut input: Self) {
        self.bytes += input.bytes;
        self.chunks.append(&mut input.chunks);
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

    pub(super) fn consume(&mut self, count: usize) {
        assert!(
            count <= self.bytes,
            "terminal input prefix exceeds its queue"
        );
        let mut remaining = count;
        while remaining > 0 {
            let front = self.chunks.front_mut().unwrap();
            let taken = remaining.min(front.remaining().len());
            front.consumed += taken;
            remaining -= taken;
            if front.remaining().is_empty() {
                self.chunks.pop_front();
            }
        }
        self.bytes -= count;
    }
}
