use crate::constants::NUMBER_OF_NODES;
pub use crate::messages::Block;
pub use crate::consensus::Consensus;
pub use crate::config::{Committee, Parameters};

//mod node;
mod constants;
//mod network;
mod election;
mod vote;
mod round;
mod tally;
mod message;
mod consensus;
mod messages;
mod error;
mod config;
//mod synchronizer;
mod mempool;
//mod helper;
mod core;

#[test]
fn test_consensus() {
    for i in 0..1 {
        let network = Network::new();
        println!("CONSENSUS INSTANCE {} RUNNING...", i);
        network.run();
    }
}



