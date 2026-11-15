//! Les petites règles d'une demande de sauvegarde (BR-BACKUP-006, BR-BACKUP-007, BR-BACKUP-012, BR-BACKUP-003) :
//! quels états se sauvegardent, quels repères porte une sauvegarde, ce qu'est une note valable et une liste
//! d'éléments ignorés valable. Fonctions pures, sans E/S.

use hearth_proto::api::backups::{BackupFlag, IgnoredItem, MAX_ITEMS, NOTE_MAX_CHARS};

use crate::domain::files::RelPath;
use crate::domain::services::ServiceStateKind;

/// Le service tourne-t-il (BR-BACKUP-006) ? Un service en marche, ou qui redémarre en boucle, demande un choix de
/// mode ; tout autre état (ou un service sans conteneur) se copie directement.
pub fn is_running(state: Option<ServiceStateKind>) -> bool {
    matches!(
        state,
        Some(ServiceStateKind::Running | ServiceStateKind::RestartLoop)
    )
}

/// Les états d'un service que l'on sauvegarde : en marche, arrêté, en erreur, ou qui redémarre en boucle. Un
/// déploiement en cours, un arrêt ou un démarrage en cours, une pause, un état inconnu : non.
pub fn backable(state: ServiceStateKind) -> bool {
    matches!(
        state,
        ServiceStateKind::Running
            | ServiceStateKind::Stopped
            | ServiceStateKind::Error
            | ServiceStateKind::RestartLoop
    )
}

/// Les repères d'une sauvegarde (BR-BACKUP-007, BR-BACKUP-008, BR-BACKUP-012) : « À chaud » pour une copie à chaud ;
/// « Arrêt forcé avant la copie » ; « Peut être incohérente » dès qu'un fichier a changé pendant la copie (quel
/// que soit le mode : un service arrêté que le moteur relance, ou un autre écrivain, change aussi des fichiers) et
/// après un arrêt forcé.
pub fn flags_of(hot: bool, forced_stop: bool, changed: u32) -> Vec<BackupFlag> {
    let mut flags = Vec::new();
    if hot {
        flags.push(BackupFlag::Hot);
    }
    if forced_stop {
        flags.push(BackupFlag::ForcedStop);
    }
    if changed > 0 || forced_stop {
        flags.push(BackupFlag::MaybeInconsistent);
    }
    flags
}

/// Pourquoi une note est refusée.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteError {
    TooLong,
    Invalid,
}

/// La note : 80 caractères au plus, sans caractère de contrôle, rognée ; vide : pas de note (BR-BACKUP-012).
pub fn clean_note(note: Option<&str>) -> Result<Option<String>, NoteError> {
    let Some(note) = note.map(str::trim).filter(|note| !note.is_empty()) else {
        return Ok(None);
    };
    if note.chars().count() > NOTE_MAX_CHARS {
        return Err(NoteError::TooLong);
    }
    if note.chars().any(char::is_control) {
        return Err(NoteError::Invalid);
    }
    Ok(Some(note.to_owned()))
}

/// Pourquoi une liste d'éléments ignorés est refusée.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemsError {
    TooMany,
    Invalid,
}

/// Une liste d'éléments ignorés : bornée, chaque chemin relatif et sûr (BR-BACKUP-003).
pub fn validate_items(items: &[IgnoredItem]) -> Result<(), ItemsError> {
    if items.len() > MAX_ITEMS {
        return Err(ItemsError::TooMany);
    }
    for item in items {
        let root_ok = !item.root.is_empty()
            && item.root.len() <= 4096
            && !item.root.chars().any(char::is_control);
        let path_ok = item.path.is_empty() || RelPath::parse(&item.path).is_ok();
        if !root_ok || !path_ok {
            return Err(ItemsError::Invalid);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_note_is_trimmed_bounded_and_free_of_control_characters() {
        assert_eq!(clean_note(None), Ok(None));
        assert_eq!(clean_note(Some("   ")), Ok(None));
        assert_eq!(
            clean_note(Some("  Avant mise à jour 1.4 ")),
            Ok(Some("Avant mise à jour 1.4".to_owned()))
        );
        assert_eq!(clean_note(Some(&"x".repeat(81))), Err(NoteError::TooLong));
        assert!(clean_note(Some(&"x".repeat(80))).is_ok());
        assert_eq!(clean_note(Some("a\nb")), Err(NoteError::Invalid));
    }

    #[test]
    fn the_ignored_lists_are_bounded_and_their_paths_stay_relative() {
        let item = |root: &str, path: &str| IgnoredItem {
            root: root.into(),
            path: path.into(),
        };
        assert_eq!(
            validate_items(&[item("/data", ""), item("/data", "logs/old")]),
            Ok(())
        );
        for bad in [
            item("", "a"),
            item("/data", "../etc"),
            item("/data", "/abs"),
            item("/data", "a//b"),
            item("/da\nta", "a"),
        ] {
            assert_eq!(validate_items(&[bad]), Err(ItemsError::Invalid));
        }
        let many = vec![item("/data", "a"); MAX_ITEMS + 1];
        assert_eq!(validate_items(&many), Err(ItemsError::TooMany));
    }

    #[test]
    fn only_running_stopped_failed_and_looping_services_are_backed_up() {
        for state in [
            ServiceStateKind::Running,
            ServiceStateKind::Stopped,
            ServiceStateKind::Error,
            ServiceStateKind::RestartLoop,
        ] {
            assert!(backable(state), "{state:?}");
        }
        for state in [
            ServiceStateKind::Deploying,
            ServiceStateKind::Stopping,
            ServiceStateKind::Starting,
            ServiceStateKind::Paused,
            ServiceStateKind::Unknown,
        ] {
            assert!(!backable(state), "{state:?}");
        }
    }

    #[test]
    fn a_service_without_a_container_or_not_running_is_not_running() {
        assert!(is_running(Some(ServiceStateKind::Running)));
        assert!(is_running(Some(ServiceStateKind::RestartLoop)));
        assert!(!is_running(Some(ServiceStateKind::Stopped)));
        assert!(!is_running(None));
    }

    #[test]
    fn any_changed_file_or_a_forced_stop_may_make_a_backup_inconsistent() {
        assert_eq!(flags_of(true, false, 0), [BackupFlag::Hot]);
        assert_eq!(
            flags_of(true, false, 2),
            [BackupFlag::Hot, BackupFlag::MaybeInconsistent]
        );
        assert_eq!(
            flags_of(false, true, 0),
            [BackupFlag::ForcedStop, BackupFlag::MaybeInconsistent]
        );
        // Un service arrêté dont un fichier bouge quand même (le moteur le relance, un autre écrivain).
        assert_eq!(flags_of(false, false, 5), [BackupFlag::MaybeInconsistent]);
        assert!(flags_of(false, false, 0).is_empty());
    }
}
