//! La phase d'un serveur de jeu installé depuis une fiche : « Premier démarrage », « Démarrage », « Prêt » ou
//! « Hearth ne peut pas savoir si le jeu est prêt » (HRT-101, BR-FICHE-028, BR-FICHE-056).
//!
//! Les phases vivent **en mémoire** : jamais en base, jamais dans la description. Un redémarrage de l'agent les
//! oublie, et le service retombe sur « Hearth ne peut pas savoir » : c'est vrai (l'agent n'a pas vu le signe).
//!
//! **État du branchement (HRT-101)** : `begin` est appelé au démarrage d'une installation (`Deployments`). `refresh`
//! (vérification périodique), la lecture de la phase par le client (champ `game` de `ServiceItem`) et `forget` (à la
//! suppression d'un service créé par Hearth, HRT-85) n'ont pas encore d'appelant : aucune route de suppression ni
//! projection dans la liste n'existe dans `main`. Suivi : HRT-197.
//!
//! **Limite** : le moteur ne filtre `since` qu'à la SECONDE ; la vérification suivante relit donc les lignes de
//! la dernière seconde déjà lue (sans effet : le signe n'y était pas, et `ready_since` ne compte qu'une fois).
//!
//! Le signe « prêt » est cherché dans les seules lignes du journal du jeu écrites à partir du lancement
//! (`domain::catalog::readiness`), comme une sous-chaîne littérale. Le texte d'une ligne est du contenu sensible :
//! il n'est ni tracé, ni gardé, ni mis dans une erreur.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use futures_util::StreamExt;
use time::OffsetDateTime;

use super::ports::{Clock, ContainerEngine, LogQuery};
use crate::domain::catalog::readiness::{GamePhase, ReadyRule, phase_of, ready_since};
use crate::domain::services::ServiceKey;

/// Au plus ce nombre de lignes lues par vérification ; la suivante reprend à la dernière ligne lue (incrémental).
const MAX_LINES_PER_CHECK: usize = 20_000;
/// Au plus ce temps par vérification.
const CHECK_BUDGET: Duration = Duration::from_secs(5);

struct Entry {
    container: String,
    rule: ReadyRule,
    first_start: bool,
    /// Le lancement, en millisecondes depuis l'époque.
    started_ms: i64,
    ready_ms: Option<i64>,
    /// L'heure de la dernière ligne lue : la vérification suivante reprend d'ici, jamais du lancement (un jeu
    /// bavard ne fait pas relire tout son journal à chaque fois).
    last_read_ms: i64,
}

pub struct GamePhases {
    engine: Arc<dyn ContainerEngine>,
    clock: Arc<dyn Clock>,
    /// Le temps qu'une vérification peut prendre (5 s en production).
    budget: Duration,
    entries: Mutex<HashMap<String, Entry>>,
}

fn now_ms(at: OffsetDateTime) -> i64 {
    i64::try_from(at.unix_timestamp_nanos() / 1_000_000).unwrap_or(i64::MAX)
}

fn at_ms(ms: i64) -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp_nanos(i128::from(ms) * 1_000_000)
        .unwrap_or(OffsetDateTime::UNIX_EPOCH)
}

impl GamePhases {
    pub fn new(engine: Arc<dyn ContainerEngine>, clock: Arc<dyn Clock>) -> Arc<Self> {
        Arc::new(Self {
            engine,
            clock,
            budget: CHECK_BUDGET,
            entries: Mutex::new(HashMap::new()),
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Entry>> {
        self.entries.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Le jeu vient d'être lancé : ses phases commencent à cet instant. `first_start` : c'est son premier
    /// démarrage (celui de l'installation).
    pub fn begin(&self, key: &ServiceKey, container: &str, rule: ReadyRule, first_start: bool) {
        let started_ms = now_ms(self.clock.now());
        self.lock().insert(
            key.as_str().to_owned(),
            Entry {
                container: container.to_owned(),
                rule,
                first_start,
                started_ms,
                ready_ms: None,
                last_read_ms: started_ms,
            },
        );
    }

    /// Le service n'existe plus (ou n'est plus suivi).
    pub fn forget(&self, key: &ServiceKey) {
        self.lock().remove(key.as_str());
    }

    /// La phase d'après ce que l'agent a déjà vu, sans lire le moindre journal. `None` : service non suivi (pas un
    /// serveur de jeu installé depuis une fiche, ou l'agent a redémarré depuis).
    pub fn phase_of(&self, key: &ServiceKey) -> Option<GamePhase> {
        let inner = self.lock();
        let entry = inner.get(key.as_str())?;
        Some(self.phase_for(entry))
    }

    fn phase_for(&self, entry: &Entry) -> GamePhase {
        let elapsed_s = u64::try_from(
            now_ms(self.clock.now())
                .saturating_sub(entry.started_ms)
                .max(0)
                / 1000,
        )
        .unwrap_or(0);
        phase_of(
            &entry.rule,
            entry.first_start,
            elapsed_s,
            entry.ready_ms.map(at_ms),
        )
    }

    /// Cherche le signe « prêt » dans les lignes écrites depuis le lancement, puis rend la phase. Une lecture qui
    /// échoue (moteur muet, journal illisible) ne change rien : la phase reste ce qu'elle était.
    pub async fn refresh(&self, key: &ServiceKey) -> Option<GamePhase> {
        let (container, sign, started_ms, from_ms) = {
            let inner = self.lock();
            let entry = inner.get(key.as_str())?;
            match (&entry.rule.sign, entry.ready_ms) {
                (Some(sign), None) => (
                    entry.container.clone(),
                    sign.clone(),
                    entry.started_ms,
                    entry.last_read_ms,
                ),
                _ => return Some(self.phase_for(entry)),
            }
        };
        let mut stream = self.engine.logs(
            &container,
            LogQuery {
                follow: false,
                tail: None,
                since_ms: Some(from_ms),
                tty: false,
            },
        );
        // L'avancement vit HORS du futur borné par le délai : si le délai coupe la lecture, ce qui a été lu avant
        // reste acquis et la vérification suivante repart plus loin.
        let mut found: Option<i64> = None;
        let mut last = from_ms;
        let _ = tokio::time::timeout(self.budget, async {
            let mut read = 0_usize;
            while let Some(item) = stream.next().await {
                let Ok(line) = item else { break };
                if let Some(at) = ready_since(
                    &sign,
                    started_ms,
                    std::iter::once((line.at_ms, line.text.as_str())),
                ) {
                    found = Some(at);
                    last = last.max(line.at_ms);
                    break;
                }
                last = last.max(line.at_ms);
                read += 1;
                if read >= MAX_LINES_PER_CHECK {
                    break;
                }
            }
        })
        .await;
        drop(stream);
        let mut inner = self.lock();
        let entry = inner.get_mut(key.as_str())?;
        entry.last_read_ms = entry.last_read_ms.max(last);
        if let Some(at) = found {
            entry.ready_ms = Some(at);
        }
        Some(self.phase_for(entry))
    }
}
