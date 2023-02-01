use std::collections::BTreeSet;
use crate::vote::Category::{Decided, Final, Initial};
use crate::vote::Value::{One, Zero};
use crate::vote::Vote;

pub struct Tally {
    pub initial_zeros: usize,
    pub initial_ones: usize,
    pub final_zeros: usize,
    pub final_ones: usize,
    pub decided_zeros: usize,
    pub decided_ones: usize,
}

impl Tally {
    pub fn new() -> Tally {
        Tally {
            initial_zeros: 0,
            initial_ones: 0,
            final_zeros: 0,
            final_ones: 0,
            decided_zeros: 0,
            decided_ones: 0,
        }
    }
}

pub fn tally_votes(votes: &BTreeSet<Vote>) -> Tally {
    let mut tally = Tally::new();
    for vote in votes {
        if vote.value == Zero && vote.category == Initial {
            tally.initial_zeros += 1;
        }
        else if vote.value == One && vote.category == Initial {
            tally.initial_ones += 1;
        }
        else if vote.value == Zero && vote.category == Final {
            tally.final_zeros += 1;
        }
        else if vote.value == One && vote.category == Final {
            tally.final_ones += 1;
        }
        else if vote.value == Zero && vote.category == Decided {
            tally.decided_zeros += 1;
        }
        else if vote.value == One && vote.category == Decided {
            tally.decided_ones += 1;
        }
    }
    tally
}
