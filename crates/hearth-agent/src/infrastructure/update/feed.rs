//! Diffusion interne de la progression de la mise à jour : un canal `tokio::sync::broadcast` en
//! mémoire. Rien n'est conservé ici (l'état courant se relit à l'abonnement, le résultat est un
//! fichier) ; un abonné absent ne coûte rien.

use hearth_proto::api::update::UpdateProgress;
use tokio::sync::broadcast;

use crate::application::ports::UpdateFeed;

const CAPACITY: usize = 64;

pub struct BroadcastUpdateFeed {
    sender: broadcast::Sender<UpdateProgress>,
}

impl BroadcastUpdateFeed {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(CAPACITY);
        Self { sender }
    }
}

impl Default for BroadcastUpdateFeed {
    fn default() -> Self {
        Self::new()
    }
}

impl UpdateFeed for BroadcastUpdateFeed {
    fn publish(&self, progress: UpdateProgress) {
        // Sans abonné, `send` rend une erreur : personne n'écoute, ce n'est pas un échec.
        let _ = self.sender.send(progress);
    }

    fn subscribe(&self) -> broadcast::Receiver<UpdateProgress> {
        self.sender.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use hearth_proto::api::update::UpdateStep;

    use super::*;

    fn progress(step: UpdateStep) -> UpdateProgress {
        UpdateProgress {
            version: "0.2.0".into(),
            step,
            percent: None,
            outcome: None,
            reason: None,
        }
    }

    #[test]
    fn publishing_without_a_subscriber_is_not_an_error_and_subscribers_get_what_follows() {
        let feed = BroadcastUpdateFeed::new();
        feed.publish(progress(UpdateStep::Download));
        let mut receiver = feed.subscribe();
        feed.publish(progress(UpdateStep::Verify));
        assert_eq!(receiver.try_recv().unwrap().step, UpdateStep::Verify);
        assert!(receiver.try_recv().is_err());
    }
}
