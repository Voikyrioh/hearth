use async_trait::async_trait;
use time::OffsetDateTime;

use super::StoreError;
use crate::domain::accounts::AccountId;
use crate::domain::trust::TrialKind;
use crate::domain::trust::attack_mode::{EndHow, Stored};

/// Lecture du mode attaque (HRT-25). Écritures : `UnitOfWork::attack_mode`. La ligne est unique et
/// lue à chaque décision : la sous-commande `attack-mode off`, lancée dans un autre processus, écrit
/// la même base et le service doit s'en apercevoir tout de suite.
#[async_trait]
pub trait AttackModeRepo: Send + Sync {
    async fn load(&self) -> Result<Stored, StoreError>;
}

#[async_trait]
pub trait AttackModeTx: Send {
    /// La ligne unique, lue dans la transaction.
    async fn load(&mut self) -> Result<Stored, StoreError>;

    /// Nouvelle activation : `activation_id` neuf, essais des activations précédentes supprimés,
    /// fenêtre et fin effacées.
    async fn activate_fresh(
        &mut self,
        activation_id: &str,
        at: OffsetDateTime,
        by: &str,
    ) -> Result<(), StoreError>;

    /// Réactivation proche de la fin de la précédente : MÊME identifiant d'activation, essais gardés ;
    /// la fin et la fenêtre sont effacées.
    async fn reactivate(&mut self) -> Result<(), StoreError>;

    /// Désactive : date, manière, et le démarrage du noyau avec son temps écoulé (garde de
    /// réactivation sans horloge murale). La fenêtre est effacée : sortie pendant la fenêtre, le mode
    /// n'est pas rouvert après.
    async fn deactivate(
        &mut self,
        at: OffsetDateTime,
        how: EndHow,
        boot_id: Option<&str>,
        uptime_s: Option<u64>,
    ) -> Result<(), StoreError>;

    /// Au lancement du service : l'identifiant de démarrage vu, la fenêtre, la marque de redémarrage
    /// demandé.
    async fn save_boot(
        &mut self,
        last_boot_id: Option<&str>,
        window_boot_id: Option<&str>,
        remote_reboot_boot_id: Option<&str>,
    ) -> Result<(), StoreError>;

    /// La fenêtre est finie et sa reprise est consignée : la marque est effacée.
    async fn clear_window(&mut self) -> Result<(), StoreError>;

    /// L'essai de ce critère est-il déjà consommé pour cette activation ?
    async fn trial_used(
        &mut self,
        activation_id: &str,
        account: &AccountId,
        kind: TrialKind,
        subject: &str,
    ) -> Result<bool, StoreError>;

    /// Consomme l'essai. Appelé une seule fois par critère, compte et activation : la lecture et
    /// l'écriture sont dans la même transaction `BEGIN IMMEDIATE`, donc deux tentatives simultanées ne
    /// le consomment pas deux fois.
    async fn record_trial(
        &mut self,
        activation_id: &str,
        account: &AccountId,
        kind: TrialKind,
        subject: &str,
        at: OffsetDateTime,
        succeeded: bool,
    ) -> Result<(), StoreError>;

    /// Supprime les essais d'une autre activation que celle de la ligne ; rend leur nombre.
    async fn purge_stale_trials(&mut self) -> Result<u64, StoreError>;
}
