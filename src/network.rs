use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::sync::mpsc::{channel, Receiver};
use std::thread;
use crate::constants::NUMBER_OF_CORRECT_NODES;
use crate::message::Message;
use crate::node::{Id, Node};
use crate::NUMBER_OF_NODES;
use crate::vote::Category::Decided;
use crate::vote::Vote;

#[derive(Debug)]
pub(crate) struct Network {
    pub(crate) nodes: HashMap<Id, Arc<Mutex<Node>>>,
    receiver: Arc<Mutex<Receiver<Message>>>,
}

impl Network {
    /// Create a new network with `n` participating nodes.
    pub(crate) fn new() -> Self {
        let (sender, receiver) = channel();
        let mut nodes = HashMap::new();
        for i in 0..NUMBER_OF_NODES {
            if i >= NUMBER_OF_CORRECT_NODES {
                nodes.insert(i, Arc::new(Mutex::new(Node::new(i, sender.clone(), true))));
            }
            else {
                nodes.insert(i, Arc::new(Mutex::new(Node::new(i, sender.clone(), false))));
            }
        }
        println!("nodes: {:?}", nodes.clone());
        Network {
            nodes,
            receiver: Arc::new(Mutex::new(receiver)),
        }
    }

    pub(crate) fn run(&self) {
        let receiver = self.receiver.clone();
        let mut nodes = self.nodes.clone();
        let mut decisions = HashMap::new();

        // Start a new thread to receive messages
        let receive_thread = thread::spawn(move || {
            loop {
                let message = receiver.lock().unwrap().recv().unwrap();
                let sender = message.sender;
                let vote = message.vote.clone();
                let receiver = message.receiver;
                let round = message.vote.round;
                if vote.category == Decided && !nodes.get(&vote.signer).unwrap().lock().unwrap().byzantine {
                    let insert = decisions.insert(sender, vote.value.clone());
                    if insert.is_none() {
                        println!("Node {} decided value {:?} in round {}", sender, &vote.value, round);
                    }
                }
                if decisions.len() == NUMBER_OF_CORRECT_NODES {
                    let decision = decisions.get(&0).unwrap();
                    for (_, d) in &decisions {
                        assert_eq!(decision, d);
                    }
                    println!("CONSENSUS ACHIEVED!!!");
                    break;
                }
                let node = nodes.get_mut(&receiver).unwrap();
                node.lock().unwrap().handle_message(&message);
            }
        });

        for (id, node) in &self.nodes {
            let vote = Vote::random_initial(*id);
            let guard = node.lock().unwrap();
            guard.send_vote(vote);
        }

        // Wait for the receive thread to finish
        receive_thread.join().unwrap();
    }
}

