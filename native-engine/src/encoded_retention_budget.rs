//! Shared owner-entry quota for already-allocated encoded buffers, not decode.
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, Weak};

const MAX_BYTES: usize = 128 * 1024 * 1024;
static SHARED: OnceLock<EncodedRetentionBudget> = OnceLock::new();
pub(super) fn shared() -> &'static EncodedRetentionBudget {
    SHARED.get_or_init(|| EncodedRetentionBudget::new(MAX_BYTES).expect("valid fixed quota"))
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RetentionError {
    Limit,
    Capacity,
    Allocation,
    Unavailable,
}
struct Entry {
    allocation: Weak<Vec<u8>>,
    cost: usize,
    pending: usize,
    committed: bool,
}
#[derive(Default)]
struct State {
    entries: HashMap<usize, Entry>,
    used: usize,
}
impl State {
    fn reap(&mut self) {
        let mut freed = 0;
        self.entries.retain(|_, entry| {
            let keep =
                entry.pending > 0 || (entry.committed && entry.allocation.upgrade().is_some());
            if !keep {
                freed += entry.cost;
            }
            keep
        });
        self.used -= freed;
    }
}
#[derive(Clone)]
pub(super) struct EncodedRetentionBudget {
    limit: usize,
    state: Arc<Mutex<State>>,
}
impl EncodedRetentionBudget {
    pub(super) fn new(limit: usize) -> Result<Self, RetentionError> {
        if limit == 0 || limit > MAX_BYTES {
            return Err(RetentionError::Limit);
        }
        Ok(Self {
            limit,
            state: Arc::new(Mutex::new(State::default())),
        })
    }
    /// No allocation/copy of binary data. Ticket pins input through identity.
    pub(super) fn admit(&self, allocation: &Arc<Vec<u8>>) -> Result<Admission, RetentionError> {
        let cost = allocation.capacity();
        if cost == 0 || cost > self.limit {
            return Err(RetentionError::Capacity);
        }
        let key = Arc::as_ptr(allocation) as usize;
        let weak = Arc::downgrade(allocation);
        let mut state = self.state.lock().map_err(|_| RetentionError::Unavailable)?;
        state.reap();
        if let Some(entry) = state.entries.get_mut(&key) {
            if !entry.allocation.ptr_eq(&weak) || entry.cost != cost {
                return Err(RetentionError::Unavailable);
            }
            entry.pending = entry
                .pending
                .checked_add(1)
                .ok_or(RetentionError::Capacity)?;
        } else {
            let total = state
                .used
                .checked_add(cost)
                .filter(|n| *n <= self.limit)
                .ok_or(RetentionError::Capacity)?;
            state
                .entries
                .try_reserve(1)
                .map_err(|_| RetentionError::Allocation)?;
            state.entries.insert(
                key,
                Entry {
                    allocation: weak,
                    cost,
                    pending: 1,
                    committed: false,
                },
            );
            state.used = total;
        }
        Ok(Admission {
            budget: self.clone(),
            key,
            _pin: Arc::clone(allocation),
            done: false,
        })
    }
    pub(super) fn used(&self) -> Result<usize, RetentionError> {
        let mut state = self.state.lock().map_err(|_| RetentionError::Unavailable)?;
        state.reap();
        Ok(state.used)
    }
}
pub(super) struct Admission {
    budget: EncodedRetentionBudget,
    key: usize,
    _pin: Arc<Vec<u8>>,
    done: bool,
}
impl Admission {
    pub(super) fn commit(mut self) -> Result<(), RetentionError> {
        let mut state = self
            .budget
            .state
            .lock()
            .map_err(|_| RetentionError::Unavailable)?;
        let entry = state
            .entries
            .get_mut(&self.key)
            .ok_or(RetentionError::Unavailable)?;
        entry.pending = entry
            .pending
            .checked_sub(1)
            .ok_or(RetentionError::Unavailable)?;
        entry.committed = true;
        self.done = true;
        Ok(())
    }
}
impl Drop for Admission {
    fn drop(&mut self) {
        if self.done {
            return;
        }
        // Poison fails closed; never panic/release a possibly-corrupt charge.
        if let Ok(mut state) = self.budget.state.lock() {
            if let Some(entry) = state.entries.get_mut(&self.key) {
                entry.pending -= 1;
                if entry.pending == 0 && !entry.committed {
                    let cost = entry.cost;
                    state.entries.remove(&self.key);
                    state.used -= cost;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capacity_identity_and_last_strong_drop_control_charges() {
        let a = Arc::new(vec![1, 2, 3]);
        let b = Arc::new(vec![1, 2, 3]);
        let budget = EncodedRetentionBudget::new(a.capacity() + b.capacity()).unwrap();
        budget.admit(&a).unwrap().commit().unwrap();
        budget.admit(&a.clone()).unwrap().commit().unwrap();
        assert_eq!(budget.used(), Ok(a.capacity()));
        budget.admit(&b).unwrap().commit().unwrap();
        assert_eq!(budget.used(), Ok(a.capacity() + b.capacity()));
        let escaped = Arc::clone(&a);
        drop(a);
        assert_eq!(budget.used(), Ok(escaped.capacity() + b.capacity()));
        drop(escaped);
        assert_eq!(budget.used(), Ok(b.capacity()));
        drop(b);
        assert_eq!(budget.used(), Ok(0));
    }
    #[test]
    fn rollback_does_not_remove_committed_or_other_pending_adopters() {
        let blob = Arc::new(vec![1, 2, 3]);
        let budget = EncodedRetentionBudget::new(blob.capacity()).unwrap();
        let first = budget.admit(&blob).unwrap();
        let second = budget.admit(&blob).unwrap();
        drop(first);
        assert_eq!(budget.used(), Ok(blob.capacity()));
        second.commit().unwrap();
        drop(budget.admit(&blob).unwrap());
        assert_eq!(budget.used(), Ok(blob.capacity()));
        let other = EncodedRetentionBudget::new(blob.capacity()).unwrap();
        drop(other.admit(&blob).unwrap());
        assert_eq!(other.used(), Ok(0));
    }
    #[test]
    fn exact_capacity_limits_fail_atomically_and_reap_before_admit() {
        for limit in [0, MAX_BYTES + 1, usize::MAX] {
            assert!(EncodedRetentionBudget::new(limit).is_err());
        }
        let a = Arc::new(vec![1, 2, 3]);
        let b = Arc::new(vec![1, 2, 3]);
        let budget = EncodedRetentionBudget::new(a.capacity()).unwrap();
        budget.admit(&a).unwrap().commit().unwrap();
        assert!(budget.admit(&b).is_err());
        assert_eq!(budget.used(), Ok(a.capacity()));
        let too_small = EncodedRetentionBudget::new(a.capacity() - 1).unwrap();
        assert!(too_small.admit(&a).is_err());
        assert_eq!(too_small.used(), Ok(0));
        drop(a);
        budget.admit(&b).unwrap().commit().unwrap();
        assert_eq!(budget.used(), Ok(b.capacity()));
    }
    #[test]
    fn competing_last_capacity_admissions_are_serialized() {
        let blobs = [Arc::new(vec![1, 2, 3]), Arc::new(vec![1, 2, 3])];
        let limit = blobs[0].capacity().max(blobs[1].capacity());
        let budget = EncodedRetentionBudget::new(limit).unwrap();
        let barrier = Arc::new(std::sync::Barrier::new(3));
        let threads: Vec<_> = blobs
            .iter()
            .map(|blob| {
                let budget = budget.clone();
                let blob = blob.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    budget.admit(&blob).and_then(Admission::commit).is_ok()
                })
            })
            .collect();
        barrier.wait();
        assert_eq!(
            threads
                .into_iter()
                .map(|t| usize::from(t.join().unwrap()))
                .sum::<usize>(),
            1
        );
        assert!(budget.used().unwrap() <= limit);
    }
    #[test]
    fn concurrent_alias_commit_and_rollback_keep_exactly_one_charge() {
        let blob = Arc::new(vec![1, 2, 3]);
        let cost = blob.capacity();
        let budget = EncodedRetentionBudget::new(cost).unwrap();
        let barrier = Arc::new(std::sync::Barrier::new(3));
        let threads: Vec<_> = [false, true]
            .into_iter()
            .map(|commit| {
                let blob = blob.clone();
                let budget = budget.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    let ticket = budget.admit(&blob).unwrap();
                    barrier.wait();
                    if commit {
                        ticket.commit().unwrap();
                    }
                })
            })
            .collect();
        barrier.wait();
        for thread in threads {
            thread.join().unwrap();
        }
        assert_eq!(budget.used(), Ok(cost));
        drop(blob);
        assert_eq!(budget.used(), Ok(0));
    }
    #[test]
    fn poisoned_domain_refuses_without_reset_or_panic() {
        let budget = EncodedRetentionBudget::new(16).unwrap();
        let state = budget.state.clone();
        assert!(std::thread::spawn(move || {
            let _lock = state.lock().unwrap();
            panic!("injected poison");
        })
        .join()
        .is_err());
        assert_eq!(budget.used(), Err(RetentionError::Unavailable));
        assert!(matches!(
            budget.admit(&Arc::new(vec![1])),
            Err(RetentionError::Unavailable)
        ));
    }
}
