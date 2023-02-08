use std::collections::{BTreeSet, HashMap};
use std::net::SocketAddr;
use std::sync::{Arc, Condvar, Mutex};
use tokio::sync::mpsc::Sender;
use std::thread;
use std::thread::sleep;
use std::time::Duration;
use bytes::Bytes;
use futures::task::SpawnExt;
use log::{debug, info, warn};
use rand::{Rng, thread_rng};
use tokio::sync::mpsc::{channel, Receiver};
use crate::constants::{NUMBER_OF_BYZANTINE_NODES, NUMBER_OF_CORRECT_NODES, NUMBER_OF_TXS, QUORUM, ROUND_TIMER, VOTE_DELAY};
use crate::election::{Election, ElectionId};
use crate::message::Message;
use crate::{Block, NUMBER_OF_NODES};
use crate::round::{Round, RoundState, Timer};
use crate::tally::tally_votes;
use crate::vote::Category::{Decided, Final, Initial};
use crate::vote::Value::{One, Zero};
use crate::vote::{ParentHash, Transaction, TxHash, Value, Vote, VoteState};
use crate::vote::VoteState::{Invalid, Pending, Valid};
use network::{CancelHandler, MessageHandler, Receiver as NetworkReceiver, ReliableSender, Writer};
use async_recursion::async_recursion;
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use thiserror::Error;

//#[cfg(test)]
//#[path = "tests/core_tests.rs"]
//pub mod core_tests;

use crypto::{CryptoError, Digest, PublicKey, SignatureService};
use network::SimpleSender;
use store::{Store, StoreError};
use crate::config::{Committee, Stake};
use crate::consensus::{CHANNEL_CAPACITY, ConsensusMessage};
use crate::error::ConsensusError;

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

    /*pub(crate) fn start_new_round(&mut self, round: Round, tx: &Transaction) {
        //let election = self.elections.lock().unwrap().get_mut(&tx.parent_hash).unwrap();
        debug!("Node {}: started round {}!", self.id, round);
        let round_state = RoundState::new(round);
        let timer = Arc::clone(&round_state.timer);
        self.elections.get_mut(&tx.parent_hash).unwrap().state.insert(round, round_state);
        //self.start_timer(timer, round);
    }

    fn start_timer(&self, timer: Arc<(Mutex<Timer>, Condvar)>, round: Round) {
        let id = self.id;
        thread::spawn(move || {
            sleep(Duration::from_millis(ROUND_TIMER as u64));
            debug!("Node {}: round {} expired!", id, round);
            let &(ref mutex, ref cvar) = &*timer;
            let mut value = mutex.lock().unwrap();
            *value = Timer::Expired;
            cvar.notify_one();
        });
    }

    #[async_recursion]
    async fn handle_vote(&mut self, vote: Vote) {
        debug!("Node {}: received {:?} from node {}", self.id, vote, vote.signer);
        let tx = &vote.value;
        let round = vote.round;
        if !self.elections.contains_key(&vote.value.parent_hash) {
            let election = Election::new();
            self.elections.insert(vote.value.parent_hash.clone(), election);
        }
        if self.validate_vote(&vote, tx) == Valid {
            self.send_vote(vote.clone()).await;
            self.insert_vote(vote.clone(), tx).await;
            self.try_validate_pending_votes(&round, tx).await;
            //let election = self.elections.lock().unwrap().get_mut(&tx.parent_hash).unwrap();
            let round_state = self.elections.get_mut(&tx.parent_hash).unwrap().state.get(&round).unwrap().clone();
            let votes = &round_state.votes;
            let mut voted_next_round = false;
            if let Some(round_state) = self.elections.get_mut(&tx.parent_hash).unwrap().state.get(&(round + 1)) {
                voted_next_round = round_state.voted;
            }
            let mut election_finished = true;
            let decisions = self.decided_txs.clone();
            if decisions.len() != NUMBER_OF_CORRECT_NODES {
                election_finished = false;
            }
            for (pk, txs) in decisions {
                if txs.len() != NUMBER_OF_TXS {
                    election_finished = false;
                }
            }
            if votes.len() >= QUORUM && !voted_next_round && !election_finished {
                {
                    let &(ref mutex, ref cvar) = &*round_state.timer;
                    let value = mutex.lock().unwrap();
                    let mut value = value;
                    debug!("timer: {:?}", *value);
                    while *value == Timer::Active {
                        debug!("Node {}: waiting for the round {} to expire...", self.id, round);
                        value = cvar.wait(value).unwrap();
                    }
                }
                if self.elections.get_mut(&tx.parent_hash).unwrap().decided_vote.is_some() {
                    let mut vote = self.elections.get_mut(&tx.parent_hash).unwrap().decided_vote.as_ref().unwrap().clone();
                    vote.round = round + 1;
                    self.send_vote(vote).await;
                } else {
                    self.vote_next_round(round + 1, votes,&vote.value).await;
                }
            }
        }
        debug!("Txs of node4 {:?}: {:?}", self.id, self.elections.get_mut(&tx.parent_hash).unwrap().concurrent_txs);
    }

    async fn insert_vote(&mut self, vote: Vote, tx: &Transaction) {
        let round = vote.round;
        debug!("Elections of {:?}: {:?}", self.id, &self.elections);
        //let mut binding = self.elections.lock().unwrap();
        let election= self.elections.get_mut(&tx.parent_hash).unwrap();
        debug!("Txs of node1 {:?}: {:?}", self.id, election.concurrent_txs);
        election.concurrent_txs.insert(tx.tx_hash.clone());
        debug!("Txs of node2 {:?}: {:?}", self.id, election.concurrent_txs);
        let rs = election.state.get_mut(&round);
        match rs {
            Some(round_state) => {
                debug!("Node {}: inserted {:?}", self.id, &vote);
                round_state.votes.insert(vote.clone());
            }
            None => {
                debug!("Node {}: inserted {:?}", self.id, &vote);
                //self.start_new_round(round, tx);
                debug!("Node {}: started round {}!", self.id, round);
                let round_state = RoundState::new(round);
                let timer = Arc::clone(&round_state.timer);
                election.state.insert(round, round_state);
                self.elections.get_mut(&tx.parent_hash).unwrap().state.get_mut(&round).unwrap().votes.insert(vote.clone());
            }
        }
        //debug!("Txs of node3 {:?}: {:?}", self.id, &election.concurrent_txs);
        //debug!("Node {}: votes of round {} -> {:?}", self.id, &vote.round, &election.state.get(&vote.round));
    }

    //#[async_recursion]
    async fn vote_next_round(&mut self, round: Round, votes: &BTreeSet<Vote>, tx: &Transaction) {
        //if election.state.get_mut(&round).is_none() {
        self.start_new_round(round, tx);
        //}
        //let mut binding = self.elections.lock().unwrap();
        //let election = self.elections.lock().unwrap().get_mut(&tx.parent_hash).unwrap();
        /*let concurrent_txs: Vec<TxHash> = election.concurrent_txs.clone().into_iter().collect();
        let rand = thread_rng().gen_range(0..(concurrent_txs.len() + 1));
        let rand2 = thread_rng().gen_range(0..3);
        let mut round_state = election.state.get_mut(&round).unwrap().clone();
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
        if vote.is_some() {
            let vote = vote.unwrap();
            self.send_vote(vote.clone());
            round_state.votes.insert(vote.clone());
            round_state.voted = true;
            self.elections.get_mut(&tx.parent_hash).unwrap().state.insert(round, round_state);
        }*/
        let mut round_state = self.elections.get_mut(&tx.parent_hash).unwrap().state.get_mut(&round).unwrap().clone();
        let vote = self.decide_vote(&votes, round, tx);
        round_state.votes.insert(vote.clone());
        round_state.voted = true;
        self.send_vote(vote.clone()).await;
        self.elections.get_mut(&tx.parent_hash).unwrap().state.insert(round, round_state);
    }*/

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
        if self.decided_txs.get(&parent_hash).unwrap().len() == NUMBER_OF_NODES {
            debug!("Election {:?} is finished!", &parent_hash);
            self.elections.get_mut(&parent_hash).unwrap().active = false;
        }
    }

    /*fn validate_vote(&mut self, vote: &Vote, tx: &Transaction) -> VoteState {
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
                            self.insert_decided(vote.signer, tx.tx_hash);
                            /*if !self.decided_txs.contains_key(&vote.signer) {
                                self.decided_txs.insert(vote.signer, tx.tx_hash);
                                info!("Inserted {:?}", &vote);
                            }*/
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
                //let election = self.elections.lock().unwrap().get_mut(&tx.parent_hash).unwrap();
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
        //let election = self.elections.lock().unwrap().get_mut(&tx.parent_hash).unwrap();
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
        //let binding = self.elections.lock().unwrap();
        //let election = binding.get(&tx.parent_hash).unwrap();
        let pending_votes = self.elections.get(&tx.parent_hash).unwrap().pending_votes.clone();
        for vote in &pending_votes {
            if &vote.proof_round.unwrap() == round {
                let state = self.validate_vote(vote, tx);
                if state == Valid {
                    self.insert_vote(vote.clone(), tx).await;
                }
            }
        }
    }*/

    /*async fn decide_vote(&mut self, votes: &BTreeSet<Vote>, round: Round, tx: &Transaction) -> Vote {
        let concurrent_txs = self.elections.get(&tx.parent_hash).unwrap().concurrent_txs.clone();
        let tallies = tally_votes(&concurrent_txs, votes);
        let mut vote = Vote::random(self.id, tx.parent_hash.clone());
        for (tx_hash, tally) in &tallies {
            if tally.decided_count > 0 {
                let proof_round = votes.iter().filter(|v| v.category == Decided).next().unwrap().proof_round.unwrap();
                let tx = Transaction::new(tx.parent_hash.clone(), tx_hash.clone());
                vote = Vote::new(self.id, round, tx.clone(), Decided, Some(proof_round));
                self.elections.get_mut(&tx.parent_hash).unwrap().decided_vote = Some(vote.clone());
                self.insert_decided(self.id, tx_hash.clone(), tx.parent_hash);
                info!("Decided {:?}", &tx_hash);
            }
            else if tally.final_count >= QUORUM {
                let tx = Transaction::new(tx.parent_hash.clone(), tx_hash.clone());
                vote = Vote::new(self.id, round, tx.clone(), Decided, Some(vote.round-1));
                self.elections.get_mut(&tx.parent_hash).unwrap().decided_vote = Some(vote.clone());
                self.insert_decided(self.id, tx_hash.clone(), tx.parent_hash);
                info!("Decided {:?}", &tx_hash);
            } else if tally.final_count > 0 {
                let final_votes: Vec<&Vote> = votes.iter().filter(|v| v.category == Final).collect();
                let mut final_vote = &final_votes[0].clone();
                for vote in &final_votes {
                    if vote.proof_round.unwrap() > final_vote.proof_round.unwrap() {
                        final_vote = vote.clone();
                    }
                }
                if self.elections.get_mut(&tx.parent_hash).unwrap().state.get(&(round-1)).unwrap().final_vote.is_some() {
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
            }
        }
        vote
    }*/

    async fn decide_vote(&mut self, votes: &BTreeSet<Vote>, round: Round, tx: &Transaction) -> Vote {
        let concurrent_txs = self.elections.get(&tx.parent_hash).unwrap().concurrent_txs.clone();
        //let election = self.elections.lock().unwrap().get_mut(&tx.parent_hash).unwrap();
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
                //if !self.decided_txs.contains_key(&vote.signer) {
                    //self.insert_decided(self.id, digest.clone(), tx.parent_hash);
                    info!("Decided {:?}", &digest);
                //}
                return vote
            }
            else if tally.final_count >= QUORUM {
                let tx = Transaction::new(tx.parent_hash.clone(), digest.clone());
                let vote = Vote::new(self.id, round, tx.clone(), Decided, Some(previous_round));
                self.elections.get_mut(&tx.parent_hash).unwrap().decided_vote = Some(vote.clone());
                //if !self.decided_txs.contains_key(&vote.signer) {
                //self.insert_decided(self.id, digest.clone(), tx.parent_hash);
                    info!("Decided {:?}", &digest);
                //}
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
                if tally.initial_count == highest_tally.initial_count {
                    //debug!("!!!!!!!!!!!!!!!!!!");
                    if digest.clone() > highest {
                        highest_tally = tally.clone();
                        highest = digest.clone();
                    }
                }
            }
        }
        Vote::new(self.id, round, Transaction::new(tx.parent_hash.clone(), highest.clone()), Initial, None)
        // if tie, vote for the highest hash
    }

    /*#[async_recursion]
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
        if !self.byzantine {
            for i in 0..NUMBER_OF_NODES {
                let msg = Message::new(self.id, vote.clone());
                let rand = rand::thread_rng().gen_range(0..VOTE_DELAY as u64);
                //sleep(Duration::from_millis(rand));
                let message = bincode::serialize(&ConsensusMessage::Message(msg))
                    .expect("Failed to serialize vote");
                let handlers = self.network.send(addresses[i], Bytes::from(message)).await;
                self.cancel_handlers
                    .entry(vote.round)
                    .or_insert_with(Vec::new)
                    .insert(0, handlers);
                debug!("Node {}: sent {:?} to {}", self.id, &vote, public_keys[i]);
            }
        }
        else {
            if round != 0 {
                for i in 0..NUMBER_OF_NODES {
                    let txs: Vec<TxHash> = self.elections.get(&vote.value.parent_hash).unwrap().clone().concurrent_txs.into_iter().collect();
                    let rand = thread_rng().gen_range(0..txs.len());
                    let mut category = Initial;
                    let rand = thread_rng().gen_range(0..3);
                    if rand == 0 {
                        category = Final;
                    }
                    if rand == 1 {
                        category = Decided;
                    }
                    let rand2 = thread_rng().gen_range(0..round);
                    let vote = Vote::new(self.id, round, Transaction::new(vote.value.parent_hash.clone(), txs[rand].clone()), Initial, Some(rand2));
                    //if vote.is_some() {
                    debug!("Node {}: sent {:?} to {}", self.id, &vote, public_keys[i]);
                    let msg = Message::new(self.id, vote.clone());
                    let message = bincode::serialize(&ConsensusMessage::Message(msg))
                        .expect("Failed to serialize vote");
                    let handlers = self.network.send(addresses[i], Bytes::from(message)).await;
                    self.cancel_handlers
                        .entry(vote.round)
                        .or_insert_with(Vec::new)
                        .insert(0, handlers);
                    //}
                }
            }
        }
    }

    /*async fn insert_vote(&mut self, vote: Vote) {
        match self.elections.lock().unwrap().get_mut(&vote.value.parent_hash) {
            Some(election) => {
                election.concurrent_txs.insert(vote.value.tx_hash.clone());
                debug!("Txs of node1 {:?}: {:?}", self.id, &election.concurrent_txs);
            }
            None => {
                let mut election = Election::new();
                election.concurrent_txs.insert(vote.value.tx_hash.clone());
                debug!("Txs of node2 {:?}: {:?}", self.id, &election.concurrent_txs);
                self.elections.lock().unwrap().insert(vote.value.parent_hash, election);
            }
        }
    }*/*/

    async fn start_election(&mut self, vote: Vote) {
        match self.elections.get(&vote.value.parent_hash) {
            Some(election) => (),
            None => {
                let election = Election::new();
                debug!("Started election {:?}", &vote.value.parent_hash);
                self.elections.insert(vote.value.parent_hash, election);
            }
        }
    }

    async fn start_round(&mut self, vote: Vote) {
        if let None = self.elections.get(&vote.value.parent_hash).unwrap().state.get(&vote.round) {
            let round_state = RoundState::new(vote.round);
            debug!("Started round {:?} of election {:?}", &vote.round, &vote.value.parent_hash);
            self.elections.get_mut(&vote.value.parent_hash).unwrap().state.insert(vote.round,round_state);
        }
    }

    async fn insert_vote(&mut self, vote: Vote) {
        debug!("Inserted {:?}", vote);
        if vote.category == Decided {
            self.insert_decided(vote.signer.clone(), vote.value.tx_hash.clone(), vote.value.parent_hash.clone());
        }
        self.elections.get_mut(&vote.value.parent_hash).unwrap().state.get_mut(&vote.round).unwrap().votes.insert(vote);
    }

    async fn handle_vote(&mut self, vote: Vote) {
        debug!("Received {:?}", &vote);
        let vote_state = self.validate_vote(vote.clone()).await;
        self.start_election(vote.clone()).await;
        self.start_round(vote.clone()).await;
        if vote_state == Valid {
            self.elections.get_mut(&vote.value.parent_hash).unwrap().concurrent_txs.insert(vote.value.tx_hash.clone());
            self.insert_vote(vote.clone()).await;
            //self.send_vote(vote.clone()).await;
            // validate pending
            // send valid
        }
        let next_round = vote.round + 1;
        let mut voted_next_round = false;
        if let Some(rs) = self.elections.get_mut(&vote.value.parent_hash).unwrap().state.get(&(vote.round+1)) {
            voted_next_round = rs.voted;
        }
        let votes = self.elections.get(&vote.value.parent_hash).unwrap().state.get(&vote.round).unwrap().votes.clone();
        let election_active = self.elections.get(&vote.value.parent_hash).unwrap().active;
        if votes.len() > QUORUM && !voted_next_round && election_active {
            let vote = self.decide_vote(&votes, next_round, &vote.value).await;
            /*match self.elections.get_mut(&vote.value.parent_hash).unwrap().state.get_mut(&next_round) {
                Some(rs) => {
                    rs.voted = true;
                }
                None => {
                    let mut round_state = RoundState::new(next_round);
                    round_state.voted = true;
                    debug!("Started round {:?} of election {:?}", &next_round, &vote.value.parent_hash);
                    self.elections.get_mut(&vote.value.parent_hash).unwrap().state.insert(next_round,round_state);
                }
            }*/
            self.start_round(vote.clone()).await;
            self.elections.get_mut(&vote.value.parent_hash).unwrap().state.get_mut(&next_round).unwrap().voted = true;
            self.send_vote(vote.clone()).await;
        }
    }

    async fn validate_vote(&mut self, vote: Vote) -> VoteState {
        Valid
    }

    async fn send_vote(&mut self, vote: Vote) {
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
            //let result =
            tokio::select! {
                Some(vote) = self.rx_vote.recv() => {
                    //ConsensusMessage::Message(msg) => {
                        //info!("Received!");
                        //self.send_vote(vote).await;
                        self.handle_vote(vote.clone()).await;
                    //}
                    //_ => panic!("Unexpected protocol message")
                },
                Some(transaction) = self.rx_transaction.recv() => {
                    //info!("Received {:?}", transaction.tx_hash);
                    let vote = Vote::new(self.id, 0, transaction.clone(), Initial, None);
                    //self.handle_vote(vote.clone()).await;
                    self.send_vote(vote).await;
                },
            };
            //match result {
                //Ok(()) => (),
                //Err(e) => warn!("{}", e),
            //}

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

    #[error("Invalid header id")]
    InvalidHeaderId,

    #[error("Malformed header {0}")]
    MalformedHeader(Digest),

    #[error("Received message from unknown authority {0}")]
    UnknownAuthority(PublicKey),

    #[error("Authority {0} appears in quorum more than once")]
    AuthorityReuse(PublicKey),

    #[error("Received unexpected vote fo header {0}")]
    UnexpectedVote(Digest),

    #[error("Received certificate without a quorum")]
    CertificateRequiresQuorum,

    #[error("Parents of header {0} are not a quorum")]
    HeaderRequiresQuorum(Digest),

    #[error("Message {0} (round {1}) too old")]
    TooOld(Digest, Round),
}

