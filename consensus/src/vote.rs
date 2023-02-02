use rand::Rng;
use crate::round::Round;
use crate::vote::Category::{Decided, Final, Initial};
use crate::vote::Value::{One, Zero};
use serde::{Deserialize, Serialize};
use crypto::{Digest, PublicKey};
use crate::Transaction;

#[derive(Clone, Debug, Ord, PartialOrd, Eq, PartialEq, Serialize, Deserialize)]
pub enum Value {
    Zero,
    One,
}

#[derive(Clone, Debug, Ord, PartialOrd, Eq, PartialEq, Serialize, Deserialize)]
pub enum Category {
    Initial,
    Final,
    Decided,
}

#[derive(Clone, Debug, Ord, PartialOrd, Eq, PartialEq, Serialize, Deserialize)]
pub struct Vote {
    pub(crate) signer: PublicKey,
    pub(crate) round: Round,
    pub(crate) value: Transaction,
    pub(crate) category: Category,
    pub(crate) proof_round: Option<Round>
}

impl Vote {
    pub(crate) fn new(id: PublicKey, round: Round, value: Transaction, category: Category, proof_round: Option<Round>) -> Vote {
        Vote {
            signer: id, round, value, category, proof_round,
        }
    }

    /*pub(crate) fn random_initial(id: PublicKey) -> Vote {
        if rand::thread_rng().gen_range(0..2) == 0 {
            Vote::new(id, 0, Zero, Initial, None)
        } else {
            Vote::new(id, 0, One, Initial, None)
        }
    }

    pub(crate) fn random(id: PublicKey, round: Round) -> Option<Vote> {
        let rand = rand::thread_rng().gen_range(0..7);
        let rand_proof_round = rand::thread_rng().gen_range(0..round);
        if rand == 0 {
            Some(Vote::new(id, round, Zero, Initial, None))
        } else if rand == 1 {
            Some(Vote::new(id, round, One, Initial, None))
        }
        else if rand == 2 {
            Some(Vote::new(id, round, One, Final, Some(rand_proof_round)))
        }
        else if rand == 3 {
            Some(Vote::new(id, round, Zero, Final, Some(rand_proof_round)))
        }
        else if rand == 4 {
            Some(Vote::new(id, round, One, Decided, Some(rand_proof_round)))
        }
        else if rand == 5 {
            Some(Vote::new(id, round, Zero, Decided, Some(rand_proof_round)))
        }
        else {
            None
        }
    }**/
}

#[derive(Clone, Debug, Ord, PartialOrd, Eq, PartialEq)]
pub enum VoteState {
    Valid,
    Invalid,
    Pending,
}