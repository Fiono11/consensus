use async_trait::async_trait;
use bytes::Bytes;
use crypto::{PublicKey, SignatureService};
use log::info;
use network::{MessageHandler, Receiver as NetworkReceiver, Writer};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::net::SocketAddr;
use store::Store;
use tokio::sync::mpsc::{channel, Sender};
use crate::messages::Block;
use crate::config::{Committee, Parameters};
use crate::core::{Core, DagError};
use crate::message::Message;
use crate::vote::{Transaction, Vote};

/// The default channel capacity for each channel of the consensus.
pub const CHANNEL_CAPACITY: usize = 1_000;

/// The consensus round number.
pub type Round = u64;

#[derive(Serialize, Deserialize, Debug)]
pub enum ConsensusMessage {
    Transaction(Vec<Transaction>),
    Message(Message),
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
    async fn dispatch(&self, _writer: &mut Writer, serialized: Bytes) -> Result<(), Box<dyn Error>> {
        match bincode::deserialize(&serialized).map_err(DagError::SerializationError)? {
            ConsensusMessage::Transaction(txs) => {
                for tx in txs {
                    self.tx_transaction
                        .send(tx)
                        .await
                        .expect("Failed to send transaction")
                }
            },
            ConsensusMessage::Message(msg) => {
                self.tx_vote
                    .send(msg.vote)
                    .await
                    .expect("Failed to send vote")
            },
        }

        // Give the change to schedule other tasks.
        tokio::task::yield_now().await;
        Ok(())
    }
}
