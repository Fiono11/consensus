use std::collections::{BTreeSet, HashMap};
use std::net::SocketAddr;
use std::sync::{Arc, Condvar, Mutex};
use std::sync::mpsc::Sender;
use std::thread;
use std::thread::sleep;
use std::time::Duration;
use log::{debug, info};
use rand::{Rng, thread_rng};
use crypto::{Digest, PublicKey};
use crate::constants::{NUMBER_OF_BYZANTINE_NODES, QUORUM, ROUND_TIMER, VOTE_DELAY};
use crate::election::Election;
use crate::message::Message;
use crate::NUMBER_OF_NODES;
use crate::round::{Round, RoundState, Timer};
use crate::tally::tally_votes;
use crate::vote::Category::{Decided, Final, Initial};
use crate::vote::Value::{One, Zero};
use crate::vote::{ParentHash, Transaction, TxHash, Vote, VoteState};
use crate::vote::VoteState::{Invalid, Pending, Valid};
use bytes::Bytes;
use async_recursion::async_recursion;

#[derive(Debug)]
pub(crate) struct Node {
    id: PublicKey,
    sender: Sender<Message>,
    elections: HashMap<ParentHash, Election>,
    pub(crate) byzantine: bool,
    decided_txs: HashMap<PublicKey, TxHash>,
    peers: Vec<PublicKey>,
}

impl Node {
    pub(crate) fn new(id: PublicKey, sender: Sender<Message>, byzantine: bool, peers: Vec<PublicKey>) -> Self {
        Node {
            id,
            sender,
            elections: HashMap::new(),
            byzantine,
            decided_txs: HashMap::new(),
            peers,
        }
    }

    pub(crate) fn start_new_round(&mut self, round: Round, tx: &Transaction) {
        let election = self.elections.get_mut(&tx.parent_hash).unwrap();
        println!("Node {}: started round {}!", self.id, round);
        let round_state = RoundState::new();
        let timer = Arc::clone(&round_state.timer);
        election.state.insert(round, round_state);
        self.start_timer(timer, round);
    }

    fn start_timer(&self, timer: Arc<(Mutex<Timer>, Condvar)>, round: Round) {
        let id = self.id;
        thread::spawn(move || {
            //sleep(Duration::from_millis(ROUND_TIMER as u64));
            println!("Node {}: round {} expired!", id, round);
            let &(ref mutex, ref cvar) = &*timer;
            let mut value = mutex.lock().unwrap();
            *value = Timer::Expired;
            cvar.notify_one();
        });
    }

    pub(crate) fn handle_message(&mut self, msg: &Message) {
        let e = self.elections.get(&msg.vote.value.parent_hash).cloned();
        match e {
            Some(election) => {
                self.handle_vote(msg.sender, msg.vote.clone())
            }
            None => {
                self.handle_vote(msg.sender, msg.vote.clone())
            }
        }
    }

    //#[async_recursion]
    fn handle_vote(&mut self, from: PublicKey, vote: Vote) {
        println!("Node {}: received {:?} from node {}", self.id, vote, from);
        let tx = &vote.value;
        let round = vote.round;
        let election = Election::new();
        self.elections.insert(vote.value.parent_hash.clone(), election);
        if self.validate_vote(&vote, tx) == Valid {
            self.send_vote(vote.clone());
            self.insert_vote(vote.clone(), tx);
            self.try_validate_pending_votes(&round, tx);
            let election = self.elections.get_mut(&tx.parent_hash).unwrap();
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
                    println!("Node {}: waiting for the round {} to expire...", self.id, round);
                    value = cvar.wait(value).unwrap();
                }*/
                if election.decided_vote.is_some() {
                    let mut vote = election.decided_vote.as_ref().unwrap().clone();
                    vote.round = round + 1;
                    self.send_vote(vote);
                } else {
                    self.vote_next_round(round + 1, votes,&vote.value);
                }
            }
        }
    }

    fn insert_vote(&mut self, vote: Vote, tx: &Transaction) {
        let round = vote.round;
        let election= self.elections.get_mut(&tx.parent_hash).unwrap();
        election.concurrent_txs.insert(tx.tx_hash.clone());
        let rs = election.state.get_mut(&round);
        match rs {
            Some(round_state) => {
                println!("Node {}: inserted {:?}", self.id, &vote);
                round_state.votes.insert(vote.clone());
            }
            None => {
                println!("Node {}: inserted {:?}", self.id, &vote);
                election.state.get_mut(&round).unwrap().votes.insert(vote.clone());
                self.start_new_round(round, tx);
            }
        }
        //println!("Node {}: votes of round {} -> {:?}", self.id, &vote.round, &election.state.get(&vote.round));
    }

    //#[async_recursion]
    fn vote_next_round(&mut self, round: Round, votes: &BTreeSet<Vote>, tx: &Transaction) {
        //if election.state.get_mut(&round).is_none() {
        self.start_new_round(round, tx);
        //}
        let election = self.elections.get_mut(&tx.parent_hash).unwrap();
        let concurrent_txs: Vec<TxHash> = election.concurrent_txs.clone().into_iter().collect();
        let rand = thread_rng().gen_range(0..concurrent_txs.len() + 1);
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
        }
    }

    fn validate_vote(&mut self, vote: &Vote, tx: &Transaction) -> VoteState {
        let concurrent_txs = self.elections.get(&tx.parent_hash).unwrap().concurrent_txs.clone();
        let election = self.elections.get_mut(&tx.parent_hash).unwrap();
        let mut _state = Invalid;
        let tx = vote.value.clone();
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
                    println!("Node {}: previous round votes -> {:?}", self.id, rs.votes);
                }
                None => {
                    println!("Node {}: round {} have not started yet!", self.id, proof_round);
                }
            }
        }
        println!("Node {}: {:?} is {:?}", self.id, vote, _state);
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
        let concurrent_txs = self.elections.get(&tx.parent_hash).unwrap().concurrent_txs.clone();
        let election = self.elections.get_mut(&tx.parent_hash).unwrap();
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
                if tally.initial_count == highest_tally.initial_count {
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

    //#[async_recursion]
    pub(crate) fn send_vote(&self, vote: Vote) {
        let round = vote.round;
        if !self.byzantine {
            for i in 0..NUMBER_OF_NODES {
                let msg = Message::new(self.id,self.peers[i], vote.clone());
                let rand = rand::thread_rng().gen_range(0..VOTE_DELAY as u64);
                //sleep(Duration::from_millis(rand));
                self.sender.send(msg).unwrap();
                println!("Node {}: sent {:?} to {}", self.id, &vote, self.peers[i]);
            }
        }
        else {
            if round != 0 {
                for i in 0..NUMBER_OF_NODES {
                    let txs: Vec<TxHash> = self.elections.get(&vote.value.parent_hash).unwrap().clone().concurrent_txs.into_iter().collect();
                    let i = thread_rng().gen_range(0..txs.len());
                    let mut category = Initial;
                    let rand = thread_rng().gen_range(0..3);
                    if rand == 0 {
                        category = Final;
                    }
                    if rand == 1 {
                        category = Decided;
                    }
                    let rand2 = thread_rng().gen_range(0..round-1);
                    let vote = Vote::new(self.id, round, Transaction::new(vote.value.parent_hash.clone(), txs[i].clone()), Initial, Some(rand2));
                    //if vote.is_some() {
                        println!("Node {}: sent {:?} to {}", self.id, &vote, self.peers[i]);
                        let msg = Message::new(self.id, self.peers[i],vote.clone());
                        self.sender.send(msg).unwrap();
                    //}
                }
            }
        }
    }
}

