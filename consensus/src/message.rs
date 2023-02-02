use crate::vote::Vote;
use serde::{Deserialize, Serialize};
use crypto::{Digest, PublicKey};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub(crate) sender: PublicKey,
    pub(crate) receiver: PublicKey,
    pub(crate) vote: Vote,
}

impl Message {
    pub(crate) fn new(sender: PublicKey, receiver: PublicKey, vote: Vote) -> Message {
        Message {
            sender, receiver, vote,
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Ord, PartialOrd, Eq, PartialEq, Debug)]
pub struct Transaction {
    pub parent_hash: Digest,
    pub tx_hash: Digest,
}

impl Transaction {
    pub fn new(parent_hash: Digest, tx_hash: Digest) -> Self {
        Self {
            parent_hash, tx_hash
        }
    }

    pub fn random() -> Self {
        Self {
            parent_hash: Digest::random(),
            tx_hash: Digest::random(),
        }
    }
}
