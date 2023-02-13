use std::collections::{BTreeSet, HashMap};
use crypto::Digest;
use crate::round::{Round, RoundState};
use crate::vote::{ParentHash, TxHash, Vote};

pub type ElectionId = ParentHash;

#[derive(Debug, Clone)]
pub struct Election {
    pub(crate) concurrent_txs: BTreeSet<TxHash>,
    pub(crate) state: HashMap<Round, RoundState>,
    pub(crate) decided_vote: Option<Vote>,
    pub(crate) pending_votes: BTreeSet<Vote>,
    pub(crate) active: bool,
}

impl Election {
    pub(crate) fn new(election_id: &ElectionId) -> Election {
        let mut state = HashMap::new();
        state.insert(0, RoundState::new(0, election_id));
        Election {
            concurrent_txs: BTreeSet::new(),
            state,
            decided_vote: None,
            pending_votes: BTreeSet::new(),
            active: true,
        }
    }
}