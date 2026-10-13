---
id: BR-ADOPT-022
domaine: ADOPT
titre: Un port ou un nom tenu par un conteneur non adopté est dit sans nommer le conteneur ni son image
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-adopter-un-service.md (BR-ADOPT-022), docs/adr/ADR-0043-par-defaut-hearth-ne-gere-que-ses-services-un-conteneur-hors-hearth-s-adopte.md, HRT-79
maj: 2026-10-13
---

# BR-ADOPT-022 — Un port ou un nom tenu par un conteneur non adopté est dit sans nommer le conteneur ni son image

## Règle
Un port du serveur ou un nom de service tenu par un conteneur non adopté est dit « tenu par un conteneur créé hors de Hearth », sans le nommer et sans son image (message du formulaire : « Port tenu par un conteneur créé hors de Hearth » ; nom : « Ce nom est déjà pris »). Le registre répond à « qui tient ce port » et « qui porte ce nom » par `Holder` : `Service(nom)` pour un service publié (déjà dans la liste), `Unmanaged` sans aucune charge utile pour un conteneur non adopté. Un conteneur arrêté tient encore son port et son nom.

Ce ticket pose la réponse du registre ; les messages du formulaire de déploiement et d'un démarrage refusé arrivent avec ces actes et ne peuvent dire que ce que `Holder` porte.

## Application (code)
- `crates/hearth-agent/src/domain/services/visibility.rs::Holder`
- `crates/hearth-agent/src/application/services.rs::ServicesRegistry::{port_holder, name_holder}`

## Vérification
- `crates/hearth-agent/src/application/services.rs::tests::a_port_or_a_name_held_by_a_container_that_is_not_adopted_is_told_without_naming_it`
- `crates/hearth-agent/src/domain/services/visibility.rs::tests::a_holder_that_is_not_published_carries_nothing_to_leak`

## Cas limites
- Un service en réseau hôte n'a pas de port listé : il n'est pas vu par `port_holder`.

## Règles liées
- BR-ADOPT-001, BR-ADOPT-002

## Historique
- 2026-10-13 — création (HRT-79, ADR-0043 : ce que l'agent publie, moitié agent, première partie).
