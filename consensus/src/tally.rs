use std::collections::{BTreeSet, HashMap};
use crypto::Digest;
use crate::vote::Category::{Decided, Final, Initial};
use crate::vote::Value::{One, Zero};
use crate::vote::Vote;

pub struct Tally {
    pub initial_count: usize,
    pub final_count: usize,
    pub decided_count: usize,
    /*pub initial_zeros: usize,
    pub initial_ones: usize,
    pub final_zeros: usize,
    pub final_ones: usize,
    pub decided_zeros: usize,
    pub decided_ones: usize,*/
}

impl Tally {
    pub fn new() -> Tally {
        Tally {
            /*initial_zeros: 0,
            initial_ones: 0,
            final_zeros: 0,
            final_ones: 0,
            decided_zeros: 0,
            decided_ones: 0,*/
            initial_count: 0,
            final_count: 0,
            decided_count: 0
        }
    }
}

pub fn tally_votes(concurrent_txs: &BTreeSet<Digest>, votes: &BTreeSet<Vote>) -> HashMap<Digest, Tally> {
    let mut tallies = HashMap::new();
    for tx in concurrent_txs {
        let mut tally = Tally::new();
        for vote in votes {
            if &vote.value.tx_hash == tx && vote.category == Initial {
                tally.initial_count += 1;
            }
            else if &vote.value.tx_hash == tx && vote.category == Final {
                tally.final_count += 1;
            }
            else if &vote.value .tx_hash == tx && vote.category == Decided {
                tally.decided_count += 1;
            }
        }
        tallies.insert(tx.clone(), tally);
    }
    tallies
}
