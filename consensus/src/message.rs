use std::fmt::Debug;
use crypto::PublicKey;
use crate::election::ElectionId;
use crate::round::{Round, Timer};
use crate::vote::Vote;

#[derive(Debug, Clone)]
pub enum Messages {
    Message(Message),
    Timer(TimerMessage),
}

#[derive(Debug, Clone)]
pub struct TimerMessage {
    pub(crate) sender: PublicKey,
    pub(crate) receiver: PublicKey,
    pub(crate) election_id: ElectionId,
    pub(crate) round: Round,
}

impl TimerMessage {
    pub(crate) fn new(sender: PublicKey, receiver: PublicKey, election_id: ElectionId, round: Round) -> TimerMessage {
        TimerMessage {
            sender, receiver, election_id, round,
        }
    }
}

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
