

//#[cfg(test)]
//#[path = "tests/core_tests.rs"]
//pub mod core_tests;

use bytes::Bytes;
use log::{debug, error, warn};
use tokio::sync::mpsc::Receiver;
use tokio::time::timeout;
use crypto::{Digest, PublicKey, SignatureService};
use network::SimpleSender;
use store::Store;
use crate::{Block, Committee, Transaction};
use crate::consensus::ConsensusMessage;
use crate::error::{ConsensusError, ConsensusResult};
use crate::message::Message;
use crate::messages::Timeout;
use crate::round::Timer;
use crate::vote::{ParentHash, Vote};

pub struct Core {
    name: PublicKey,
    committee: Committee,
    rx_message: Receiver<ConsensusMessage>,
    /// Channel to receive transactions from the network.
    rx_transaction: Receiver<Transaction>,
    network: SimpleSender,
}

impl Core {
    #[allow(clippy::too_many_arguments)]
    pub fn spawn(
        name: PublicKey,
        committee: Committee,
        rx_transaction: Receiver<Transaction>,
        rx_message: Receiver<ConsensusMessage>,
    ) {
        tokio::spawn(async move {
            Self {
                name,
                committee: committee.clone(),
                rx_message,
                rx_transaction,
                network: SimpleSender::new(),
            }
            .run()
            .await
        });
    }

    /*async fn local_timeout_round(&mut self) -> ConsensusResult<()> {
        warn!("Timeout reached for round");

        // Broadcast the timeout message.
        debug!("Broadcasting {:?}", timeout);
        let addresses = self
            .committee
            .broadcast_addresses(&self.name)
            .into_iter()
            .map(|(_, x)| x)
            .collect();
        let message = bincode::serialize(&ConsensusMessage::Timeout(timeout.clone()))
            .expect("Failed to serialize timeout message");
        self.network
            .broadcast(addresses, Bytes::from(message))
            .await;
        Ok(())
    }*/

    async fn handle_tx(&mut self) -> ConsensusResult<()> {

        // Broadcast the TC.
        debug!("Broadcasting...");
        let addresses = self
            .committee
            .broadcast_addresses(&self.name)
            .into_iter()
            .map(|(_, x)| x)
            .collect();
        let msg = Message::new(self.name, Vote::random(self.name, ParentHash(Digest::random())));
        let message = bincode::serialize(&ConsensusMessage::Message(msg)).expect("Failed to serialize timeout certificate");
        self.network
            .broadcast(addresses, Bytes::from(message))
            .await;

        Ok(())
    }

    async fn handle_timeout(&mut self, vote: Vote) -> ConsensusResult<()> {

            // Broadcast the TC.
            debug!("Broadcasting {:?}", &vote);
            let addresses = self
                .committee
                .broadcast_addresses(&self.name)
                .into_iter()
                .map(|(_, x)| x)
                .collect();
            let msg = Message::new(self.name, vote.clone());
            let message = bincode::serialize(&ConsensusMessage::Message(msg)).expect("Failed to serialize timeout certificate");
            self.network
                .broadcast(addresses, Bytes::from(message))
                .await;

        Ok(())
    }

    pub async fn run(&mut self) {
        // This is the main loop: it processes incoming blocks and votes,
        // and receive timeout notifications from our Timeout Manager.
        loop {
            let result = tokio::select! {
                Some(message) = self.rx_message.recv() => match message {
                    ConsensusMessage::Message(msg) => self.handle_timeout(msg.vote.clone()).await,
                    _ => panic!("Unexpected protocol message")
                },
                Some(transaction) = self.rx_transaction.recv() => self.handle_tx().await,
            };
            match result {
                Ok(()) => (),
                Err(ConsensusError::StoreError(e)) => error!("{}", e),
                Err(ConsensusError::SerializationError(e)) => error!("Store corrupted. {}", e),
                Err(e) => warn!("{}", e),
            }
        }
    }
}
