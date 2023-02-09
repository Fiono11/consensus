use rand::Rng;
use crate::round::Round;
use crate::vote::Category::{Decided, Final, Initial};
use crate::vote::Value::{One, Zero};
use serde::{Deserialize, Serialize};
use crypto::{Digest, PublicKey};

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

#[derive(Clone, Serialize, Deserialize, Ord, PartialOrd, Eq, PartialEq, Debug, Hash)]
pub struct ParentHash(pub Digest);

#[derive(Clone, Serialize, Deserialize, Ord, PartialOrd, Eq, PartialEq, Debug, Hash)]
pub struct TxHash(pub Digest);

#[derive(Clone, Serialize, Deserialize, Ord, PartialOrd, Eq, PartialEq, Debug)]
pub struct Transaction {
    pub parent_hash: ParentHash,
    pub tx_hash: TxHash,
}

impl Transaction {
    pub fn new(parent_hash: ParentHash, tx_hash: TxHash) -> Self {
        Self {
            parent_hash, tx_hash
        }
    }

    pub fn default() -> Self {
        Self {
            parent_hash: ParentHash(Digest::default()),
            tx_hash: TxHash(Digest::random()),
        }
    }

    pub fn random() -> Self {
        Self {
            parent_hash: ParentHash(Digest::random()),
            tx_hash: TxHash(Digest::random()),
        }
    }
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
    pub(crate) fn random(id: PublicKey, parent_hash: ParentHash, round: Round) -> Vote {
        let tx_hash = TxHash(Digest::random());
        let tx = Transaction::new(parent_hash, tx_hash);
        Vote::new(id, round, tx, Initial, None)
    }
}

#[derive(Clone, Debug, Ord, PartialOrd, Eq, PartialEq)]
pub enum VoteState {
    Valid,
    Invalid,
    Pending,
}