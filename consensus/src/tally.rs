use std::collections::{BTreeSet, HashMap};
use crypto::Digest;
use crate::vote::Category::{Decided, Final, Initial};
use crate::vote::Value::{One, Zero};
use crate::vote::{TxHash, Vote};

#[derive(Clone, Debug)]
pub struct Tally {
    pub initial_count: usize,
    pub final_count: usize,
    pub decided_count: usize,
}

impl Tally {
    pub fn new() -> Tally {
        Tally {
            initial_count: 0,
            final_count: 0,
            decided_count: 0
        }
    }
}

pub fn tally_votes(concurrent_txs: &BTreeSet<TxHash>, votes: &BTreeSet<Vote>) -> HashMap<TxHash, Tally> {
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
