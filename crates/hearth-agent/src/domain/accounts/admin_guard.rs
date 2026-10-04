//! Garde du dernier administrateur (BR-ACCT-007).
//!
//! Fonctions pures : l'adaptateur compte les administrateurs dans la même transaction que
//! l'écriture, appelle ces fonctions, puis exécute ou abandonne.

use thiserror::Error;

use super::role::Role;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("Il doit toujours rester au moins un administrateur")]
pub struct LastAdminError;

/// Un compte ne peut pas être supprimé s'il est le seul administrateur.
/// `admin_count` : nombre d'administrateurs avant la suppression, cible comprise.
pub fn check_removal(target_role: Role, admin_count: u64) -> Result<(), LastAdminError> {
    if target_role == Role::Admin && admin_count <= 1 {
        Err(LastAdminError)
    } else {
        Ok(())
    }
}

/// Un compte ne peut pas être rétrogradé s'il est le seul administrateur.
/// `admin_count` : nombre d'administrateurs avant le changement, cible comprise.
pub fn check_role_change(current: Role, new: Role, admin_count: u64) -> Result<(), LastAdminError> {
    if current == Role::Admin && new != Role::Admin && admin_count <= 1 {
        Err(LastAdminError)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_only_administrator_cannot_be_removed() {
        assert_eq!(check_removal(Role::Admin, 1), Err(LastAdminError));
        assert_eq!(check_removal(Role::Admin, 0), Err(LastAdminError));
    }

    #[test]
    fn an_administrator_can_be_removed_when_another_remains() {
        assert_eq!(check_removal(Role::Admin, 2), Ok(()));
        assert_eq!(check_removal(Role::Admin, 5), Ok(()));
    }

    #[test]
    fn a_read_only_account_can_always_be_removed() {
        assert_eq!(check_removal(Role::ReadOnly, 1), Ok(()));
        assert_eq!(check_removal(Role::ReadOnly, 0), Ok(()));
    }

    #[test]
    fn the_only_administrator_cannot_be_demoted() {
        assert_eq!(
            check_role_change(Role::Admin, Role::ReadOnly, 1),
            Err(LastAdminError)
        );
    }

    #[test]
    fn an_administrator_can_be_demoted_when_another_remains() {
        assert_eq!(check_role_change(Role::Admin, Role::ReadOnly, 2), Ok(()));
    }

    #[test]
    fn promotion_and_unchanged_roles_are_never_blocked() {
        assert_eq!(check_role_change(Role::ReadOnly, Role::Admin, 1), Ok(()));
        assert_eq!(check_role_change(Role::ReadOnly, Role::ReadOnly, 1), Ok(()));
        assert_eq!(check_role_change(Role::Admin, Role::Admin, 1), Ok(()));
    }

    #[test]
    fn message_is_the_one_of_the_specification() {
        assert_eq!(
            LastAdminError.to_string(),
            "Il doit toujours rester au moins un administrateur"
        );
    }
}
