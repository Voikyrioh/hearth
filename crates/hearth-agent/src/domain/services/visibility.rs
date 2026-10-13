//! Ce que Hearth publie d'un conteneur, et à qui (HRT-79, ADR-0043, `BR-ADOPT-001`, `BR-ADOPT-002`,
//! `BR-ADOPT-015`, `BR-ADOPT-016`, `BR-ADOPT-017`).
//!
//! Par défaut Hearth ne publie que les services qu'il a créés (clé `h_`) et les services adoptés. Un
//! autre conteneur est compté, jamais détaillé. L'agent continue de LIRE tous les conteneurs (conflits
//! de ports et de noms, total des ressources, candidats à l'adoption) : la décision porte sur ce qui
//! sort de l'agent. La décision se prend ICI, dans [`visibility_of`], et le registre n'appelle cette
//! fonction qu'à un seul endroit (`ServicesRegistry::publish`), par lequel passent l'instantané, les
//! lots de changements et toute réponse qui nomme un service.
//!
//! L'adoption suit l'identité stable du service, jamais son nom (ADR-0037) : la clé `u_<unité>` d'un
//! service lancé par une unité du système, la clé `c_<identifiant>` sinon. Un conteneur adopté sans
//! unité n'est adopté que pour la vie de ce conteneur ([`lapsed`]).

use std::collections::{HashMap, HashSet};

use time::OffsetDateTime;

use super::key::{KeyKind, ServiceKey};
use super::unit::UnitLookup;

/// Qui a adopté un service, et quand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Adoption {
    /// Le compte qui a adopté, tel que l'agent l'a figé.
    pub by: String,
    pub at: OffsetDateTime,
}

/// Ce que Hearth fait d'un service vis-à-vis du client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility<'a> {
    /// Créé par Hearth : publié de droit, avec son contenu.
    Hearth,
    /// Adopté : publié ; sa commande et ses noms de variables sont réservés au droit
    /// ([`may_read_details`]).
    Adopted(&'a Adoption),
    /// Ni l'un ni l'autre : compté, jamais publié.
    Hidden,
}

/// Le seul endroit qui décide qu'un service sort de l'agent. `adopted` ne contient jamais de clé `h_`
/// (la base l'interdit) ; si l'une s'y glissait, `h_` l'emporterait.
pub fn visibility_of<'a>(
    key: &ServiceKey,
    adopted: &'a HashMap<ServiceKey, Adoption>,
) -> Visibility<'a> {
    if key.kind() == KeyKind::Hearth {
        return Visibility::Hearth;
    }
    match adopted.get(key) {
        Some(adoption) => Visibility::Adopted(adoption),
        None => Visibility::Hidden,
    }
}

/// Qui tient un port du serveur ou un nom (`BR-ADOPT-022`) : ce que le formulaire de déploiement et un
/// démarrage refusé peuvent dire. Un conteneur non adopté n'a AUCUNE charge utile : ni son nom, ni son
/// image, ni son état ne peuvent sortir par ce type, il ne les porte pas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Holder {
    /// Un service publié (créé par Hearth ou adopté) : son nom est déjà dans la liste.
    Service(String),
    /// Un conteneur créé hors de Hearth et non adopté : « Port tenu par un conteneur créé hors de
    /// Hearth », sans le nommer.
    Unmanaged,
}

/// `BR-ADOPT-015`, `BR-ADOPT-016` : pour un service adopté, la commande de lancement et les noms de
/// variables sont réservés aux comptes qui ont le droit « gérer les services ». Pour un service créé
/// par Hearth rien ne change : son créateur en a choisi le contenu, tous les comptes le lisent.
pub fn may_read_details(adoption: Option<&Adoption>, may_manage_services: bool) -> bool {
    adoption.is_none() || may_manage_services
}

/// `BR-ADOPT-017` : les adoptions qui tombent parce que leur conteneur n'existe plus. Un conteneur sans
/// unité du système n'est adopté que pour sa vie : si un tiers le recrée, le nouveau est un autre
/// conteneur, non géré, même sous le même nom. Une adoption par unité ne tombe pas avec le conteneur
/// (le système le recrée à chaque démarrage). `live` : les clés des conteneurs que le moteur connaît
/// maintenant. Le trier est inutile : l'ordre n'a pas de sens.
pub fn lapsed(
    adopted: &HashMap<ServiceKey, Adoption>,
    live: &HashSet<ServiceKey>,
) -> Vec<ServiceKey> {
    adopted
        .keys()
        .filter(|key| key.kind() == KeyKind::Container && !live.contains(*key))
        .cloned()
        .collect()
}

/// `BR-ADOPT-017` : l'adoption d'un service lancé par une unité tient tant que l'unité existe pour le
/// système. L'unité a disparu quand le système ne la connaît plus (`NotLoaded` : supprimée, masquée, ou
/// transitoire arrêtée, que systemd décharge) ET qu'aucun conteneur ne la porte. Un simple arrêt d'une
/// unité écrite dans un fichier la laisse `loaded/inactive` : elle existe, l'adoption reste. Une réponse
/// que le système ne donne pas ou une unité qui ne se charge pas (`Unavailable` : erreur, masquée,
/// invalide, délai) ne dit rien : jamais une adoption ne tombe dessus. Une lecture ne suffit pas :
/// voir [`UNIT_GONE_READS`].
pub fn unit_is_gone(lookup: UnitLookup, container_present: bool) -> bool {
    lookup == UnitLookup::NotLoaded && !container_present
}

/// `BR-ADOPT-017` : nombre de lectures « introuvable, sans conteneur » CONSÉCUTIVES (espacées de deux
/// périodes de sonde, 8 s en production) avant qu'une adoption par unité tombe. Une lecture qui dit
/// autre chose remet le compte à zéro.
pub const UNIT_GONE_READS: u32 = 3;

/// `BR-ADOPT-017` : `consecutive` lectures « disparue » de suite suffisent-elles à faire tomber ?
pub fn gone_long_enough(consecutive: u32) -> bool {
    consecutive >= UNIT_GONE_READS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adoption() -> Adoption {
        Adoption {
            by: "marie".to_owned(),
            at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    fn key(text: &str) -> ServiceKey {
        ServiceKey::parse(text).expect("clé valide")
    }

    #[test]
    fn a_holder_that_is_not_published_carries_nothing_to_leak() {
        let text = format!("{:?}", Holder::Unmanaged);
        assert_eq!(text, "Unmanaged");
        assert_eq!(
            Holder::Service("web".to_owned()),
            Holder::Service("web".to_owned())
        );
    }

    #[test]
    fn a_service_made_by_hearth_is_published_without_being_adopted() {
        let hearth = key("h_01ARZ3NDEKTSV4RRFFQ69G5FAV");
        assert_eq!(visibility_of(&hearth, &HashMap::new()), Visibility::Hearth);
    }

    #[test]
    fn a_container_is_hidden_until_it_is_adopted_by_its_key() {
        let mut adopted = HashMap::new();
        let container = key("c_abcdef0123456789");
        let unit = key("u_docker-terraria.service");
        assert_eq!(visibility_of(&container, &adopted), Visibility::Hidden);
        assert_eq!(visibility_of(&unit, &adopted), Visibility::Hidden);
        adopted.insert(unit.clone(), adoption());
        assert_eq!(
            visibility_of(&unit, &adopted),
            Visibility::Adopted(&adoption())
        );
        // Adopter l'un n'adopte pas l'autre : l'adoption est par clé, jamais par nom.
        assert_eq!(visibility_of(&container, &adopted), Visibility::Hidden);
        // Un autre conteneur sous la même unité n'existe pas : la clé est celle de l'unité.
        assert_eq!(
            visibility_of(&key("u_docker-7dtd.service"), &adopted),
            Visibility::Hidden
        );
    }

    #[test]
    fn details_of_an_adopted_service_are_for_the_right_only_and_those_of_a_hearth_service_for_all()
    {
        let adopted = adoption();
        assert!(may_read_details(Some(&adopted), true));
        assert!(!may_read_details(Some(&adopted), false));
        assert!(may_read_details(None, false));
        assert!(may_read_details(None, true));
    }

    #[test]
    fn an_adoption_by_unit_falls_only_when_the_system_no_longer_knows_the_unit() {
        use crate::domain::services::unit::{UnitActivity, UnitStatus};
        let stopped_file_unit = UnitLookup::Loaded(UnitStatus {
            activity: UnitActivity::Inactive,
            auto_restart: false,
            restarts: false,
            timeout_stop_s: None,
        });
        // Un simple arrêt : l'unité est chargée et inactive, elle existe.
        assert!(!unit_is_gone(stopped_file_unit, false));
        assert!(!unit_is_gone(stopped_file_unit, true));
        // Supprimée, masquée ou transitoire arrêtée : le système ne la connaît plus.
        assert!(unit_is_gone(UnitLookup::NotLoaded, false));
        // Un conteneur la porte encore : elle existe pour lui.
        assert!(!unit_is_gone(UnitLookup::NotLoaded, true));
        // Le système ne répond pas, ou l'unité ne se charge pas : on ne sait rien, rien ne tombe.
        assert!(!unit_is_gone(UnitLookup::Unavailable, false));
    }

    #[test]
    fn a_single_read_never_ends_an_adoption() {
        assert!(!gone_long_enough(0));
        assert!(!gone_long_enough(1));
        assert!(!gone_long_enough(UNIT_GONE_READS - 1));
        assert!(gone_long_enough(UNIT_GONE_READS));
        const { assert!(UNIT_GONE_READS >= 3) };
    }

    #[test]
    fn an_adoption_without_a_unit_lapses_with_its_container_and_one_by_unit_never_does() {
        let mut adopted = HashMap::new();
        let gone = key("c_aaaa");
        let kept = key("c_bbbb");
        let unit = key("u_docker-terraria.service");
        for each in [&gone, &kept, &unit] {
            adopted.insert(each.clone(), adoption());
        }
        let live: HashSet<ServiceKey> = [kept.clone()].into_iter().collect();
        assert_eq!(lapsed(&adopted, &live), vec![gone]);
        // Plus aucun conteneur : l'adoption par unité reste (le système le recrée).
        let mut none = lapsed(&adopted, &HashSet::new());
        none.sort();
        assert_eq!(none, vec![key("c_aaaa"), key("c_bbbb")]);
    }
}
