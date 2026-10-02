//! Bounded provider-to-Store calls. Network and preparation stay outside the writer.
use crate::Work;
use fm_core::{Result, Store};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, OwnedSemaphorePermit, Semaphore};

/// A queued store operation. It holds its slot permit until it has run or
/// been dropped, so queued calls count against the slot limit.
pub(crate) struct CoreCall {
    run: Box<dyn FnOnce(&mut Store) + Send>,
    /// Where the call was made, for the slow-step log.
    pub(crate) origin: &'static str,
    _permit: OwnedSemaphorePermit,
}
impl CoreCall {
    /// Runs the operation on the writer thread and sends its result back.
    pub(crate) fn run(self, store: &mut Store) {
        (self.run)(store);
    }
}

/// A runtime's handle to the store-writer thread. Cloning shares the queue
/// and the slot semaphore.
#[derive(Clone)]
pub(crate) struct CoreBridge {
    queue: mpsc::Sender<Work>,
    slots: Arc<Semaphore>,
}
impl CoreBridge {
    /// Creates a bridge whose calls each hold one of `slots` while queued or running.
    pub(crate) fn new(queue: mpsc::Sender<Work>, slots: Arc<Semaphore>) -> Self {
        Self { queue, slots }
    }

    /// Runs `operation` on the writer thread and returns its result.
    ///
    /// Waits for a slot and for queue space, so callers are back-pressured
    /// rather than refused. Keep `operation` short: network and parsing
    /// belong outside it. Fails with `engine_stopped` when the writer has gone
    /// away. If this future is dropped before the call starts, the operation
    /// never runs.
    pub(crate) async fn call<T, F>(&self, operation: F) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce(&mut Store) -> Result<T> + Send + 'static,
    {
        let permit = self
            .slots
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| stopped())?;
        let (reply, receive) = oneshot::channel();
        self.queue
            .send(Work::Provider(CoreCall {
                run: Box::new(move |store| {
                    // Finish admitted short mutations; an abandoned queued request
                    // must never start a new mutation after its owner leaves.
                    if !reply.is_closed() {
                        let _ = reply.send(operation(store));
                    }
                }),
                origin: std::any::type_name::<F>(),
                _permit: permit,
            }))
            .await
            .map_err(|_| stopped())?;
        receive.await.map_err(|_| stopped())?
    }
}

fn stopped() -> fm_core::CoreError {
    fm_core::CoreError {
        code: "engine_stopped".into(),
        message: "The local engine session ended".into(),
        details: serde_json::Value::Null,
    }
}
