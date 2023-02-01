use std::collections::BTreeSet;
use std::net::SocketAddr;
use std::sync::{Arc, Condvar, Mutex};
use std::sync::mpsc::Sender;
use std::thread;
use std::thread::sleep;
use std::time::Duration;
use bytes::Bytes;
use log::{error, warn};
use rand::Rng;
use tokio::sync::mpsc::Receiver;
use crate::constants::{NUMBER_OF_BYZANTINE_NODES, QUORUM, ROUND_TIMER, VOTE_DELAY};
use crate::election::Election;
use crate::message::Message;
use crate::{Block, NUMBER_OF_NODES};
use crate::round::{Round, RoundState, Timer};
use crate::tally::tally_votes;
use crate::vote::Category::{Decided, Final, Initial};
use crate::vote::Value::{One, Zero};
use crate::vote::{Vote, VoteState};
use crate::vote::VoteState::{Invalid, Pending, Valid};

//#[cfg(test)]
//#[path = "tests/core_tests.rs"]
//pub mod core_tests;

use crypto::{PublicKey, SignatureService};
use network::SimpleSender;
use store::Store;
use crate::config::Committee;
use crate::consensus::ConsensusMessage;
use crate::error::ConsensusError;
use crate::mempool::MempoolDriver;
use crate::node::Id;

pub struct Core {
    name: PublicKey,
    committee: Committee,
    store: Store,
    signature_service: SignatureService,
    //leader_elector: LeaderElector,
    mempool_driver: MempoolDriver,
    //synchronizer: Synchronizer,
    rx_message: Receiver<ConsensusMessage>,
    //rx_loopback: Receiver<Block>,
    //tx_proposer: Sender<ProposerMessage>,
    //tx_commit: Sender<Block>,
    round: Round,
    last_voted_round: Round,
    last_committed_round: Round,
    //high_qc: QC,
    //timer: Timer,
    //aggregator: Aggregator,
    network: SimpleSender,
    id: Id,
    sender: Sender<Message>,
    election: Election,
    pub(crate) byzantine: bool,
}

impl Core {
    #[allow(clippy::too_many_arguments)]
    pub fn spawn(
        name: PublicKey,
        committee: Committee,
        signature_service: SignatureService,
        store: Store,
        //leader_elector: LeaderElector,
        mempool_driver: MempoolDriver,
        //synchronizer: Synchronizer,
        timeout_delay: u64,
        rx_message: Receiver<ConsensusMessage>,
        rx_loopback: Receiver<Block>,
        //tx_proposer: Sender<ProposerMessage>,
        tx_commit: Sender<Block>,
        id: Id,
        sender: Sender<Message>,
        byzantine: bool
    ) {
        tokio::spawn(async move {
            Self {
                name,
                committee: committee.clone(),
                signature_service,
                store,
                //leader_elector,
                mempool_driver,
                //synchronizer,
                rx_message,
                //rx_loopback,
                //tx_proposer,
                //tx_commit,
                round: 1,
                last_voted_round: 0,
                last_committed_round: 0,
                //high_qc: QC::genesis(),
                //timer: Timer::new(timeout_delay),
                //aggregator: Aggregator::new(committee),
                network: SimpleSender::new(),
                id,
                sender,
                election: Election::new(),
                byzantine,
            }
            .run()
            .await
        });
    }

    pub(crate) fn start_new_round(&mut self, round: Round) {
        println!("Node {}: started round {}!", self.id, round);
        let round_state = RoundState::new();
        let timer = Arc::clone(&round_state.timer);
        self.election.state.insert(round, round_state);
        self.start_timer(timer, round);
    }

    fn start_timer(&self, timer: Arc<(Mutex<Timer>, Condvar)>, round: Round) {
        let id = self.id;
        thread::spawn(move || {
            sleep(Duration::from_millis(ROUND_TIMER as u64));
            println!("Node {}: round {} expired!", id, round);
            let &(ref mutex, ref cvar) = &*timer;
            let mut value = mutex.lock().unwrap();
            *value = Timer::Expired;
            cvar.notify_one();
        });
    }

    pub(crate) fn handle_message(&mut self, msg: &Message) {
        self.handle_vote(msg.sender, msg.vote.clone());
    }

    //#[async_recursion]
    async fn handle_vote(&mut self, from: Id, vote: Vote) {
        println!("Node {}: received {:?} from node {}", self.id, vote, from);
        let round = vote.round;
        if self.validate_vote(&vote) == Valid {
            //self.send_vote(vote.clone());
            self.insert_vote(vote);
            self.try_validate_pending_votes(&round);
            let round_state = self.election.state.get(&round).unwrap().clone();
            let votes = &round_state.votes;
            let mut voted_next_round = false;
            if let Some(round_state) = self.election.state.get(&(round + 1)) {
                voted_next_round = round_state.voted;
            }
            if votes.len() >= QUORUM && !voted_next_round {
                let &(ref mutex, ref cvar) = &*round_state.timer;
                let value = mutex.lock().unwrap();
                let mut value = value;
                while *value == Timer::Active {
                    println!("Node {}: waiting for the round {} to expire...", self.id, round);
                    value = cvar.wait(value).unwrap();
                }
                if self.election.decided_vote.is_some() {
                    let mut vote = self.election.decided_vote.as_ref().unwrap().clone();
                    vote.round = round + 1;
                    self.send_vote(vote);
                } else {
                    self.vote_next_round(round + 1, votes);
                }
            }
        }
    }

    fn insert_vote(&mut self, vote: Vote) {
        let round = vote.round;
        match self.election.state.get_mut(&round) {
            Some(round_state) => {
                println!("Node {}: inserted {:?}", self.id, &vote);
                round_state.votes.insert(vote.clone());
            }
            None => {
                self.start_new_round(round);
                println!("Node {}: inserted {:?}", self.id, &vote);
                self.election.state.get_mut(&round).unwrap().votes.insert(vote.clone());
            }
        }
        println!("Node {}: votes of round {} -> {:?}", self.id, &vote.round, &self.election.state.get(&vote.round));
    }

    fn vote_next_round(&mut self, round: Round, votes: &BTreeSet<Vote>) {
        let mut vote = Vote::random(self.id, round);
        if !self.byzantine {
            vote = Some(self.decide_vote(&votes, round));
        }
        if self.election.state.get_mut(&round).is_none() {
            self.start_new_round(round);
        }
        let mut round_state = self.election.state.get_mut(&round).unwrap();
        round_state.voted = true;
        if vote.is_some() {
            let vote = vote.unwrap();
            round_state.votes.insert(vote.clone());
            self.send_vote(vote);
        }
    }

    fn validate_vote(&mut self, vote: &Vote) -> VoteState {
        let mut _state = Invalid;
        match vote.category {
            Decided => {
                let proof_round = vote.proof_round.unwrap();
                let proof_round_state = self.election.state.get(&proof_round);
                match proof_round_state {
                    Some(round_state) => {
                        let proof_round_votes = round_state.votes.clone();
                        let tally = tally_votes(&proof_round_votes);
                        if tally.final_zeros >= QUORUM && vote.value == Zero {
                            _state = Valid;
                        }
                        else if tally.final_ones >= QUORUM && vote.value == One {
                            _state = Valid;
                        }
                        else if tally.initial_ones + tally.final_ones + tally.initial_zeros > NUMBER_OF_BYZANTINE_NODES && vote.value == Zero {
                            _state = Invalid;
                        }
                        else if tally.initial_zeros + tally.final_zeros + tally.initial_ones > NUMBER_OF_BYZANTINE_NODES && vote.value == One {
                            _state = Invalid
                        } else {
                            self.election.pending_votes.insert(vote.clone());
                            _state = Pending
                        }
                    },
                    None => _state = Pending,
                }
            },
            Final => {
                let proof_round = vote.proof_round.unwrap();
                let proof_round_state = self.election.state.get(&proof_round);
                match proof_round_state {
                    Some(round_state) => {
                        let proof_round_votes = round_state.votes.clone();
                        let tally = tally_votes(&proof_round_votes);
                        if tally.initial_zeros >= QUORUM && vote.value == Zero {
                            _state = Valid;
                        }
                        else if tally.initial_ones >= QUORUM && vote.value == One {
                            _state = Valid;
                            /*}
                            else if tally.initial_ones > NUMBER_OF_BYZANTINE_NODES && vote.value == Zero {
                                _state = Invalid;
                            }
                            else if tally.initial_zeros > NUMBER_OF_BYZANTINE_NODES && vote.value == One {
                                _state = Invalid*/
                        } else {
                            self.election.pending_votes.insert(vote.clone());
                            _state = Pending
                        }
                    },
                    None => _state = Pending,
                }
            },
            Initial => _state = Valid,
        }
        if (_state == Valid || _state == Invalid) && self.election.pending_votes.contains(vote) {
            self.election.pending_votes.remove(vote);
        }
        if _state == Pending || _state == Invalid {
            let proof_round = &vote.proof_round.unwrap();
            let rs = self.election.state.get(proof_round);
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

    fn try_validate_pending_votes(&mut self, round: &Round) {
        let pending_votes = self.election.pending_votes.clone();
        for vote in &pending_votes {
            if &vote.proof_round.unwrap() == round {
                let state = self.validate_vote(vote);
                if state == Valid {
                    self.election.state.get_mut(&vote.round).unwrap().votes.insert(vote.clone());
                    self.try_validate_pending_votes(&vote.round);
                }
            }
        }
    }

    fn decide_vote(&mut self, votes: &BTreeSet<Vote>, round: Round) -> Vote {
        assert!(votes.len() >= QUORUM);
        let previous_round = round - 1;
        let previous_round_state = self.election.state.get(&previous_round).unwrap();
        let tally = tally_votes(votes);
        return if tally.decided_zeros > 0 {
            let proof_round = votes.iter().filter(|v| v.category == Decided).next().unwrap().proof_round.unwrap();
            let vote = Vote::new(self.id, round, Zero, Decided, Some(proof_round));
            self.election.decided_vote = Some(vote.clone());
            vote
        } else if tally.decided_ones > 0 {
            let proof_round = votes.iter().filter(|v| v.category == Decided).next().unwrap().proof_round.unwrap();
            let vote = Vote::new(self.id, round, One, Decided, Some(proof_round));
            self.election.decided_vote = Some(vote.clone());
            vote
        } else if tally.final_zeros >= QUORUM {
            let vote = Vote::new(self.id, round, Zero, Decided, Some(previous_round));
            self.election.decided_vote = Some(vote.clone());
            vote
        } else if tally.final_ones >= QUORUM {
            let vote= Vote::new(self.id, round, One, Decided, Some(previous_round));
            self.election.decided_vote = Some(vote.clone());
            vote
        } else if tally.final_ones > 0 || tally.final_zeros > 0 {
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
                    Vote::new(self.id, round, final_vote.value.clone(), Final, final_vote.proof_round)
                }
                else {
                    Vote::new(self.id, round, old_vote.value, Final, old_vote.proof_round)
                }
            }
            else {
                Vote::new(self.id, round, final_vote.value.clone(), Final, final_vote.proof_round)
            }
        } else if tally.initial_zeros >= QUORUM {
            Vote::new(self.id, round, Zero, Final, Some(previous_round))
        } else if tally.initial_ones >= QUORUM {
            Vote::new(self.id, round, One, Final,Some(previous_round))
        }
        else {
            if tally.initial_zeros >= tally.initial_ones {
                Vote::new(self.id, round, Zero, Initial, None)
            } else {
                Vote::new(self.id, round, One, Initial, None)
            }
        }
    }

    //#[async_recursion]
    pub(crate) async fn send_vote(&mut self, vote: Vote) {
        let round = vote.round;
        let broadcast_addresses = self
            .committee
            .broadcast_addresses(&self.name);
        let mut addresses: Vec<SocketAddr> = Vec::new();
        for (pk, sa) in broadcast_addresses {
            addresses.push(sa);
        }
        if !self.byzantine {
            for i in 0..NUMBER_OF_NODES {
                let msg = Message::new(self.id, i, vote.clone());
                let rand = rand::thread_rng().gen_range(0..VOTE_DELAY as u64);
                sleep(Duration::from_millis(rand));
                let message = bincode::serialize(&ConsensusMessage::Message(msg))
                    .expect("Failed to serialize vote");
                self.network.broadcast(addresses.clone(), Bytes::from(message)).await;
                println!("Node {}: sent {:?} to {}", self.id, &vote, i);
            }
        }
        else {
            if round != 0 {
                for i in 0..NUMBER_OF_NODES {
                    let vote = Vote::random(self.id, round);
                    if vote.is_some() {
                        println!("Node {}: sent {:?} to {}", self.id, &vote, i);
                        let msg = Message::new(self.id, i,vote.unwrap().clone());
                        // send to different nodes
                        let message = bincode::serialize(&ConsensusMessage::Message(msg))
                            .expect("Failed to serialize vote");
                        self.network.broadcast(addresses.clone(), Bytes::from(message)).await;
                    }
                }
            }
        }
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
                    ConsensusMessage::Message(msg) => self.handle_vote(msg.sender, msg.vote).await,
                    //ConsensusMessage::Timeout(timeout) => self.handle_timeout(&timeout).await,
                    //ConsensusMessage::TC(tc) => self.handle_tc(tc).await,
                    _ => panic!("Unexpected protocol message")
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
