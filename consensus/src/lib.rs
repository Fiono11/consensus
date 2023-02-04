extern crate core;

use crate::constants::NUMBER_OF_NODES;
use crate::network::Network;

mod node;
mod constants;
mod network;
mod election;
mod vote;
mod round;
mod tally;
mod message;

#[test]
fn test_consensus() {
    for i in 0..1 {
        let network = Network::new();
        println!("CONSENSUS INSTANCE {} RUNNING...", i);
        network.run();
    }
}



