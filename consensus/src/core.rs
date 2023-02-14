use std::collections::{BTreeSet, HashMap};
use std::net::SocketAddr;
use tokio::sync::mpsc::Sender;
use bytes::Bytes;
use log::{debug, info};
use rand::Rng;
use tokio::sync::mpsc::Receiver;
use crate::constants::{NUMBER_OF_BYZANTINE_NODES, NUMBER_OF_CORRECT_NODES, QUORUM, VOTE_DELAY};
use crate::election::{Election, ElectionId};
use crate::message::Message;
use crate::Block;
use crate::round::{Round, RoundState, Timer};
use crate::tally::tally_votes;
use crate::vote::Category::{Decided, Final, Initial};
use crate::vote::{ParentHash, Transaction, TxHash, Vote, VoteState};
use crate::vote::VoteState::{Invalid, Pending, Valid};
use network::{CancelHandler, ReliableSender};
use thiserror::Error;

//#[cfg(test)]
//#[path = "tests/core_tests.rs"]
//pub mod core_tests;

use crypto::{CryptoError, Digest, PublicKey, SignatureService};
use store::{Store, StoreError};
use crate::config::Committee;
use crate::consensus::ConsensusMessage;

pub struct Core {
    id: PublicKey,
    committee: Committee,
    store: Store,
    signature_service: SignatureService,
    rx_vote: Receiver<Vote>,
    network: ReliableSender,
    elections: HashMap<ElectionId, Election>,
    byzantine: bool,
    /// Channel to receive transactions from the network.
    rx_transaction: Receiver<Transaction>,
    tx_commit: Sender<Block>,
    /// Decided txs
    decided_txs: HashMap<ElectionId, BTreeSet<(PublicKey, TxHash)>>,
    /// Keeps the cancel handlers of the messages we sent.
    cancel_handlers: HashMap<Round, Vec<CancelHandler>>,
    peers: (Vec<PublicKey>, Vec<SocketAddr>),
}

impl Core {
    #[allow(clippy::too_many_arguments)]
    pub fn spawn(
        id: PublicKey,
        committee: Committee,
        signature_service: SignatureService,
        store: Store,
        rx_message: Receiver<Vote>,
        byzantine: bool,
        rx_transaction: Receiver<Transaction>,
        tx_commit: Sender<Block>,
        peers: (Vec<PublicKey>, Vec<SocketAddr>),
    ) {
        debug!("Node {:?} is byzantine: {}", id, byzantine);
        tokio::spawn(async move {
            Self {
                id,
                committee: committee.clone(),
                signature_service,
                store,
                rx_vote: rx_message,
                network: ReliableSender::new(),
                elections: HashMap::new(),
                byzantine,
                rx_transaction,
                tx_commit,
                decided_txs: HashMap::new(),
                cancel_handlers: HashMap::new(),
                peers,
            }
            .run()
            .await
        });
    }

    fn insert_decided(&mut self, id: PublicKey, tx_hash: TxHash, parent_hash:  ParentHash) {
        match self.decided_txs.get_mut(&parent_hash) {
            Some(txs) => {
                if !txs.contains(&(id, tx_hash.clone())) {
                    txs.insert((id, tx_hash.clone()));
                }
            }
            None => {
                let mut txs = BTreeSet::new();
                txs.insert((id, tx_hash.clone()));
                self.decided_txs.insert(parent_hash.clone(), txs);
            }
        }
        debug!("Inserted decided tx: {:?}", (id, &tx_hash));
        if self.decided_txs.get(&parent_hash).unwrap().len() == NUMBER_OF_CORRECT_NODES {
            debug!("Election {:?} is finished!", &parent_hash);
            self.elections.get_mut(&parent_hash).unwrap().active = false;
        }
    }

    async fn validate_vote(&mut self, vote: &Vote, tx: &Transaction) -> VoteState {
        let concurrent_txs = self.elections.get(&tx.parent_hash).unwrap().concurrent_txs.clone();
        let mut _state = Invalid;
        let tx = vote.value.clone();
        match vote.category {
            Decided => {
                let proof_round = vote.proof_round.unwrap();
                let proof_round_state = self.elections.get_mut(&tx.parent_hash).unwrap().state.get(&proof_round).cloned();
                match proof_round_state {
                    Some(round_state) => {
                        let proof_round_votes = round_state.votes.clone();
                        let tallies = tally_votes(&concurrent_txs, &proof_round_votes);
                        let tally = tallies.get(&tx.tx_hash).unwrap();
                        if tally.final_count >= QUORUM {
                            _state = Valid;
                        }
                        else if tally.final_count < NUMBER_OF_BYZANTINE_NODES + 1 {
                            _state = Invalid;
                        }
                        else {
                            self.elections.get_mut(&tx.parent_hash).unwrap().pending_votes.insert(vote.clone());
                            _state = Pending
                        }
                    },
                    None => _state = Pending,
                }
            },
            Final => {
                let proof_round = vote.proof_round.unwrap();
                let proof_round_state = self.elections.get(&tx.parent_hash).unwrap().state.get(&proof_round).cloned();
                match proof_round_state {
                    Some(round_state) => {
                        let proof_round_votes = round_state.votes.clone();
                        let tallies = tally_votes(&concurrent_txs, &proof_round_votes);
                        let tally = tallies.get(&tx.tx_hash).unwrap();
                        if tally.initial_count >= QUORUM {
                            _state = Valid;
                        }
                        else {
                            self.elections.get_mut(&tx.parent_hash).unwrap().pending_votes.insert(vote.clone());
                            _state = Pending
                        }
                    },
                    None => _state = Pending,
                }
            },
            Initial => _state = Valid,
        }
        if (_state == Valid || _state == Invalid) && self.elections.get_mut(&tx.parent_hash).unwrap().pending_votes.contains(vote) {
            self.elections.get_mut(&tx.parent_hash).unwrap().pending_votes.remove(vote);
        }
        if _state == Pending || _state == Invalid {
            let proof_round = &vote.proof_round.unwrap();
            let rs = self.elections.get(&tx.parent_hash).unwrap().state.get(proof_round).cloned();
            match rs {
                Some(rs) => {
                    debug!("Node {}: previous round votes -> {:?}", self.id, rs.votes);
                }
                None => {
                    debug!("Node {}: round {} have not started yet!", self.id, proof_round);
                }
            }
        }
        debug!("Node {}: {:?} is {:?}", self.id, vote, _state);
        _state
    }

    async fn try_validate_pending_votes(&mut self, round: &Round, tx: &Transaction) {
        let pending_votes = self.elections.get(&tx.parent_hash).unwrap().pending_votes.clone();
        for vote in &pending_votes {
            if &vote.proof_round.unwrap() == round {
                let state = self.validate_vote(vote, tx).await;
                if state == Valid {
                    self.insert_vote(vote.clone()).await;
                }
            }
        }
    }

    async fn decide_vote(&mut self, votes: &BTreeSet<Vote>, round: Round, tx: &Transaction) -> Vote {
        if self.elections.get_mut(&tx.parent_hash).unwrap().decided_vote.is_some() {
            let mut vote = self.elections.get_mut(&tx.parent_hash).unwrap().decided_vote.as_ref().unwrap().clone();
            vote.round = round;
            return vote;
        }
        let concurrent_txs = self.elections.get(&tx.parent_hash).unwrap().concurrent_txs.clone();
        assert!(votes.len() >= QUORUM);
        let previous_round = round - 1;
        let previous_round_state = self.elections.get(&tx.parent_hash).unwrap().state.get(&previous_round).unwrap().clone();
        let tallies = tally_votes(&concurrent_txs, votes);
        debug!("Tallies: {:?}", &tallies);
        let mut highest_tally = tallies.get(&tx.tx_hash).unwrap().clone();
        let mut highest = tx.tx_hash.clone();
        for (digest, tally) in &tallies {
            if tally.decided_count > 0 {
                let proof_round = votes.iter().filter(|v| v.category == Decided).next().unwrap().proof_round.unwrap();
                let tx = Transaction::new(tx.parent_hash.clone(), digest.clone());
                let vote = Vote::new(self.id, round, tx.clone(), Decided, Some(proof_round));
                self.elections.get_mut(&tx.parent_hash).unwrap().decided_vote = Some(vote.clone());
                info!("Decided1 {:?} -> {:?}", &tx.parent_hash, &digest);
                info!("Decided2 {:?} -> {:?}", &digest, &tx.parent_hash);
                return vote
            }
            else if tally.final_count >= QUORUM {
                let tx = Transaction::new(tx.parent_hash.clone(), digest.clone());
                let vote = Vote::new(self.id, round, tx.clone(), Decided, Some(previous_round));
                self.elections.get_mut(&tx.parent_hash).unwrap().decided_vote = Some(vote.clone());
                info!("Decided1 {:?} -> {:?}", &tx.parent_hash, &digest);
                info!("Decided2 {:?} -> {:?}", &digest, &tx.parent_hash);
                return vote
            } else if tally.final_count > 0 {
                let final_votes: Vec<&Vote> = votes.iter().filter(|v| v.category == Final).collect();
                let mut final_vote = &final_votes[0].clone();
                for vote in &final_votes {
                    if vote.proof_round.unwrap() > final_vote.proof_round.unwrap() {
                        final_vote = vote.clone();
                    }
                }
                return if previous_round_state.final_vote.is_some() {
                    let old_vote = previous_round_state.final_vote.as_ref().unwrap().clone();
                    let old_proof_round = previous_round_state.final_vote.as_ref().unwrap().proof_round.unwrap().clone();
                    let new_proof_round = final_vote.proof_round.unwrap();
                    if new_proof_round > old_proof_round {
                        Vote::new(self.id, round, final_vote.value.clone(), Final, final_vote.proof_round)
                    } else {
                        Vote::new(self.id, round, old_vote.value, Final, old_vote.proof_round)
                    }
                } else {
                    Vote::new(self.id, round, final_vote.value.clone(), Final, final_vote.proof_round)
                }
            } else if tally.initial_count >= QUORUM {
                return Vote::new(self.id, round, tx.clone(), Final, Some(previous_round))
            } else {
                if tally.initial_count > highest_tally.initial_count {
                    highest_tally = tally.clone();
                    highest = digest.clone();
                }
                // if tie, vote for the highest hash
                if tally.initial_count == highest_tally.initial_count {
                    if digest.clone() > highest {
                        highest_tally = tally.clone();
                        highest = digest.clone();
                    }
                }
            }
        }
        Vote::new(self.id, round, Transaction::new(tx.parent_hash.clone(), highest.clone()), Initial, None)
    }

    async fn start_election(&mut self, vote: Vote) {
        match self.elections.get(&vote.value.parent_hash) {
            Some(election) => (),
            None => {
                let election = Election::new(&vote.value.parent_hash);
                if !self.elections.contains_key(&vote.value.parent_hash) {
                    info!("Created {:?}", &vote.value.parent_hash);
                }
                self.elections.insert(vote.value.parent_hash, election);
            }
        }
    }

    async fn start_round(&mut self, vote: Vote) {
        if let None = self.elections.get(&vote.value.parent_hash).unwrap().state.get(&vote.round) {
            let round_state = RoundState::new(vote.round, &vote.value.parent_hash);
            debug!("Started round {:?} of election {:?}", &vote.round, &vote.value.parent_hash);
            self.elections.get_mut(&vote.value.parent_hash).unwrap().state.insert(vote.round,round_state);
        }
    }

    async fn insert_vote(&mut self, vote: Vote) {
        debug!("Inserted {:?}", vote);
        let mut contains = false;
        if let Some(e) = self.decided_txs.get(&vote.value.parent_hash) {
            contains = e.contains(&(vote.signer.clone(), vote.value.tx_hash.clone()));
        }
        if vote.category == Decided && !contains {
            self.insert_decided(vote.signer.clone(), vote.value.tx_hash.clone(), vote.value.parent_hash.clone());
        }
        self.elections.get_mut(&vote.value.parent_hash).unwrap().state.get_mut(&vote.round).unwrap().votes.insert(vote);
    }

    async fn handle_vote(&mut self, vote: Vote) {
        debug!("Received {:?}", &vote);
        self.start_election(vote.clone()).await;
        self.start_round(vote.clone()).await;
        let vote_state = self.validate_vote(&vote, &vote.value).await;
        if vote_state == Valid {
            self.elections.get_mut(&vote.value.parent_hash).unwrap().concurrent_txs.insert(vote.value.tx_hash.clone());
            self.insert_vote(vote.clone()).await;
            //self.send_vote(vote.clone()).await;
            self.try_validate_pending_votes(&vote.round, &vote.value).await;
        }
        let next_round = vote.round + 1;
        let mut voted_next_round = false;
        if let Some(rs) = self.elections.get_mut(&vote.value.parent_hash).unwrap().state.get(&(vote.round+1)) {
            voted_next_round = rs.voted;
        }
        let votes = self.elections.get(&vote.value.parent_hash).unwrap().state.get(&vote.round).unwrap().votes.clone();
        let election_active = self.elections.get(&vote.value.parent_hash).unwrap().active;
        if votes.len() >= QUORUM && !voted_next_round && election_active {
            {
                let &(ref mutex, ref cvar) = &*self.elections.get(&vote.value.parent_hash).unwrap().state.get(&vote.round).unwrap().timer;
                let value = mutex.lock().unwrap();
                let mut value = value;
                while *value == Timer::Active {
                    debug!("Node {}: waiting for the round {} to expire...", self.id, &vote.round);
                    value = cvar.wait(value).unwrap();
                }
            }
            let vote = self.decide_vote(&votes, next_round, &vote.value).await;
            self.start_round(vote.clone()).await;
            self.elections.get_mut(&vote.value.parent_hash).unwrap().state.get_mut(&next_round).unwrap().voted = true;
            self.send_vote(vote.clone()).await;
        }
    }

    async fn send_vote(&mut self, vote: Vote) {
        let mut vote = vote;
        if self.byzantine {
            vote = Vote::random(self.id, vote.value.parent_hash, vote.round);
        }
        let rand = rand::thread_rng().gen_range(0..VOTE_DELAY as u64);
        //sleep(Duration::from_millis(rand));
        let msg = Message::new(self.id, vote.clone());
        debug!("Sent {:?}", &vote);
        let message = bincode::serialize(&ConsensusMessage::Message(msg))
            .expect("Failed to serialize vote");
        let handlers = self.network.broadcast(self.peers.1.clone(), Bytes::from(message)).await;
        self.cancel_handlers
            .entry(vote.round)
            .or_insert_with(Vec::new)
            .extend(handlers);
    }

    async fn run(&mut self) {
        loop {
            tokio::select! {
                Some(vote) = self.rx_vote.recv() => {
                    self.handle_vote(vote.clone()).await;
                },
                Some(transaction) = self.rx_transaction.recv() => {
                    info!("Received {:?}", &transaction);
                    let vote = Vote::new(self.id, 0, transaction.clone(), Initial, None);
                    self.send_vote(vote).await;
                },
            };

            // Give the change to schedule other tasks.
            tokio::task::yield_now().await;
        }
    }
}

pub type DagResult<T> = Result<T, DagError>;

#[derive(Debug, Error)]
pub enum DagError {
    #[error("Invalid signature")]
    InvalidSignature(#[from] CryptoError),

    #[error("Storage failure: {0}")]
    StoreError(#[from] StoreError),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] Box<bincode::ErrorKind>),
}

