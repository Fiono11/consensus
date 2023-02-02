use std::collections::{BTreeSet, HashMap};
use std::error::Error;
use std::net::SocketAddr;
use std::sync::{Arc, Condvar, Mutex};
use tokio::sync::mpsc::Sender;
use std::thread;
use std::thread::sleep;
use std::time::Duration;
use bytes::Bytes;
use futures::task::SpawnExt;
use log::{debug, error, info, warn};
use rand::{Rng, thread_rng};
use tokio::sync::mpsc::{channel, Receiver};
use crate::constants::{NUMBER_OF_BYZANTINE_NODES, QUORUM, ROUND_TIMER, VOTE_DELAY};
use crate::election::Election;
use crate::message::Message;
use crate::{Block, NUMBER_OF_NODES, Transaction};
use crate::round::{Round, RoundState, Timer};
use crate::tally::tally_votes;
use crate::vote::Category::{Decided, Final, Initial};
use crate::vote::Value::{One, Zero};
use crate::vote::{Value, Vote, VoteState};
use crate::vote::VoteState::{Invalid, Pending, Valid};
use network::{MessageHandler, Receiver as NetworkReceiver, Writer};
use async_recursion::async_recursion;

//#[cfg(test)]
//#[path = "tests/core_tests.rs"]
//pub mod core_tests;

use crypto::{Digest, PublicKey, SignatureService};
use network::SimpleSender;
use store::Store;
use crate::config::Committee;
use crate::consensus::{CHANNEL_CAPACITY, ConsensusMessage};
use crate::error::ConsensusError;

pub struct Core {
    id: PublicKey,
    committee: Committee,
    store: Store,
    signature_service: SignatureService,
    rx_message: Receiver<ConsensusMessage>,
    network: SimpleSender,
    elections: HashMap<Digest, Election>,
    byzantine: bool,
    /// Channel to receive transactions from the network.
    rx_transaction: Receiver<Transaction>,
    tx_commit: Sender<Block>,
    /// Decided txs
    decided_txs: HashMap<PublicKey, Digest>,
}

impl Core {
    #[allow(clippy::too_many_arguments)]
    pub fn spawn(
        id: PublicKey,
        committee: Committee,
        signature_service: SignatureService,
        store: Store,
        rx_message: Receiver<ConsensusMessage>,
        byzantine: bool,
        rx_transaction: Receiver<Transaction>,
        tx_commit: Sender<Block>,
    ) {
        tokio::spawn(async move {
            Self {
                id,
                committee: committee.clone(),
                signature_service,
                store,
                rx_message,
                network: SimpleSender::new(),
                elections: HashMap::new(),
                byzantine,
                rx_transaction,
                tx_commit,
                decided_txs: HashMap::new(),
            }
            .run()
            .await
        });
    }

    pub(crate) fn start_new_round(&mut self, round: Round, tx: &Transaction) {
        let election = self.elections.get_mut(&tx.parent_hash).unwrap();
        debug!("Node {}: started round {}!", self.id, round);
        let round_state = RoundState::new();
        let timer = Arc::clone(&round_state.timer);
        election.state.insert(round, round_state);
        self.start_timer(timer, round);
    }

    fn start_timer(&self, timer: Arc<(Mutex<Timer>, Condvar)>, round: Round) {
        let id = self.id;
        thread::spawn(move || {
            //sleep(Duration::from_millis(ROUND_TIMER as u64));
            debug!("Node {}: round {} expired!", id, round);
            let &(ref mutex, ref cvar) = &*timer;
            let mut value = mutex.lock().unwrap();
            *value = Timer::Expired;
            cvar.notify_one();
        });
    }

    pub(crate) async fn handle_message(&mut self, msg: &Message) {
        let e = self.elections.get(&msg.vote.value.parent_hash).cloned();
        match e {
            Some(election) => {
                self.handle_vote(msg.sender, msg.vote.clone(), &election).await
            }
            None => {
                self.handle_vote(msg.sender, msg.vote.clone(), &Election::new()).await
            }
        }
    }

    //#[async_recursion]
    async fn handle_vote(&mut self, from: PublicKey, vote: Vote, election: &Election) {
        //let election = self.elections.get_mut(&tx.parent_hash).unwrap();
        debug!("Node {}: received {:?} from node {}", self.id, vote, from);
        let tx = &vote.value;
        let round = vote.round;
        if self.validate_vote(&vote, tx) == Valid {
            self.send_vote(vote.clone());
            self.insert_vote(vote.clone(), tx);
            self.try_validate_pending_votes(&round, tx);
            let round_state = election.state.get(&round).unwrap().clone();
            let votes = &round_state.votes;
            let mut voted_next_round = false;
            if let Some(round_state) = election.state.get(&(round + 1)) {
                voted_next_round = round_state.voted;
            }
            if votes.len() >= QUORUM && !voted_next_round && self.decided_txs.len() != NUMBER_OF_NODES {
                /*let &(ref mutex, ref cvar) = &*round_state.timer;
                let value = mutex.lock().unwrap();
                let mut value = value;
                while *value == Timer::Active {
                    debug!("Node {}: waiting for the round {} to expire...", self.id, round);
                    value = cvar.wait(value).unwrap();
                }*/
                if election.decided_vote.is_some() {
                    let mut vote = election.decided_vote.as_ref().unwrap().clone();
                    vote.round = round + 1;
                    self.send_vote(vote).await;
                } else {
                    self.vote_next_round(round + 1, votes,&vote.value).await;
                }
            }
        }
    }

    fn insert_vote(&mut self, vote: Vote, tx: &Transaction) {
        let round = vote.round;
        //let election = self.elections.get_mut(&tx.parent_hash).unwrap();
        match election.state.get_mut(&round) {
            Some(round_state) => {
                debug!("Node {}: inserted {:?}", self.id, &vote);
                round_state.votes.insert(vote.clone());
            }
            None => {
                self.start_new_round(round, tx);
                debug!("Node {}: inserted {:?}", self.id, &vote);
                election.state.get_mut(&round).unwrap().votes.insert(vote.clone());
            }
        }
        debug!("Node {}: votes of round {} -> {:?}", self.id, &vote.round, &election.state.get(&vote.round));
    }

    #[async_recursion]
    async fn vote_next_round(&mut self, round: Round, votes: &BTreeSet<Vote>, tx: &Transaction) {
        let election = self.elections.get_mut(&tx.parent_hash).unwrap();
        let concurrent_txs: Vec<Digest> = self.elections.get(&tx.parent_hash).unwrap().concurrent_txs.clone().into_iter().collect();
        let rand = thread_rng().gen_range(0..concurrent_txs.len() + 1);
        let rand2 = thread_rng().gen_range(0..3);
        let mut category = Initial;
        if rand2 == 0 {
            category = Final;
        }
        if rand2 == 1 {
            category = Decided;
        }
        let mut vote = None;
        let rand3 = thread_rng().gen_range(0..round-1);
        if rand != 0 {
            vote = Some(Vote::new(self.id, round, Transaction::new(tx.parent_hash.clone(), concurrent_txs[rand].clone()), category, Some(rand3)));
        }
        if !self.byzantine {
            vote = Some(self.decide_vote(&votes, round, tx));
        }
        if election.state.get_mut(&round).is_none() {
            self.start_new_round(round, tx);
        }
        let mut round_state = election.state.get_mut(&round).unwrap();
        round_state.voted = true;
        if vote.is_some() {
            let vote = vote.unwrap();
            round_state.votes.insert(vote.clone());
            self.send_vote(vote).await;
        }
    }

    fn validate_vote(&mut self, vote: &Vote, tx: &Transaction) -> VoteState {
        let election = self.elections.get_mut(&tx.parent_hash).unwrap();
        let mut _state = Invalid;
        let tx = vote.value.clone();
        let concurrent_txs = self.elections.get(&tx.parent_hash).unwrap().concurrent_txs.clone();
        match vote.category {
            Decided => {
                let proof_round = vote.proof_round.unwrap();
                let proof_round_state = election.state.get(&proof_round);
                match proof_round_state {
                    Some(round_state) => {
                        let proof_round_votes = round_state.votes.clone();
                        let tallies = tally_votes(&concurrent_txs, &proof_round_votes);
                        let tally = tallies.get(&tx.tx_hash).unwrap();
                        if tally.final_count >= QUORUM {
                            if !self.decided_txs.contains_key(&vote.signer) {
                                self.decided_txs.insert(vote.signer, tx.tx_hash);
                                info!("Inserted {:?}", &vote);
                            }
                            _state = Valid;
                        }
                        else if tally.final_count < NUMBER_OF_BYZANTINE_NODES + 1 {
                            _state = Invalid;
                        }
                        else {
                            election.pending_votes.insert(vote.clone());
                            _state = Pending
                        }
                    },
                    None => _state = Pending,
                }
            },
            Final => {
                let proof_round = vote.proof_round.unwrap();
                let proof_round_state = election.state.get(&proof_round);
                match proof_round_state {
                    Some(round_state) => {
                        let proof_round_votes = round_state.votes.clone();
                        let tallies = tally_votes(&concurrent_txs, &proof_round_votes);
                        let tally = tallies.get(&tx.tx_hash).unwrap();
                        if tally.initial_count >= QUORUM {
                            _state = Valid;
                        }
                        else {
                            election.pending_votes.insert(vote.clone());
                            _state = Pending
                        }
                    },
                    None => _state = Pending,
                }
            },
            Initial => _state = Valid,
        }
        if (_state == Valid || _state == Invalid) && election.pending_votes.contains(vote) {
            election.pending_votes.remove(vote);
        }
        if _state == Pending || _state == Invalid {
            let proof_round = &vote.proof_round.unwrap();
            let rs = election.state.get(proof_round);
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

    fn try_validate_pending_votes(&mut self, round: &Round, tx: &Transaction) {
        let election = self.elections.get(&tx.parent_hash).unwrap();
        let pending_votes = election.pending_votes.clone();
        for vote in &pending_votes {
            if &vote.proof_round.unwrap() == round {
                let state = self.validate_vote(vote, tx);
                if state == Valid {
                    self.insert_vote(vote.clone(), tx);
                }
            }
        }
    }

    fn decide_vote(&mut self, votes: &BTreeSet<Vote>, round: Round, tx: &Transaction) -> Vote {
        let election = self.elections.get_mut(&tx.parent_hash).unwrap();
        let concurrent_txs = self.elections.get(&tx.parent_hash).unwrap().concurrent_txs.clone();
        assert!(votes.len() >= QUORUM);
        let previous_round = round - 1;
        let previous_round_state = election.state.get(&previous_round).unwrap();
        let tallies = tally_votes(&concurrent_txs, votes);
        let mut highest_tally = tallies.get(&tx.tx_hash).unwrap();
        let mut highest = tx.tx_hash.clone();
        for (digest, tally) in &tallies {
            if tally.decided_count > 0 {
                let proof_round = votes.iter().filter(|v| v.category == Decided).next().unwrap().proof_round.unwrap();
                let vote = Vote::new(self.id, round, tx.clone(), Decided, Some(proof_round));
                election.decided_vote = Some(vote.clone());
                if !self.decided_txs.contains_key(&vote.signer) {
                    self.decided_txs.insert(self.id, tx.tx_hash.clone());
                    info!("Decided {:?}", &tx.tx_hash);
                }
                return vote
            }
            else if tally.final_count >= QUORUM {
                let vote = Vote::new(self.id, round, tx.clone(), Decided, Some(previous_round));
                election.decided_vote = Some(vote.clone());
                if !self.decided_txs.contains_key(&vote.signer) {
                    self.decided_txs.insert(self.id, tx.tx_hash.clone());
                    info!("Decided {:?}", &tx.tx_hash);
                }
                return vote
            } else if tally.final_count > 0 {
                let final_votes: Vec<&Vote> = votes.iter().filter(|v| v.category == Final).collect();
                let mut final_vote = &final_votes[0].clone();
                for vote in &final_votes {
                    if vote.proof_round.unwrap() > final_vote.proof_round.unwrap() {
                        final_vote = vote.clone();
                    }
                }
                if previous_round_state.final_vote.is_some() {
                    let old_vote = previous_round_state.final_vote.as_ref().unwrap().clone();
                    let old_proof_round = previous_round_state.final_vote.as_ref().unwrap().proof_round.unwrap().clone();
                    let new_proof_round = final_vote.proof_round.unwrap();
                    if new_proof_round > old_proof_round {
                        return Vote::new(self.id, round, final_vote.value.clone(), Final, final_vote.proof_round)
                    } else {
                        return Vote::new(self.id, round, old_vote.value, Final, old_vote.proof_round)
                    }
                } else {
                    return Vote::new(self.id, round, final_vote.value.clone(), Final, final_vote.proof_round)
                }
            } else if tally.initial_count >= QUORUM {
                return Vote::new(self.id, round, tx.clone(), Final, Some(previous_round))
            } else {
                if tally.initial_count > highest_tally.initial_count {
                    highest_tally = tally.clone();
                    highest = digest.clone();
                }
            }
        }
        Vote::new(self.id, round, Transaction::new(tx.parent_hash.clone(), highest.clone()), Initial, None)
        // if tie, vote for the highest hash
    }

    #[async_recursion]
    pub(crate) async fn send_vote(&mut self, vote: Vote) {
        let round = vote.round;
        let broadcast_addresses = self
            .committee
            .broadcast_addresses(&self.id);
        let mut addresses: Vec<SocketAddr> = Vec::new();
        let mut public_keys: Vec<PublicKey> = Vec::new();
        for (pk, sa) in broadcast_addresses {
            addresses.push(sa);
            public_keys.push(pk);
        }
        //if !self.byzantine {
            for i in 0..NUMBER_OF_NODES {
                let msg = Message::new(self.id, public_keys[i], vote.clone());
                let rand = rand::thread_rng().gen_range(0..VOTE_DELAY as u64);
                //sleep(Duration::from_millis(rand));
                let message = bincode::serialize(&ConsensusMessage::Message(msg))
                    .expect("Failed to serialize vote");
                self.network.send(addresses[i], Bytes::from(message)).await;
                debug!("Node {}: sent {:?} to {}", self.id, &vote, i);
            }
        //}
        /*else {
            if round != 0 {
                for i in 0..NUMBER_OF_NODES {
                    let vote = Vote::random(self.id, round);
                    if vote.is_some() {
                        debug!("Node {}: sent {:?} to {}", self.id, &vote, i);
                        let msg = Message::new(self.id, public_keys[i],vote.unwrap().clone());
                        let message = bincode::serialize(&ConsensusMessage::Message(msg))
                            .expect("Failed to serialize vote");
                        self.network.send(addresses[i], Bytes::from(message)).await;
                    }
                }
            }
        }*/
    }

    pub async fn run(&mut self) {
        // Upon booting, generate the very first block (if we are the leader).
        // Also, schedule a timer in case we don't hear from the leader.
        //self.timer.reset();
        //if self.name == self.leader_elector.get_leader(self.round) {
            //self.generate_proposal(None).await;
        //}

        // This is the main loop: it processes incoming blocks and votes,
        // and receive timeout notifications from our Timeout Manager.
        loop {
            let result = tokio::select! {
                Some(message) = self.rx_message.recv() => match message {
                    //ConsensusMessage::Propose(block) => self.handle_proposal(&block).await,
                    ConsensusMessage::Message(msg) => {
                        let e = self.elections.get(&msg.vote.value.parent_hash).cloned();
                        match e {
                            Some(election) => {
                                self.handle_vote(msg.sender, msg.vote.clone(), &election).await
                            }
                            None => {
                                self.handle_vote(msg.sender, msg.vote.clone(), &Election::new()).await
                            }
                        }
                    }
                    //ConsensusMessage::Timeout(timeout) => self.handle_timeout(&timeout).await,
                    //ConsensusMessage::TC(tc) => self.handle_tc(tc).await,
                    _ => panic!("Unexpected protocol message")
                },
                Some(transaction) = self.rx_transaction.recv() => {
                    info!("Received {:?}", transaction.tx_hash);
                    //let vote = Vote::random_initial(self.id);
                    let vote = Vote::new(self.id, 0, transaction.clone(), Initial, None);
                    self.send_vote(vote).await;
                },
                //Some(block) = self.rx_loopback.recv() => self.process_block(&block).await,
                //() = &mut self.timer => self.local_timeout_round().await,
            };
            /*match result {
                Ok(()) => (),
                Err(ConsensusError::StoreError(e)) => error!("{}", e),
                Err(ConsensusError::SerializationError(e)) => error!("Store corrupted. {}", e),
                Err(e) => warn!("{}", e),
            }*/
        }
    }
}

