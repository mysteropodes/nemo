//! Commit one set of admission tickets under one ledger lock, or commit none.
use crate::revision::object_revision::encoded_retention_budget::{Admission, RetentionError};
use std::collections::HashMap;
use std::sync::Arc;

impl Admission {
    pub(crate) fn commit_all(mut tickets: Vec<Self>) -> Result<(), RetentionError> {
        let Some(first) = tickets.first() else {
            return Ok(());
        };
        let budget = first.budget.clone();
        let mut counts = HashMap::<usize, usize>::new();
        counts
            .try_reserve(tickets.len())
            .map_err(|_| RetentionError::Allocation)?;
        for ticket in &tickets {
            if ticket.done || !Arc::ptr_eq(&budget.state, &ticket.budget.state) {
                return Err(RetentionError::Unavailable);
            }
            *counts.entry(ticket.key).or_default() += 1;
        }
        let mut state = budget
            .state
            .lock()
            .map_err(|_| RetentionError::Unavailable)?;
        for (key, count) in &counts {
            if state
                .entries
                .get(key)
                .is_none_or(|entry| entry.pending < *count)
            {
                return Err(RetentionError::Unavailable);
            }
        }
        // Every potentially fallible step precedes the first ledger mutation.
        for (key, count) in counts {
            let entry = state
                .entries
                .get_mut(&key)
                .expect("validated under the same lock");
            entry.pending -= count;
            entry.committed = true;
        }
        for ticket in &mut tickets {
            ticket.done = true;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "../tests/encoded_retention_batch.rs"]
mod tests;
