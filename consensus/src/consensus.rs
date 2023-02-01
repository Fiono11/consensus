/*use crate::config::{Committee, Parameters};
use crate::core::Core;
use crate::error::ConsensusError;
use crate::helper::Helper;
use crate::leader::LeaderElector;
use crate::mempool::MempoolDriver;
use crate::messages::{Block, Timeout, Vote, TC};
use crate::proposer::Proposer;
use crate::synchronizer::Synchronizer;*/
use async_trait::async_trait;
use bytes::Bytes;
use crypto::{Digest, PublicKey, SignatureService};
use futures::SinkExt as _;
use log::info;
use network::{MessageHandler, Receiver as NetworkReceiver, Writer};
use serde::{Deserialize, Serialize};
use std::error::Error;
use store::Store;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use mempool::ConsensusMempoolMessage;
use crate::error::ConsensusError;
use crate::messages::{Block, TC, Timeout};
use crate::config::{Committee, Parameters};
use crate::core::Core;
use crate::mempool::MempoolDriver;
use crate::message::Message;
use crate::vote::Vote;

//#[cfg(test)]
//#[path = "tests/consensus_tests.rs"]
//pub mod consensus_tests;

/// The default channel capacity for each channel of the consensus.
pub const CHANNEL_CAPACITY: usize = 1_000;

/// The consensus round number.
pub type Round = u64;

#[derive(Serialize, Deserialize, Debug)]
pub enum ConsensusMessage {
    //Propose(Block),
    //Vote(Vote),
    Message(Message),
    //Timeout(Timeout),
    //TC(TC),
    //SyncRequest(Digest, PublicKey),
}

pub struct Consensus;

impl Consensus {
    #[allow(clippy::too_many_arguments)]
    pub fn spawn(
        name: PublicKey,
        committee: Committee,
        parameters: Parameters,
        signature_service: SignatureService,
        store: Store,
    ) {
        // NOTE: This log entry is used to compute performance.
        parameters.log();

        let (tx_consensus, rx_consensus) = channel(CHANNEL_CAPACITY);
        let (tx_transaction, rx_transaction) = channel(CHANNEL_CAPACITY);

        // Spawn the network receiver.
        let mut address = committee
            .address(&name)
            .expect("Our public key is not in the committee");
        address.set_ip("0.0.0.0".parse().unwrap());

        // We first receive clients' transactions from the network.
        let mut tx_address = committee
            .transactions_address(&name)
            .expect("Our public key is not in the committee");
        address.set_ip("0.0.0.0".parse().unwrap());

        NetworkReceiver::spawn(
            address,
            /* handler */
            ConsensusReceiverHandler {
                tx_consensus,
            },
        );

        NetworkReceiver::spawn(
            tx_address,
            /* handler */
            TxReceiverHandler {
                tx_transaction,
            },
        );

        info!(
            "Node {} listening to consensus messages on {}",
            name, address
        );

        // Spawn the consensus core.
        Core::spawn(
            name,
            committee.clone(),
            signature_service.clone(),
            store.clone(),
            /* rx_message */ rx_consensus,
            false,
            rx_transaction,
        );
    }
}

/// Defines how the network receiver handles incoming primary messages.
#[derive(Clone)]
struct ConsensusReceiverHandler {
    tx_consensus: Sender<ConsensusMessage>,
}

#[async_trait]
impl MessageHandler for ConsensusReceiverHandler {
    async fn dispatch(&self, writer: &mut Writer, serialized: Bytes) -> Result<(), Box<dyn Error>> {
        // Deserialize and parse the message.
        match bincode::deserialize(&serialized).map_err(ConsensusError::SerializationError)? {
            message => self
                .tx_consensus
                .send(message)
                .await
                .expect("Failed to consensus message"),
        }
        Ok(())
    }
}

pub type Transaction = Vec<u8>;

/// Defines how the network receiver handles incoming transactions.
#[derive(Clone)]
struct TxReceiverHandler {
    tx_transaction: Sender<Transaction>,
}

#[async_trait]
impl MessageHandler for TxReceiverHandler {
    async fn dispatch(&self, _writer: &mut Writer, message: Bytes) -> Result<(), Box<dyn Error>> {
        // Send the transaction to the batch maker.
        self.tx_transaction
            .send(message.to_vec())
            .await
            .expect("Failed to send transaction");

        // Give the change to schedule other tasks.
        tokio::task::yield_now().await;
        Ok(())
    }
}

