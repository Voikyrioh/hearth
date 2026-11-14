//! Les éléments ignorés par les sauvegardes d'un service (BR-BACKUP-003).
//!
//! Un élément ignoré n'est ni lu, ni archivé : c'est la moitié de la protection des sauvegardes faites hors de
//! Hearth (l'autre moitié est le registre, `ownership`). **La règle de noms n'est qu'une proposition** : tout
//! élément dont le nom contient « backup » ou « sauvegarde » (sans tenir compte de la casse) ou finit par
//! « .bak » est ignoré PAR DÉFAUT, sauf si un compte l'a fait entrer dans les sauvegardes (`included`). Un
//! compte peut aussi ajouter un élément (`chosen`), et une sauvegarde peut en ignorer un de plus pour cette
//! fois (`once`, un gros fichier, BR-BACKUP-033).
//!
//! Les chemins sont ceux du SERVICE : la cible de montage du dossier (`root`), puis un chemin relatif à ce
//! dossier, `/` pour séparateur (vide : le dossier entier).

use hearth_proto::api::backups::{IgnoredItem, IgnoredWhy};

/// Le nom ressemble-t-il à des sauvegardes faites hors de Hearth ? Une PROPOSITION, jamais une preuve.
pub fn name_looks_like_backups(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.contains("backup") || lower.contains("sauvegarde") || lower.ends_with(".bak")
}

/// Le chemin `path` est-il `item` ou dessous ? `item` vide : tout le dossier. Comparaison par composants.
fn covers(item: &str, path: &str) -> bool {
    if item.is_empty() {
        return true;
    }
    path == item
        || path
            .strip_prefix(item)
            .is_some_and(|rest| rest.starts_with('/'))
}

/// Les trois listes qui décident d'un élément.
#[derive(Debug, Clone, Default)]
pub struct IgnoreRules {
    /// Ajoutés par un compte (réglage du service).
    pub chosen: Vec<IgnoredItem>,
    /// Les ignorés par défaut que le compte a fait entrer dans les sauvegardes.
    pub included: Vec<IgnoredItem>,
    /// Ignorés pour cette sauvegarde seulement.
    pub once: Vec<IgnoredItem>,
}

impl IgnoreRules {
    fn listed(list: &[IgnoredItem], root: &str, path: &str) -> bool {
        list.iter()
            .any(|item| item.root == root && covers(&item.path, path))
    }

    /// Un dossier monté entier est-il ignoré ? `target` : où il est monté dans le service. Son dernier
    /// composant répond à la règle de noms, sauf retrait par le compte ; ou un compte l'a ajouté (chemin vide).
    pub fn root_ignored(&self, target: &str) -> Option<IgnoredWhy> {
        if self
            .once
            .iter()
            .any(|item| item.root == target && item.path.is_empty())
        {
            return Some(IgnoredWhy::ThisTime);
        }
        if self
            .chosen
            .iter()
            .any(|item| item.root == target && item.path.is_empty())
        {
            return Some(IgnoredWhy::Chosen);
        }
        let last = target
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or_default();
        let kept = self
            .included
            .iter()
            .any(|item| item.root == target && item.path.is_empty());
        (name_looks_like_backups(last) && !kept).then_some(IgnoredWhy::LooksLikeBackups)
    }

    /// Un élément d'un dossier est-il ignoré ? `path` : son chemin relatif au dossier (`root`), `name` : son
    /// dernier composant. `None` : il entre dans la sauvegarde.
    pub fn judge(&self, root: &str, path: &str, name: &str) -> Option<IgnoredWhy> {
        if Self::listed(&self.once, root, path) {
            return Some(IgnoredWhy::ThisTime);
        }
        if Self::listed(&self.chosen, root, path) {
            return Some(IgnoredWhy::Chosen);
        }
        if name_looks_like_backups(name) && !Self::listed(&self.included, root, path) {
            return Some(IgnoredWhy::LooksLikeBackups);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(root: &str, path: &str) -> IgnoredItem {
        IgnoredItem {
            root: root.into(),
            path: path.into(),
        }
    }

    #[test]
    fn the_name_rule_is_case_blind_and_catches_the_three_shapes() {
        for name in [
            "backups",
            "Backup",
            "world-BACKUP-2026",
            "sauvegardes",
            "Ma_Sauvegarde_1",
            "Forge.wld.bak",
            "x.BAK",
        ] {
            assert!(name_looks_like_backups(name), "{name}");
        }
        for name in [
            "world",
            "saves_old",
            "Forge.wld.2026-09-02-120000",
            "bakery",
        ] {
            assert!(!name_looks_like_backups(name), "{name}");
        }
    }

    #[test]
    fn an_element_with_a_backup_name_is_ignored_by_default_and_a_plain_one_is_not() {
        let rules = IgnoreRules::default();
        assert_eq!(
            rules.judge("/data", "backups", "backups"),
            Some(IgnoredWhy::LooksLikeBackups)
        );
        assert_eq!(rules.judge("/data", "world", "world"), None);
        // `saves_old` : un nom se rate, la protection ne repose pas sur lui (le registre, `ownership`).
        assert_eq!(rules.judge("/data", "saves_old", "saves_old"), None);
    }

    #[test]
    fn a_chosen_item_covers_what_is_below_it_and_only_by_whole_components() {
        let rules = IgnoreRules {
            chosen: vec![item("/data", "logs")],
            ..IgnoreRules::default()
        };
        assert_eq!(
            rules.judge("/data", "logs", "logs"),
            Some(IgnoredWhy::Chosen)
        );
        assert_eq!(
            rules.judge("/data", "logs/a/b.txt", "b.txt"),
            Some(IgnoredWhy::Chosen)
        );
        assert_eq!(rules.judge("/data", "logsX", "logsX"), None);
        assert_eq!(rules.judge("/config", "logs", "logs"), None);
    }

    #[test]
    fn removing_a_default_ignored_item_brings_it_back_into_the_backups() {
        let rules = IgnoreRules {
            included: vec![item("/data", "world/backups")],
            ..IgnoreRules::default()
        };
        assert_eq!(rules.judge("/data", "world/backups", "backups"), None);
        // Un autre élément du même nom, ailleurs, reste ignoré.
        assert_eq!(
            rules.judge("/data", "other/backups", "backups"),
            Some(IgnoredWhy::LooksLikeBackups)
        );
    }

    #[test]
    fn an_item_ignored_this_time_wins_and_a_mounted_folder_can_be_ignored_whole() {
        let rules = IgnoreRules {
            once: vec![item("/data", "big.log"), item("/cache", "")],
            ..IgnoreRules::default()
        };
        assert_eq!(
            rules.judge("/data", "big.log", "big.log"),
            Some(IgnoredWhy::ThisTime)
        );
        assert_eq!(rules.root_ignored("/cache"), Some(IgnoredWhy::ThisTime));
        assert_eq!(rules.root_ignored("/data"), None);
    }

    #[test]
    fn a_mounted_folder_named_like_backups_is_ignored_unless_the_account_took_it_back() {
        let rules = IgnoreRules::default();
        assert_eq!(
            rules.root_ignored("/var/terraria-backups"),
            Some(IgnoredWhy::LooksLikeBackups)
        );
        let kept = IgnoreRules {
            included: vec![item("/var/terraria-backups", "")],
            ..IgnoreRules::default()
        };
        assert_eq!(kept.root_ignored("/var/terraria-backups"), None);
    }
}
