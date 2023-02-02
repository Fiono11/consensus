use std::collections::{BTreeSet, HashMap};
use crypto::Digest;
use crate::round::{Round, RoundState};
use crate::vote::Vote;

#[derive(Debug, Clone)]
pub struct Election {
    pub(crate) concurrent_txs: BTreeSet<Digest>,
    pub(crate) state: HashMap<Round, RoundState>,
    pub(crate) decided_vote: Option<Vote>,
    pub(crate) pending_votes: BTreeSet<Vote>,
}

impl Election {
    pub(crate) fn new() -> Election {
        Election {
            concurrent_txs: Default::default(),
            state: HashMap::new(),
            decided_vote: None,
            pending_votes: BTreeSet::new(),
        }
    }
}