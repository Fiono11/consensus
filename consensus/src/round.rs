use std::collections::BTreeSet;
use std::sync::{Arc, Condvar, Mutex};
use crate::round::Timer::Active;
use crate::vote::Vote;

#[derive(Debug, Clone, PartialEq)]
pub enum Timer {
    Active,
    Expired,
}

pub type Round = u8;

#[derive(Debug, Clone)]
pub struct RoundState {
    pub(crate) votes: BTreeSet<Vote>,
    pub(crate) voted: bool,
    pub(crate) final_vote: Option<Vote>,
    pub(crate) timer: Arc<Mutex<Timer>>,
}

impl RoundState {
    pub(crate) fn new() -> RoundState {
        RoundState {
            votes: BTreeSet::new(),
            voted: false,
            timer: Arc::new((Mutex::new(Active))),
            final_vote: None,
        }
    }
}