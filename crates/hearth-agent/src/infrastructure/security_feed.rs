//! Diffusion interne des changements de l'état de sécurité : un canal `tokio::sync::broadcast`, en
//! mémoire, qui ne porte aucune donnée (voir `SecurityFeed`).

use tokio::sync::broadcast;

use crate::application::ports::SecurityFeed;

/// Tics gardés pour un abonné lent avant qu'il n'en perde : sans conséquence, il relit son état.
const CAPACITY: usize = 16;

pub struct BroadcastSecurityFeed {
    sender: broadcast::Sender<()>,
}

impl BroadcastSecurityFeed {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(CAPACITY);
        Self { sender }
    }
}

impl Default for BroadcastSecurityFeed {
    fn default() -> Self {
        Self::new()
    }
}

impl SecurityFeed for BroadcastSecurityFeed {
    fn publish(&self) {
        // Sans abonné, `send` rend une erreur : personne n'écoute, ce n'est pas un échec.
        let _ = self.sender.send(());
    }

    fn subscribe(&self) -> broadcast::Receiver<()> {
        self.sender.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_subscriber_gets_the_ticks_published_after_it_subscribed() {
        let feed = BroadcastSecurityFeed::new();
        feed.publish();
        let mut receiver = feed.subscribe();
        feed.publish();
        assert!(receiver.recv().await.is_ok());
        assert!(
            receiver.try_recv().is_err(),
            "un seul tic depuis l'abonnement"
        );
    }

    #[test]
    fn publishing_without_subscriber_is_not_an_error() {
        BroadcastSecurityFeed::new().publish();
    }
}
