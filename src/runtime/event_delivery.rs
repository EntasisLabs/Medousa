//! Lifetime ownership for best-effort runtime observations in headless hosts.
//!
//! This is not a turn journal, delivery outbox, scheduler, or authority. Dropping
//! the owner requests asynchronous abortion of only the observation drain. The
//! receiver closes when Tokio next processes cancellation, not synchronously in
//! Drop. This does not stop durable workers or all background tasks; durable work
//! has its own shutdown.

use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::events::RuntimeEvent;

pub(crate) struct HeadlessEventDrain {
    task: JoinHandle<()>,
}

impl HeadlessEventDrain {
    pub(crate) fn start() -> (mpsc::Sender<RuntimeEvent>, Self) {
        let (sender, mut receiver) = mpsc::channel(256);
        let task = tokio::spawn(async move { while receiver.recv().await.is_some() {} });
        (sender, Self { task })
    }
}

impl Drop for HeadlessEventDrain {
    fn drop(&mut self) {
        // Requests asynchronous cancellation; receiver closure requires runtime progress.
        // Also covers cancelled/failed construction, even if tools retained a sender.
        // Does not shut down durable workers or other background tasks.
        self.task.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::HeadlessEventDrain;
    use crate::events::RuntimeEvent;
    use std::time::Duration;

    #[tokio::test]
    async fn owner_drop_stops_drain_with_live_producer() {
        let (sender, owner) = HeadlessEventDrain::start();
        sender
            .send(RuntimeEvent::UiNotice("observation".into()))
            .await
            .unwrap();
        drop(owner);
        tokio::time::timeout(Duration::from_secs(1), sender.closed())
            .await
            .unwrap();
        assert!(
            sender
                .send(RuntimeEvent::UiNotice("late".into()))
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn failed_construction_does_not_detach_drain() {
        let (sender, owner) = HeadlessEventDrain::start();
        let construction: Result<(), ()> = async move {
            let _owned_until_result = owner;
            Err(())
        }
        .await;
        assert!(construction.is_err());
        tokio::time::timeout(Duration::from_secs(1), sender.closed())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn cancelled_construction_drops_its_drain() {
        let (sender, owner) = HeadlessEventDrain::start();
        let construction = async move {
            let _owned_until_result = owner;
            std::future::pending::<()>().await;
        };
        // The pending constructor owns the drain even before it is first polled.
        drop(construction);
        tokio::time::timeout(Duration::from_secs(1), sender.closed())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn producer_completion_finishes_drain_naturally() {
        let (sender, owner) = HeadlessEventDrain::start();
        drop(sender);
        tokio::time::timeout(Duration::from_secs(1), async {
            while !owner.task.is_finished() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }
}
