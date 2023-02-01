use crate::mempool::MempoolMessage;
use crate::quorum_waiter::QuorumWaiterMessage;
use bytes::Bytes;
//#[cfg(feature = "benchmark")]
use crypto::Digest;
use crypto::{Hash, PublicKey};
//#[cfg(feature = "benchmark")]
use ed25519_dalek::{Digest as _, Sha512};
//#[cfg(feature = "benchmark")]
use log::info;
use network::{ReliableSender, SimpleSender};
//#[cfg(feature = "benchmark")]
use std::convert::TryInto as _;
use std::fmt::Debug;
use std::io::Write;
use std::net::SocketAddr;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::time::{sleep, Duration, Instant};
use crate::processor::SerializedBatchMessage;

#[cfg(test)]
#[path = "tests/batch_maker_tests.rs"]
pub mod batch_maker_tests;

pub type Transaction = Vec<u8>;
pub type Batch = Vec<Transaction>;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NewBatch(pub Vec<NewTransaction>);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NewTransaction(pub Vec<u8>);

impl Hash for NewTransaction {
    fn digest(&self) -> Digest {
        let mut hasher = Sha512::new();
        hasher.write(&self.0);
        Digest(hasher.finalize().as_slice()[..32].try_into().unwrap())
    }
}

/// Assemble clients transactions into batches.
pub struct BatchMaker {
    /// The preferred batch size (in bytes).
    batch_size: usize,
    /// The maximum delay after which to seal the batch (in ms).
    max_batch_delay: u64,
    /// Channel to receive transactions from the network.
    rx_transaction: Receiver<Transaction>,
    /// Channel to receive new transactions from the network.
    rx_new_transaction: Receiver<NewTransaction>,
    /// Output channel to deliver sealed batches to the `QuorumWaiter`.
    tx_message: Sender<QuorumWaiterMessage>,
    /// The network addresses of the other mempools.
    mempool_addresses: Vec<(PublicKey, SocketAddr)>,
    /// Holds the current batch.
    current_batch: Batch,
    /// Holds the current batch.
    new_current_batch: NewBatch,
    /// Holds the size of the current batch (in bytes).
    current_batch_size: usize,
    /// A network sender to broadcast the batches to the other mempools.
    network: SimpleSender,
    /// Channel to deliver batches.
    tx_batch: Sender<SerializedBatchMessage>,
    /// Output channel to send out transactions digests.
    tx_digests: Sender<Vec<Digest>>,
}

impl BatchMaker {
    pub fn spawn(
        batch_size: usize,
        max_batch_delay: u64,
        rx_transaction: Receiver<Transaction>,
        rx_new_transaction: Receiver<NewTransaction>,
        tx_message: Sender<QuorumWaiterMessage>,
        mempool_addresses: Vec<(PublicKey, SocketAddr)>,
        tx_batch: Sender<SerializedBatchMessage>,
        tx_digests: Sender<Vec<Digest>>,
    ) {
        tokio::spawn(async move {
            Self {
                batch_size,
                max_batch_delay,
                rx_transaction,
                rx_new_transaction,
                tx_message,
                mempool_addresses,
                current_batch: Batch::with_capacity(batch_size * 2),
                new_current_batch: NewBatch(Vec::with_capacity(batch_size * 2)),
                current_batch_size: 0,
                network: SimpleSender::new(),
                tx_batch,
                tx_digests,
            }
            .run()
            .await;
        });
    }

    /// Main loop receiving incoming transactions and creating batches.
    async fn run(&mut self) {
        let timer = sleep(Duration::from_millis(self.max_batch_delay));
        tokio::pin!(timer);

        loop {
            tokio::select! {
                // Assemble client transactions into batches of preset size.
                Some(transaction) = self.rx_new_transaction.recv() => {
                    info!("Received tx {:?}", transaction.digest());
                    self.current_batch_size += transaction.0.len();
                    //self.current_batch.push(transaction);
                    self.new_current_batch.0.push(transaction);
                    if self.current_batch_size >= self.batch_size {
                        self.seal().await;
                        timer.as_mut().reset(Instant::now() + Duration::from_millis(self.max_batch_delay));
                    }
                },

                // If the timer triggers, seal the batch even if it contains few transactions.
                () = &mut timer => {
                    if !self.new_current_batch.0.is_empty() {
                        self.seal().await;
                    }
                    timer.as_mut().reset(Instant::now() + Duration::from_millis(self.max_batch_delay));
                }
            }

            // Give the change to schedule other tasks.
            tokio::task::yield_now().await;
        }
    }

    /// Seal and broadcast the current batch.
    async fn seal(&mut self) {
        #[cfg(feature = "benchmark")]
        let size = self.current_batch_size;

        // Look for sample txs (they all start with 0) and gather their txs id (the next 8 bytes).
        /*#[cfg(feature = "benchmark")]
        let tx_ids: Vec<_> = self
            .current_batch
            .iter()
            .filter(|tx| tx[0] == 0u8 && tx.len() > 8)
            .filter_map(|tx| tx[1..9].try_into().ok())
            .collect();*/

        // Serialize the batch.
        self.current_batch_size = 0;
        let batch: Vec<_> = self.new_current_batch.0.drain(..).collect();
        let digests: Vec<Digest> = batch.iter().map(|tx| tx.digest()).collect();
        info!("Digests: {:?}", digests);
        let message = MempoolMessage::NewBatch(NewBatch(batch.clone()));
        let serialized = bincode::serialize(&message).expect("Failed to serialize our own batch");

        #[cfg(feature = "benchmark")]
        {
            // NOTE: This is one extra hash that is only needed to print the following log entries.
            let digest = Digest(
                Sha512::digest(&serialized).as_slice()[..32]
                    .try_into()
                    .unwrap(),
            );

            for tx in &batch {
                // NOTE: This log entry is used to compute performance.
                info!(
                    "Batch {:?} contains sample tx {:?}",
                    digest,
                    tx
                    //u64::from_be_bytes(id)
                );
            }

            // NOTE: This log entry is used to compute performance.
            info!("Batch {:?} contains {} B", digest, size);
        }

        // Broadcast the batch through the network.
        let (names, addresses): (Vec<_>, _) = self.mempool_addresses.iter().cloned().unzip();
        let bytes = Bytes::from(serialized.clone());
        let handlers = self.network.broadcast(addresses, bytes).await;

        self.tx_digests
            .send(digests.clone())
            .await
            .expect("Failed to deliver digests");

        info!("Sent digests: {:?}", digests);

        //for handle in handlers {
            //handle.await;
        //}

        /*self.tx_batch
            .send(serialized)
            .await
            .expect("Failed to deliver batch");*/

        // Send the batch through the deliver channel for further processing.
        /*self.tx_message
            .send(QuorumWaiterMessage {
                batch: serialized,
                handlers: names.into_iter().zip(handlers.into_iter()).collect(),
            })
            .await
            .expect("Failed to deliver batch");*/
    }
}
