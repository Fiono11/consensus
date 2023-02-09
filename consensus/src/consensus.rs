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
use log::{debug, info};
use network::{MessageHandler, Receiver as NetworkReceiver, Writer};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::net::SocketAddr;
use store::Store;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use crate::error::ConsensusError;
use crate::messages::{Block, TC, Timeout};
use crate::config::{Committee, Parameters};
use crate::core::{Core, DagError};
use crate::message::Message;
use crate::vote::{Transaction, Vote};

//#[cfg(test)]
//#[path = "tests/consensus_tests.rs"]
//pub mod consensus_tests;

/// The default channel capacity for each channel of the consensus.
pub const CHANNEL_CAPACITY: usize = 1_000;

/// The consensus round number.
pub type Round = u64;

#[derive(Serialize, Deserialize, Debug)]
pub enum ConsensusMessage {
    Transaction(Vec<Transaction>),
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
        tx_commit: Sender<Block>,
    ) {
        // NOTE: This log entry is used to compute performance.
        parameters.log();

        let (tx_vote, rx_vote) = channel(CHANNEL_CAPACITY);
        let (tx_transaction, rx_transaction) = channel(CHANNEL_CAPACITY);

        // Spawn the network receiver.
        let mut address = committee
            .address(&name)
            .expect("Our public key is not in the committee");
        address.set_ip("0.0.0.0".parse().unwrap());

        // We first receive clients' transactions from the network.
        /*let mut tx_address = committee
            .transactions_address(&name)
            .expect("Our public key is not in the committee");
        tx_address.set_ip("0.0.0.0".parse().unwrap());

        NetworkReceiver::spawn(
            tx_address,
            /* handler */
            TxReceiverHandler {
                tx_transaction,
            },
        );*/

        NetworkReceiver::spawn(
            address,
            /* handler */
            ConsensusReceiverHandler {
                tx_vote, tx_transaction
            },
        );

        info!(
            "Node {} listening to consensus messages on {}",
            name, address
        );

        //info!("Mempool listening to client transactions on {}", tx_address);

        let broadcast_addresses = committee
            .broadcast_addresses(&name);
        let mut addresses: Vec<SocketAddr> = Vec::new();
        let mut public_keys: Vec<PublicKey> = Vec::new();
        for (pk, sa) in broadcast_addresses {
            addresses.push(sa);
            public_keys.push(pk);
        }

        // Spawn the consensus core.
        Core::spawn(
            name,
            committee.clone(),
            signature_service.clone(),
            store.clone(),
            rx_vote,
            committee.authorities.get(&name).unwrap().byzantine,
            rx_transaction,
            tx_commit,
            (public_keys, addresses),
        );
    }
}

/// Defines how the network receiver handles incoming primary messages.
#[derive(Clone)]
struct ConsensusReceiverHandler {
    tx_vote: Sender<Vote>,
    tx_transaction: Sender<Transaction>,
}

#[async_trait]
impl MessageHandler for ConsensusReceiverHandler {
    async fn dispatch(&self, writer: &mut Writer, serialized: Bytes) -> Result<(), Box<dyn Error>> {
        //debug!("Received consensus message!");
        // Reply with an ACK.
        //let _ = writer.send(Bytes::from("Ack")).await;
        // Deserialize and parse the message.
        /*match bincode::deserialize(&serialized).map_err(ConsensusError::SerializationError)? {
            message => self
                .tx_consensus
                .send(message)
                .await
                .expect("Failed to consensus message"),
        }*/

        match bincode::deserialize(&serialized).map_err(DagError::SerializationError)? {
            ConsensusMessage::Transaction(txs) => {
                //info!("Received tx!");
                for tx in txs {
                    self.tx_transaction
                        .send(tx)
                        .await
                        .expect("Failed to send transaction")
                }
            },
            ConsensusMessage::Message(msg) => {
                //info!("Received vote!");
                self.tx_vote
                    .send(msg.vote)
                    .await
                    .expect("Failed to send vote")
            },
            //Err(e) => warn!("Serialization error: {}", e),
        }

        // Give the change to schedule other tasks.
        tokio::task::yield_now().await;
        Ok(())
    }
}

/*/// Defines how the network receiver handles incoming transactions.
#[derive(Clone)]
struct TxReceiverHandler {
    tx_transaction: Sender<Transaction>,
}

#[async_trait]
impl MessageHandler for TxReceiverHandler {
    async fn dispatch(&self, _writer: &mut Writer, tx: Bytes) -> Result<(), Box<dyn Error>> {
        // Deserialize and parse the transaction.
        match bincode::deserialize(&tx).map_err(ConsensusError::SerializationError)? {
            message => self
                .tx_transaction
                .send(message)
                .await
                .expect("Failed to transaction"),
        }

        // Give the change to schedule other tasks.
        tokio::task::yield_now().await;
        Ok(())
    }
}*/

