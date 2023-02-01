use std::collections::{BTreeSet, HashMap};
use crate::round::{Round, RoundState};
use crate::vote::Vote;

#[derive(Debug)]
pub struct Election {
    pub(crate) state: HashMap<Round, RoundState>,
    pub(crate) decided_vote: Option<Vote>,
    pub(crate) pending_votes: BTreeSet<Vote>,
}

impl Election {
    pub(crate) fn new() -> Election {
        Election {
            state: HashMap::new(),
            decided_vote: None,
            pending_votes: BTreeSet::new(),
        }
    }
}