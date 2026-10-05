//! BR-UPDATE-029 : l'ordre d'un retour arrière. **La base d'abord, le binaire ensuite** : un ancien
//! binaire ne se retrouve jamais devant une base déjà migrée par la nouvelle version. La base n'est
//! remise qu'ici, une seule fois, quand l'ancien binaire est remis : une reprise qui trouve déjà la
//! version d'avant en place ne rejoue jamais ce plan (`orphan::Orphan::AlreadyRolledBack`).

/// Une étape du retour arrière.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RollbackStep {
    /// Arrêter le service (rien n'écrit plus dans la base).
    Stop,
    /// Remettre la base d'avant l'échange (copie atomique, puis la copie est retirée).
    RestoreDatabase,
    /// Remettre exactement l'ancien binaire.
    RestoreBinary,
    /// Relancer le service.
    Restart,
}

/// Les étapes, dans l'ordre. Sans copie de la base (aucune base à l'époque, ou déjà remise), la
/// remise de la base n'est pas dans le plan.
pub fn rollback_plan(database_copy_present: bool) -> Vec<RollbackStep> {
    let mut plan = vec![RollbackStep::Stop];
    if database_copy_present {
        plan.push(RollbackStep::RestoreDatabase);
    }
    plan.push(RollbackStep::RestoreBinary);
    plan.push(RollbackStep::Restart);
    plan
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_database_comes_back_before_the_binary() {
        let plan = rollback_plan(true);
        let database = plan
            .iter()
            .position(|s| *s == RollbackStep::RestoreDatabase);
        let binary = plan.iter().position(|s| *s == RollbackStep::RestoreBinary);
        assert!(database < binary && database.is_some());
        assert_eq!(plan.first(), Some(&RollbackStep::Stop));
        assert_eq!(plan.last(), Some(&RollbackStep::Restart));
    }

    #[test]
    fn without_a_copy_the_database_is_left_alone() {
        assert_eq!(
            rollback_plan(false),
            [
                RollbackStep::Stop,
                RollbackStep::RestoreBinary,
                RollbackStep::Restart
            ]
        );
    }
}
