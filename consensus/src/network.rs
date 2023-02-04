use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex};
use std::sync::mpsc::{channel, Receiver};
use std::thread;
use crypto::{Digest, PublicKey};
use crate::constants::{NUMBER_OF_CORRECT_NODES, NUMBER_OF_TXS};
use crate::message::Message;
use crate::node::Node;
use crate::NUMBER_OF_NODES;
use crate::vote::Category::Decided;
use crate::vote::{ParentHash, Transaction, Vote};

#[derive(Debug)]
pub(crate) struct Network {
    pub(crate) nodes: HashMap<PublicKey, Arc<Mutex<Node>>>,
    receiver: Arc<Mutex<Receiver<Message>>>,
}

impl Network {
    /// Create a new network with `n` participating nodes.
    pub(crate) fn new() -> Self {
        let (sender, receiver) = channel();
        let mut nodes = HashMap::new();
        let mut pks = Vec::new();
        for _ in 0..NUMBER_OF_NODES {
            let pk = PublicKey(Digest::random().0);
            pks.push(pk);
        }
        for i in 0..NUMBER_OF_NODES {
            if i >= NUMBER_OF_CORRECT_NODES {
                nodes.insert(pks[i], Arc::new(Mutex::new(Node::new(pks[i], sender.clone(), true, pks.clone()))));
            }
            else {
                nodes.insert(pks[i], Arc::new(Mutex::new(Node::new(pks[i], sender.clone(), false, pks.clone()))));
            }
        }
        for (pk, node) in nodes.clone() {
            println!("node {:?}: {:?}", pk, node);
        }
        Network {
            nodes,
            receiver: Arc::new(Mutex::new(receiver)),
        }
    }

    pub(crate) fn run(&self) {
        let receiver = self.receiver.clone();
        let mut nodes = self.nodes.clone();
        let mut decisions: BTreeSet<(PublicKey, Transaction)> = BTreeSet::new();

        // Start a new thread to receive messages
        let receive_thread = thread::spawn(move || {
            loop {
                let message = receiver.lock().unwrap().recv().unwrap();
                let sender = message.sender;
                let vote = message.vote.clone();
                let receiver = message.receiver;
                let round = message.vote.round;
                if vote.category == Decided && !nodes.get(&vote.signer).unwrap().lock().unwrap().byzantine {
                    let insert = decisions.insert((sender, vote.value.clone()));
                    //if insert.is_none() {
                        println!("Node {} decided value {:?} in round {}", sender, &vote.value, round);
                    //}
                }
                if decisions.len() == NUMBER_OF_CORRECT_NODES * NUMBER_OF_TXS {
                    let decision = decisions.iter().next().unwrap().1.clone();
                    for (_, d) in &decisions {
                        if &decision.parent_hash == &d.parent_hash {
                            assert_eq!(&decision.tx_hash, &d.tx_hash);
                        }
                    }
                    println!("CONSENSUS ACHIEVED!!!");
                    break;
                }
                let node = nodes.get_mut(&receiver).unwrap();
                node.lock().unwrap().handle_message(&message);
            }
        });

        for _ in 0..NUMBER_OF_TXS {
            let parent_hash = ParentHash(Digest::random());
            for (id, node) in &self.nodes {
                let vote = Vote::random(*id, parent_hash.clone());
                let guard = node.lock().unwrap();
                guard.send_vote(vote);
            }
        }

        // Wait for the receive thread to finish
        receive_thread.join().unwrap();
    }
}

