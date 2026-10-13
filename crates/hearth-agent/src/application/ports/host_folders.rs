use crate::domain::services::Probe;

/// Ce que l'agent apprend du système de fichiers du serveur pour juger un dossier à monter (HRT-75,
/// BR-DEPLOY-012). Deux questions, aucune ne modifie rien.
pub trait HostFolders: Send + Sync {
    /// Ce que le système dit de ce chemin absolu : lien symbolique (vers quelle cible), simple élément
    /// ou inexistant, ou impossible à savoir.
    fn probe(&self, path: &str) -> Probe;

    /// Les dossiers personnels de tous les comptes du serveur, lus MAINTENANT (un compte ajouté après
    /// le démarrage de l'agent est couvert). Un compte sans dossier personnel est absent du résultat.
    /// `None` : la liste des comptes n'a pas pu être lue ; le dossier est alors refusé (on ne sait pas
    /// quels `.ssh` protéger).
    fn account_homes(&self) -> Option<Vec<String>>;
}
