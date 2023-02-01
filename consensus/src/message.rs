use crate::vote::Vote;
use serde::{Deserialize, Serialize};
use crypto::PublicKey;

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
