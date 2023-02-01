use crypto::Digest;
use ed25519_dalek::Digest as _;
use ed25519_dalek::Sha512;
use std::convert::TryInto;
use log::info;
use store::Store;
use tokio::sync::mpsc::{Receiver, Sender};

#[cfg(test)]
#[path = "tests/processor_tests.rs"]
pub mod processor_tests;

/// Indicates a serialized `MempoolMessage::Batch` message.
pub type SerializedBatchMessage = Vec<u8>;

/// Hashes and stores batches, it then outputs the batch's digest.
pub struct Processor;

impl Processor {
    pub fn spawn(
        // The persistent storage.
        mut store: Store,
        // Input channel to receive batches.
        mut rx_batch: Receiver<SerializedBatchMessage>,
        // Input channel to receive batches.
        mut rx_new_batch: Receiver<Vec<Digest>>,
        // Output channel to send out batches' digests.
        tx_digest: Sender<Digest>,
        // Output channel to send out batches' digests.
        tx_digests: Sender<Vec<Digest>>,
    ) {
        tokio::spawn(async move {
            loop {
                tokio::select! {
                //while let Some(batch) = rx_new_batch.recv().await {
                    Some(batch) = rx_new_batch.recv() => {
                        info!("Received digests from self: {:?}", batch);
                        // Hash the batch.
                        //let digest = Digest(Sha512::digest(&batch).as_slice()[..32].try_into().unwrap());

                        // Store the batch.
                        //store.write(digest.to_vec(), batch).await;

                        //tx_digest.send(digest).await.expect("Failed to send digest");
                        tx_digests.send(batch).await.expect("Failed to send digests");
                        info!("///////////////////////////");
                    }
                }
            }
        });
    }
}
