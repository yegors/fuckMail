//! Bounded provider-to-Store calls. Network and preparation stay outside the writer.
use crate::Work;
use fm_core::{Result, Store};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, OwnedSemaphorePermit, Semaphore};

pub(crate) struct CoreCall {
    run: Box<dyn FnOnce(&mut Store) + Send>,
    _permit: OwnedSemaphorePermit,
}
impl CoreCall {
    pub(crate) fn run(self, store: &mut Store) {
        (self.run)(store);
    }
}

#[derive(Clone)]
pub(crate) struct CoreBridge {
    queue: mpsc::Sender<Work>,
    slots: Arc<Semaphore>,
}
impl CoreBridge {
    pub(crate) fn new(queue: mpsc::Sender<Work>, slots: Arc<Semaphore>) -> Self {
        Self { queue, slots }
    }

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
