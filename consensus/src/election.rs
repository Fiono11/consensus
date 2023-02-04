use std::collections::{BTreeSet, HashMap};
use crypto::Digest;
use crate::round::{Round, RoundState};
use crate::vote::{TxHash, Vote};

#[derive(Debug, Clone)]
pub struct Election {
    pub(crate) concurrent_txs: BTreeSet<TxHash>,
    pub(crate) state: HashMap<Round, RoundState>,
    pub(crate) decided_vote: Option<Vote>,
    pub(crate) pending_votes: BTreeSet<Vote>,
}

impl Election {
    pub(crate) fn new() -> Election {
        let mut state = HashMap::new();
        state.insert(0, RoundState::new());
        Election {
            concurrent_txs: BTreeSet::new(),
            state,
            decided_vote: None,
            pending_votes: BTreeSet::new(),
        }
    }
}