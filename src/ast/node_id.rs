use std::sync::atomic::{AtomicU32, Ordering};

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct NodeId(u32);

static NEXT_NODE_ID: AtomicU32 = AtomicU32::new(0);

impl NodeId {
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
