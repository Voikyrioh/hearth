//! Défaire une installation interrompue ou en erreur (BR-INSTALL-008) : la machine revient à
//! l'état d'avant. Seul ce que **cette** exécution a créé est retiré ; ce qui existait avant
//! (comptes, journal, identité, configuration d'une réinstallation) n'est jamais touché, et le
//! binaire remplacé est rétabli.

/// Un élément de l'installation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Asset {
    Binary,
    Config,
    /// Le dossier de données, créé par cette exécution : sa suppression emporte la base,
    /// l'identité et le premier compte qu'il contient.
    DataDir,
    /// Le certificat, la clé et l'identifiant d'installation, créés dans un dossier qui existait.
    Identity,
    /// La base, créée dans un dossier qui existait.
    Database,
    /// Le premier compte administrateur, créé dans une base qui existait.
    FirstAccount,
    Service,
}

/// Ce qu'une étape terminée a fait.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Done {
    /// N'existait pas, créé par cette exécution.
    Created(Asset),
    /// Existait, remplacé par cette exécution (l'ancien est gardé de côté).
    Replaced(Asset),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Undo {
    Remove(Asset),
    /// Rétablit l'ancien : le binaire de côté ; pour le service, le redémarre tel qu'il était.
    Restore(Asset),
}

/// Ce qu'il faut défaire, dans l'ordre inverse de ce qui a été fait. Un élément contenu dans un
/// dossier créé par cette exécution n'est pas retiré une seconde fois : la suppression du dossier
/// l'emporte.
pub fn undo_plan(done: &[Done]) -> Vec<Undo> {
    let data_dir_created = done.contains(&Done::Created(Asset::DataDir));
    done.iter()
        .rev()
        .filter_map(|step| match *step {
            Done::Created(Asset::Identity | Asset::Database | Asset::FirstAccount)
                if data_dir_created =>
            {
                None
            }
            Done::Created(asset) => Some(Undo::Remove(asset)),
            Done::Replaced(asset) => Some(Undo::Restore(asset)),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_done_nothing_to_undo() {
        assert!(undo_plan(&[]).is_empty());
    }

    #[test]
    fn a_fresh_installation_is_undone_in_reverse_order() {
        let done = [
            Done::Created(Asset::Binary),
            Done::Created(Asset::Config),
            Done::Created(Asset::DataDir),
            Done::Created(Asset::Identity),
            Done::Created(Asset::Database),
            Done::Created(Asset::FirstAccount),
            Done::Created(Asset::Service),
        ];
        assert_eq!(
            undo_plan(&done),
            [
                Undo::Remove(Asset::Service),
                Undo::Remove(Asset::DataDir),
                Undo::Remove(Asset::Config),
                Undo::Remove(Asset::Binary),
            ],
            "le dossier de données emporte l'identité, la base et le compte"
        );
    }

    #[test]
    fn a_replaced_binary_is_restored_and_the_old_service_is_brought_back() {
        let done = [
            Done::Replaced(Asset::Binary),
            Done::Replaced(Asset::Service),
        ];
        assert_eq!(
            undo_plan(&done),
            [Undo::Restore(Asset::Service), Undo::Restore(Asset::Binary)]
        );
    }

    #[test]
    fn what_existed_before_is_never_in_the_plan() {
        // Une réparation : le dossier de données, la base et la configuration étaient là ; seuls
        // le binaire et l'unité sont créés par cette exécution.
        let done = [Done::Created(Asset::Binary), Done::Created(Asset::Service)];
        let undo = undo_plan(&done);
        assert_eq!(
            undo,
            [Undo::Remove(Asset::Service), Undo::Remove(Asset::Binary)]
        );
        for asset in [
            Asset::DataDir,
            Asset::Database,
            Asset::Identity,
            Asset::Config,
        ] {
            assert!(!undo.contains(&Undo::Remove(asset)), "{asset:?}");
        }
    }

    #[test]
    fn a_first_account_added_to_an_existing_database_is_removed_on_its_own() {
        let done = [
            Done::Created(Asset::FirstAccount),
            Done::Created(Asset::Service),
        ];
        assert_eq!(
            undo_plan(&done),
            [
                Undo::Remove(Asset::Service),
                Undo::Remove(Asset::FirstAccount)
            ]
        );
    }
}
