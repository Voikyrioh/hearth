//! La progression d'une récupération d'image (HRT-68, BR-DEPLOY-016) : le moteur annonce des événements
//! PAR COUCHE ; le total n'est connu qu'au fil de l'eau. Fonction pure, sans E/S.
//!
//! Le pourcentage ne recule jamais (une couche annoncée tard agrandit le total) et ne dit « 100 » que quand
//! le moteur a fini : tant que rien n'est terminé, il plafonne à 99.

use std::collections::HashMap;

/// Plus de couches que cela : le suivi s'arrête de croître (une image légitime en a quelques dizaines).
const MAX_LAYERS: usize = 512;

/// Ce que le moteur dit d'une couche.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayerEvent {
    /// Téléchargement en cours : octets reçus, octets attendus (0 : inconnu).
    Downloading {
        layer: String,
        current: u64,
        total: u64,
    },
    /// La couche est là (téléchargée, extraite ou déjà présente).
    Done { layer: String },
}

/// La progression rendue à l'appelant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PullProgress {
    /// `None` tant qu'aucun total n'est connu (progression indéterminée).
    pub percent: Option<u8>,
    pub bytes_done: u64,
    pub bytes_total: Option<u64>,
}

#[derive(Debug, Default)]
pub struct PullTracker {
    layers: HashMap<String, (u64, u64)>,
    last_percent: u8,
}

impl PullTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Prend en compte un événement et rend la progression.
    pub fn apply(&mut self, event: LayerEvent) -> PullProgress {
        match event {
            LayerEvent::Downloading {
                layer,
                current,
                total,
            } => {
                if self.layers.contains_key(&layer) || self.layers.len() < MAX_LAYERS {
                    let entry = self.layers.entry(layer).or_insert((0, 0));
                    // Un total qui rétrécit ou une valeur qui recule est du bruit : on garde le plus grand.
                    entry.1 = entry.1.max(total);
                    entry.0 =
                        entry
                            .0
                            .max(current)
                            .min(if entry.1 > 0 { entry.1 } else { u64::MAX });
                }
            }
            LayerEvent::Done { layer } => {
                if self.layers.contains_key(&layer) || self.layers.len() < MAX_LAYERS {
                    let entry = self.layers.entry(layer).or_insert((0, 0));
                    entry.0 = entry.1;
                }
            }
        }
        self.progress()
    }

    pub fn progress(&mut self) -> PullProgress {
        let total: u64 = self.layers.values().map(|(_, total)| *total).sum();
        let done: u64 = self.layers.values().map(|(done, _)| *done).sum();
        if total == 0 {
            return PullProgress {
                percent: None,
                bytes_done: done,
                bytes_total: None,
            };
        }
        let computed =
            u8::try_from((u128::from(done) * 100 / u128::from(total)).min(99)).unwrap_or(99);
        self.last_percent = self.last_percent.max(computed);
        PullProgress {
            percent: Some(self.last_percent),
            bytes_done: done,
            bytes_total: Some(total),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn down(layer: &str, current: u64, total: u64) -> LayerEvent {
        LayerEvent::Downloading {
            layer: layer.into(),
            current,
            total,
        }
    }

    #[test]
    fn nothing_is_known_until_a_total_is_announced() {
        let mut tracker = PullTracker::new();
        let progress = tracker.apply(LayerEvent::Done { layer: "a".into() });
        assert_eq!(progress.percent, None);
        assert_eq!(progress.bytes_total, None);
    }

    #[test]
    fn the_layers_add_up_and_the_percent_never_goes_back() {
        let mut tracker = PullTracker::new();
        assert_eq!(tracker.apply(down("a", 50, 100)).percent, Some(50));
        // Une couche annoncée tard agrandit le total : le pourcentage ne recule pas.
        let after = tracker.apply(down("b", 0, 300));
        assert_eq!(after.bytes_total, Some(400));
        assert_eq!(after.percent, Some(50));
        assert_eq!(tracker.apply(down("b", 300, 300)).percent, Some(87));
        // Une valeur qui recule est du bruit.
        assert_eq!(tracker.apply(down("b", 10, 300)).bytes_done, 350);
    }

    #[test]
    fn it_stops_at_ninety_nine_until_the_engine_has_finished() {
        let mut tracker = PullTracker::new();
        let progress = tracker.apply(down("a", 100, 100));
        assert_eq!((progress.percent, progress.bytes_done), (Some(99), 100));
        let progress = tracker.apply(LayerEvent::Done { layer: "a".into() });
        assert_eq!(progress.percent, Some(99));
    }

    #[test]
    fn a_finished_layer_counts_as_fully_received() {
        let mut tracker = PullTracker::new();
        tracker.apply(down("a", 10, 100));
        let progress = tracker.apply(LayerEvent::Done { layer: "a".into() });
        assert_eq!(progress.bytes_done, 100);
    }

    #[test]
    fn the_number_of_tracked_layers_is_bounded() {
        let mut tracker = PullTracker::new();
        for i in 0..MAX_LAYERS + 50 {
            tracker.apply(down(&format!("l{i}"), 1, 2));
        }
        assert_eq!(tracker.progress().bytes_total, Some(2 * MAX_LAYERS as u64));
    }
}
