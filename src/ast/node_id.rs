//! [`NodeId`], a globally unique identity for AST nodes.
//!
//! Unlike [`crate::ast::interner::Symbol`], which is thread-local, `NodeId` must be unique
//! across the whole process: the parser may eventually parse files in parallel, one thread per
//! file, and later passes need to address any node in the program by a single id regardless of
//! which thread's parse produced it. A global atomic counter gives that for free, since
//! allocation order across threads is unspecified and unimportant -- only uniqueness matters.

use std::sync::atomic::{AtomicU32, Ordering};

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct NodeId(u32);

static NEXT_NODE_ID: AtomicU32 = AtomicU32::new(0);

impl NodeId {
    /// Allocates the next `NodeId`. Safe to call from any thread: ids are unique across the
    /// whole process, not just within one thread's allocations.
    pub fn next() -> NodeId {
        NodeId(NEXT_NODE_ID.fetch_add(1, Ordering::Relaxed))
    }

    pub fn index(self) -> usize {
        self.0 as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::sync::Mutex;
    use std::thread;

    /// Allocating `NodeId`s from several threads at once should never hand out a duplicate.
    #[test]
    fn concurrent_allocation_never_duplicates_an_id() {
        let seen: Mutex<Vec<NodeId>> = Mutex::new(Vec::new());
        thread::scope(|scope| {
            for _ in 0..8 {
                scope.spawn(|| {
                    let ids: Vec<NodeId> = (0..200).map(|_| NodeId::next()).collect();
                    seen.lock().unwrap().extend(ids);
                });
            }
        });

        let ids = seen.into_inner().unwrap();
        let unique: HashSet<NodeId> = ids.iter().copied().collect();
        assert_eq!(ids.len(), unique.len());
    }
}
