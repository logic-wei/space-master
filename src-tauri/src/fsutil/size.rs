//! On-disk size accounting.
//!
//! Sizes come from `st_blocks`, not `st_size`. On APFS the two disagree in both
//! directions: sparse files report a large logical size backed by few blocks,
//! and compressed files report the opposite. Only the block count corresponds to
//! space that deleting the file gives back, and it is also what `du` reports,
//! which is how we verify these numbers.
//!
//! Even so, `st_blocks` is an *upper bound* on reclaimable space, because APFS
//! clones let several files share the same blocks while each reports them as its
//! own. That gap is why the UI distinguishes an estimate from the measured
//! `statfs` delta rather than presenting one number as the truth.

use std::collections::HashSet;
use std::fs::Metadata;
use std::os::unix::fs::MetadataExt;

/// `st_blocks` counts 512-byte units regardless of the filesystem block size.
const BLOCK: u64 = 512;

pub fn on_disk(md: &Metadata) -> u64 {
    md.blocks().saturating_mul(BLOCK)
}

/// Identity of a file for hard-link accounting. Inode numbers are only unique
/// within a device, so the device is part of the key.
pub type Ident = (u64, u64);

pub fn ident(md: &Metadata) -> Ident {
    (md.dev(), md.ino())
}

/// Tracks inodes that have already been counted, so a file reachable by several
/// hard links contributes its blocks once.
///
/// Only links with `nlink > 1` are recorded. A typical cache directory is almost
/// entirely `nlink == 1`, and remembering every one of those would grow the set
/// to millions of entries to prevent collisions that cannot happen.
#[derive(Debug, Default)]
pub struct LinkLedger {
    seen: HashSet<Ident>,
}

impl LinkLedger {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the bytes this entry contributes: its full size the first time an
    /// inode is seen, zero afterwards.
    pub fn account(&mut self, md: &Metadata) -> u64 {
        let bytes = on_disk(md);
        if md.nlink() <= 1 {
            return bytes;
        }
        if self.seen.insert(ident(md)) {
            bytes
        } else {
            0
        }
    }

    pub fn tracked(&self) -> usize {
        self.seen.len()
    }
}
