use crate::node::Id;
use crate::vote::Vote;

#[derive(Debug, Clone)]
pub struct Message {
    pub(crate) sender: Id,
    pub(crate) receiver: Id,
    pub(crate) vote: Vote,
}

impl Message {
    pub(crate) fn new(sender: Id, receiver: Id, vote: Vote) -> Message {
        Message {
            sender, receiver, vote,
        }
    }
}
