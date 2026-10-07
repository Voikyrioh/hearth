//! Le mode attaque : son état, son activation et sa désactivation, sa sortie automatique, sa fenêtre
//! de redémarrage (HRT-25, ADR-0025, BR-TRUST-012, 018 à 021, 027, 030 à 032).
//!
//! **Ce service ne décide pas qui passe** : la règle « 2 critères sur 3 » est pure
//! (`domain::trust::recognition`) et se branche dans `SessionService`. Il tient l'état du mode (une
//! ligne unique en base, relue à chaque décision pour que la sous-commande `attack-mode off`, lancée
//! dans un autre processus, soit prise en compte tout de suite), et ce qui en dépend dans le temps.
//!
//! Aucune horloge murale dans une durée de 30 minutes : la sortie automatique se mesure sur l'horloge
//! monotone de l'agent, la fenêtre et la garde de réactivation sur le temps écoulé depuis le démarrage
//! du noyau (`BootInfo`). Le minuteur de sortie automatique ne vit qu'en mémoire : un redémarrage du
//! service le remet à zéro (le mode dure alors plus longtemps, jamais moins).

use std::sync::{Arc, Mutex, PoisonError};

use time::{Duration, OffsetDateTime};

use super::audit::{AuditTrail, Pending};
use super::ports::{
    AttackModeRepo, BootInfo, Clock, IdGen, MonotonicClock, SecurityFeed, Store, StoreError,
};
use crate::domain::audit::{Actor, AuditAction, AuditEvent, Outcome, Target};
use crate::domain::trust::attack_mode::{
    Activation, Boot, Effective, EndHow, Stored, effective, on_start, plan_activation,
    quiet_elapsed, window_over,
};

/// L'état du mode attaque à un instant, tel que le reste de l'agent le lit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttackStatus {
    pub state: Effective,
    /// Début de l'activation en cours (le mode n'est pas éteint).
    pub since: Option<OffsetDateTime>,
    /// Comment la dernière activation s'est terminée (le mode est éteint).
    pub last_end: Option<EndHow>,
    pub activation_id: Option<String>,
}

impl AttackStatus {
    /// Le mode est éteint, et n'a jamais fini d'une manière que l'on sache dire.
    pub fn off() -> Self {
        Self {
            state: Effective::Off,
            since: None,
            last_end: None,
            activation_id: None,
        }
    }

    fn of(stored: &Stored, state: Effective) -> Self {
        Self {
            state,
            since: stored.activated_at.filter(|_| stored.active),
            last_end: stored.ended_how.filter(|_| !stored.active),
            activation_id: stored.activation_id.clone(),
        }
    }
}

/// Ce que `sweep` a fait.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SweepReport {
    /// La fenêtre de redémarrage est finie : le mode a repris.
    pub resumed: bool,
    /// Le mode s'est arrêté tout seul.
    pub auto_disabled: bool,
}

/// Le minuteur de la sortie automatique : l'instant monotone de la dernière tentative refusée.
struct Quiet {
    /// Le mode est actif (hors fenêtre) depuis qu'on le surveille.
    armed: bool,
    last: Duration,
}

/// Ce qui a été publié au flux la dernière fois : un changement fait ailleurs (la sous-commande) est
/// vu en comparant.
type Signature = (bool, Option<String>, u8, Option<EndHow>);

pub struct AttackModeService {
    repo: Arc<dyn AttackModeRepo>,
    store: Arc<dyn Store>,
    clock: Arc<dyn Clock>,
    monotonic: Arc<dyn MonotonicClock>,
    boot: Arc<dyn BootInfo>,
    ids: Arc<dyn IdGen>,
    trail: Arc<AuditTrail>,
    feed: Arc<dyn SecurityFeed>,
    quiet: Mutex<Quiet>,
    published: Mutex<Option<Signature>>,
}

impl AttackModeService {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        repo: Arc<dyn AttackModeRepo>,
        store: Arc<dyn Store>,
        clock: Arc<dyn Clock>,
        monotonic: Arc<dyn MonotonicClock>,
        boot: Arc<dyn BootInfo>,
        ids: Arc<dyn IdGen>,
        trail: Arc<AuditTrail>,
        feed: Arc<dyn SecurityFeed>,
    ) -> Self {
        let now = monotonic.elapsed();
        Self {
            repo,
            store,
            clock,
            monotonic,
            boot,
            ids,
            trail,
            feed,
            quiet: Mutex::new(Quiet {
                armed: false,
                last: now,
            }),
            published: Mutex::new(None),
        }
    }

    /// Le démarrage en cours, tel que le noyau le dit.
    pub fn boot_now(&self) -> Boot {
        Boot {
            id: self.boot.boot_id(),
            uptime: self.boot.uptime(),
        }
    }

    /// L'état effectif d'une ligne déjà lue (dans une transaction ou non). Le noyau n'est lu que si le
    /// mode est actif : éteint, aucune lecture de plus.
    pub fn effective_of(&self, stored: &Stored) -> Effective {
        if stored.active {
            effective(stored, &self.boot_now())
        } else {
            Effective::Off
        }
    }

    /// L'état effectif maintenant (une lecture de la ligne unique).
    pub async fn effective(&self) -> Result<Effective, StoreError> {
        let stored = self.repo.load().await?;
        Ok(self.effective_of(&stored))
    }

    /// L'état complet, pour `GET /security`, le flux et `attack-mode status`.
    pub async fn status(&self) -> Result<AttackStatus, StoreError> {
        let stored = self.repo.load().await?;
        let state = self.effective_of(&stored);
        Ok(AttackStatus::of(&stored, state))
    }

    /// Une tentative a été refusée : la sortie automatique repart de zéro (BR-TRUST-019). Sans effet
    /// tant que le mode n'est pas actif (le minuteur est armé à l'activation et à la reprise).
    pub fn note_refusal(&self) {
        self.quiet
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .last = self.monotonic.elapsed();
    }

    fn arm(&self) {
        let mut quiet = self.quiet.lock().unwrap_or_else(PoisonError::into_inner);
        quiet.armed = true;
        quiet.last = self.monotonic.elapsed();
    }

    fn signature(stored: &Stored, state: Effective) -> Signature {
        let kind = match state {
            Effective::Off => 0,
            Effective::Active => 1,
            Effective::Suspended { .. } => 2,
        };
        (
            stored.active,
            stored.activation_id.clone(),
            kind,
            stored.ended_how,
        )
    }

    /// Dit au flux qu'il y a du nouveau, et retient ce qui a été dit.
    fn announce(&self, stored: &Stored) {
        let state = self.effective_of(stored);
        *self
            .published
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(Self::signature(stored, state));
        self.feed.publish();
    }

    /// Au lancement du service, une fois, avant d'accepter la moindre requête : note l'identifiant de
    /// démarrage et ouvre la fenêtre si la machine vient de redémarrer (BR-TRUST-020, 021). Rend vrai si
    /// la fenêtre vient de s'ouvrir.
    pub async fn on_start(&self) -> Result<bool, StoreError> {
        let boot = self.boot_now();
        if boot.id.is_none() {
            tracing::warn!(
                "identifiant de démarrage du noyau illisible : le mode attaque n'ouvrira jamais de fenêtre de redémarrage"
            );
        }
        let mut tx = self.store.begin().await?;
        let stored = tx.attack_mode().load().await?;
        let (next, opens) = on_start(stored.clone(), &boot);
        if next != stored {
            tx.attack_mode()
                .save_boot(
                    next.last_boot_id.as_deref(),
                    next.window_boot_id.as_deref(),
                    next.remote_reboot_boot_id.as_deref(),
                )
                .await?;
        }
        let mut journal = Pending::default();
        if opens {
            journal
                .record(
                    &mut *tx,
                    self.event(Actor::system(), AuditAction::AttackModeSuspend),
                )
                .await?;
        }
        tx.commit().await?;
        journal.publish(&self.trail);
        if opens {
            tracing::info!(
                "la machine vient de démarrer : mode attaque suspendu 30 minutes (régime d'alerte)"
            );
        }
        self.announce(&next);
        // Le minuteur de la sortie automatique repart de zéro à chaque lancement.
        if effective(&next, &boot) == Effective::Active {
            self.arm();
        }
        Ok(opens)
    }

    fn event(&self, actor: Actor, action: AuditAction) -> AuditEvent {
        AuditEvent::new(
            self.clock.now(),
            actor,
            action,
            Target::None,
            Outcome::Succeeded,
        )
    }

    /// Active ou désactive le mode (BR-TRUST-012, 018) : dans UNE transaction, l'état et l'entrée du
    /// journal. Idempotent : activer un mode actif, désactiver un mode éteint ne change rien et n'écrit
    /// rien. `by` : l'administrateur et son origine, ou la ligne de commande ; `how` : la manière dont
    /// une désactivation est consignée.
    pub async fn change(
        &self,
        active: bool,
        by: &Actor,
        how: EndHow,
    ) -> Result<AttackStatus, StoreError> {
        let mut tx = self.store.begin().await?;
        let stored = tx.attack_mode().load().await?;
        let boot = self.boot_now();
        let now = self.clock.now();
        let mut journal = Pending::default();
        let changed = if active {
            match plan_activation(&stored, &boot, now) {
                Activation::Already => false,
                plan => {
                    if plan == Activation::Fresh {
                        let id = self.ids.new_id();
                        let who = by
                            .account
                            .as_ref()
                            .map_or_else(|| "inconnu".to_owned(), ToString::to_string);
                        tx.attack_mode().activate_fresh(&id, now, &who).await?;
                    } else {
                        tx.attack_mode().reactivate().await?;
                    }
                    journal
                        .record(
                            &mut *tx,
                            self.event(by.clone(), AuditAction::AttackModeEnable),
                        )
                        .await?;
                    true
                }
            }
        } else if stored.active {
            tx.attack_mode()
                .deactivate(now, how, boot.id.as_deref(), uptime_seconds(&boot))
                .await?;
            journal
                .record(
                    &mut *tx,
                    self.event(by.clone(), AuditAction::AttackModeDisable),
                )
                .await?;
            true
        } else {
            false
        };
        let after = if changed {
            tx.attack_mode().load().await?
        } else {
            stored
        };
        tx.commit().await?;
        journal.publish(&self.trail);
        let state = self.effective_of(&after);
        if changed {
            tracing::info!(active, "mode attaque changé");
            if active {
                self.arm();
            }
            self.announce(&after);
        }
        Ok(AttackStatus::of(&after, state))
    }

    /// Désactivation depuis la ligne de commande du serveur (BR-TRUST-027, voie c). Rend vrai si le mode
    /// était actif.
    pub async fn disable_from_cli(&self) -> Result<bool, StoreError> {
        let before = self.repo.load().await?.active;
        self.change(false, &Actor::command_line(), EndHow::Cli)
            .await?;
        Ok(before)
    }

    /// Passage périodique : reprise à la fin de la fenêtre, sortie automatique après 30 minutes sans
    /// tentative refusée, et tout changement fait par un autre processus (la sous-commande) dit au flux.
    pub async fn sweep(&self) -> Result<SweepReport, StoreError> {
        let stored = self.repo.load().await?;
        let boot = self.boot_now();
        let state = effective(&stored, &boot);
        let state = if stored.active { state } else { Effective::Off };
        let mut report = SweepReport::default();
        let mono = self.monotonic.elapsed();

        // Le minuteur : armé tant que le mode est actif hors fenêtre, relâché sinon (une fenêtre ne
        // compte pas comme une période calme).
        {
            let mut quiet = self.quiet.lock().unwrap_or_else(PoisonError::into_inner);
            match state {
                Effective::Active if !quiet.armed => {
                    quiet.armed = true;
                    quiet.last = mono;
                }
                Effective::Active => {}
                Effective::Off | Effective::Suspended { .. } => quiet.armed = false,
            }
        }

        if window_over(&stored, &boot) {
            report.resumed = self.resume(&boot).await?;
        } else if state == Effective::Active {
            let due = {
                let quiet = self.quiet.lock().unwrap_or_else(PoisonError::into_inner);
                quiet.armed && quiet_elapsed(quiet.last, mono)
            };
            if due {
                report.auto_disabled = self.auto_disable().await?;
            }
        }

        // Un changement fait hors de ce processus : la sous-commande `off`.
        if !report.resumed && !report.auto_disabled {
            let current = Self::signature(&stored, state);
            let unseen = {
                let mut published = self
                    .published
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner);
                let unseen = published.as_ref().is_some_and(|last| *last != current);
                *published = Some(current);
                unseen
            };
            if unseen {
                self.feed.publish();
            }
        }
        Ok(report)
    }

    async fn resume(&self, boot: &Boot) -> Result<bool, StoreError> {
        let mut tx = self.store.begin().await?;
        let stored = tx.attack_mode().load().await?;
        if !window_over(&stored, boot) {
            return Ok(false);
        }
        tx.attack_mode().clear_window().await?;
        let mut journal = Pending::default();
        journal
            .record(
                &mut *tx,
                self.event(Actor::system(), AuditAction::AttackModeResume),
            )
            .await?;
        let after = tx.attack_mode().load().await?;
        tx.commit().await?;
        journal.publish(&self.trail);
        tracing::info!("fenêtre de redémarrage finie : le mode attaque reprend");
        // Le mode redevient actif : sa période calme repart de zéro.
        self.arm();
        self.announce(&after);
        Ok(true)
    }

    async fn auto_disable(&self) -> Result<bool, StoreError> {
        let mut tx = self.store.begin().await?;
        let stored = tx.attack_mode().load().await?;
        let boot = self.boot_now();
        // Relu dans la transaction : un autre passage, ou la sous-commande, a pu passer avant.
        if !stored.active || effective(&stored, &boot) != Effective::Active {
            return Ok(false);
        }
        let still_quiet = {
            let quiet = self.quiet.lock().unwrap_or_else(PoisonError::into_inner);
            quiet.armed && quiet_elapsed(quiet.last, self.monotonic.elapsed())
        };
        if !still_quiet {
            return Ok(false);
        }
        tx.attack_mode()
            .deactivate(
                self.clock.now(),
                EndHow::Auto,
                boot.id.as_deref(),
                uptime_seconds(&boot),
            )
            .await?;
        let mut journal = Pending::default();
        journal
            .record(
                &mut *tx,
                self.event(Actor::system(), AuditAction::AttackModeAutoDisable),
            )
            .await?;
        let after = tx.attack_mode().load().await?;
        tx.commit().await?;
        journal.publish(&self.trail);
        tracing::info!("mode attaque arrêté : 30 minutes sans tentative refusée");
        self.quiet
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .armed = false;
        self.announce(&after);
        Ok(true)
    }
}

/// Le temps écoulé depuis le démarrage, en secondes entières, tel qu'il est noté à la fin d'une
/// activation (illisible : rien n'est noté, la garde retombe sur l'horloge murale).
fn uptime_seconds(boot: &Boot) -> Option<u64> {
    boot.id.as_ref()?;
    u64::try_from(boot.uptime.whole_seconds()).ok()
}
