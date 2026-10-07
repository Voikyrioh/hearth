//! Sous-commande `attack-mode status|off` : accès direct à la base du serveur, sans réseau
//! (HRT-25, BR-TRUST-027 voie c). La voie de secours d'un administrateur qui ne peut pas prouver de
//! clé depuis le client ; le redémarrage physique de la machine est l'autre.
//!
//! Il n'y a pas de sous-commande pour **activer** : activer est un acte d'administration qui exige un
//! poste avec sa clé (Q14 point 3).

use std::io::{self, Write};

use super::cli::AttackModeAction;
use crate::application::attack_mode::{AttackModeService, AttackStatus};
use crate::application::ports::StoreError;
use crate::domain::trust::attack_mode::Effective;

pub const MSG_OFF: &str = "Le mode attaque est désactivé.";
pub const MSG_WAS_OFF: &str = "Le mode attaque n'était pas actif.";

#[derive(Debug, thiserror::Error)]
pub enum AttackModeCliError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("Écriture du résultat impossible : {0}")]
    Output(#[from] io::Error),
}

pub async fn execute(
    action: &AttackModeAction,
    service: &AttackModeService,
    out: &mut dyn Write,
) -> Result<(), AttackModeCliError> {
    match action {
        AttackModeAction::Status => {
            let status = service.status().await?;
            write_status(out, &status)?;
        }
        AttackModeAction::Off => {
            let was_active = service.disable_from_cli().await?;
            writeln!(out, "{}", if was_active { MSG_OFF } else { MSG_WAS_OFF })?;
        }
    }
    Ok(())
}

/// L'état, en lignes `clé : valeur` stables (un script les lit) après une phrase.
pub fn write_status(out: &mut dyn Write, status: &AttackStatus) -> io::Result<()> {
    let (sentence, mode) = match status.state {
        Effective::Off => ("Le mode attaque est inactif.", "off"),
        Effective::Active => ("Le mode attaque est actif.", "active"),
        Effective::Suspended { .. } => (
            "Le mode attaque est suspendu (la machine vient de démarrer) : le régime d'alerte s'applique.",
            "suspended",
        ),
    };
    writeln!(out, "{sentence}")?;
    writeln!(out, "mode : {mode}")?;
    if let Some(id) = &status.activation_id {
        writeln!(out, "activation_id : {id}")?;
    }
    if let Effective::Suspended { remaining } = status.state {
        let seconds = remaining.whole_seconds() + i64::from(remaining.subsec_nanoseconds() > 0);
        writeln!(out, "resumes_in_s : {seconds}")?;
    }
    Ok(())
}
