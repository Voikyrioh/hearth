//! Diffusion interne des entrées du journal : un canal `tokio::sync::broadcast`, en mémoire.
//! Rien n'est conservé ici (le journal est en base) ; un abonné absent ne coûte rien.

use tokio::sync::broadcast;

use crate::application::ports::AuditFeed;
use crate::domain::audit::AuditRecord;

/// Entrées gardées pour un abonné lent avant qu'il n'en perde (`Lagged`).
const CAPACITY: usize = 256;

pub struct BroadcastAuditFeed {
    sender: broadcast::Sender<AuditRecord>,
}

impl BroadcastAuditFeed {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(CAPACITY);
        Self { sender }
    }
}

impl Default for BroadcastAuditFeed {
    fn default() -> Self {
        Self::new()
    }
}

impl AuditFeed for BroadcastAuditFeed {
    fn publish(&self, record: AuditRecord) {
        // Sans abonné, `send` rend une erreur : personne n'écoute, ce n'est pas un échec.
        let _ = self.sender.send(record);
    }

    fn subscribe(&self) -> broadcast::Receiver<AuditRecord> {
        self.sender.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use time::OffsetDateTime;

    use super::*;
    use crate::domain::audit::{OriginKind, OutcomeKind};

    fn record(id: i64) -> AuditRecord {
        AuditRecord {
            id,
            at: OffsetDateTime::UNIX_EPOCH,
            account: None,
            origin_kind: OriginKind::CommandLine,
            origin_name: None,
            origin_addr: None,
            action: "login".into(),
            action_label: "Connexion".into(),
            target: None,
            outcome: OutcomeKind::Ok,
            reason: None,
            repeat_count: 0,
            repeat_addresses: 0,
        }
    }

    #[test]
    fn publishing_without_a_subscriber_is_not_an_error() {
        BroadcastAuditFeed::new().publish(record(1));
    }

    #[tokio::test]
    async fn every_subscriber_receives_what_is_published_after_it_subscribed() {
        let feed = BroadcastAuditFeed::new();
        feed.publish(record(1));
        let mut first = feed.subscribe();
        let mut second = feed.subscribe();
        feed.publish(record(2));
        assert_eq!(first.recv().await.unwrap().id, 2);
        assert_eq!(second.recv().await.unwrap().id, 2);
    }

    #[tokio::test]
    async fn a_slow_subscriber_loses_the_oldest_entries_and_is_told() {
        let feed = BroadcastAuditFeed::new();
        let mut slow = feed.subscribe();
        for id in 0..400 {
            feed.publish(record(id));
        }
        assert!(matches!(
            slow.recv().await,
            Err(broadcast::error::RecvError::Lagged(_))
        ));
        assert!(slow.recv().await.unwrap().id >= 144);
    }
}
