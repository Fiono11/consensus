use crypto::PublicKey;
use crate::vote::Vote;

#[derive(Debug, Clone)]
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
