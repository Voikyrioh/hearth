---
id: BR-SVC-016
domaine: SVC
titre: Un service a une identité stable (sa clé), indépendante de son conteneur ; un service supervisé relancé reste le même
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-voir-les-services.md (BR-SVC-016), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §0, §6, §9, HRT-63
maj: 2026-10-13
---

# BR-SVC-016 — Identité stable d'un service

## Règle
Le détail d'un service suit sa CLÉ (`ServiceKey`), jamais son nom ni l'identifiant de son conteneur :

- `u_<unité>` pour un conteneur lancé sous une unité de service systemd (le suffixe `.service` fait partie de la clé) : le système recrée le conteneur à chaque démarrage, sous un autre identifiant ; c'est le même service. L'agent cherche le propriétaire d'un conteneur une fois pour sa vie (processus client `docker` ou `podman` dont la ligne de commande nomme le conteneur, groupe de contrôle de ce processus) ; deux unités qui revendiquent le même conteneur : pas de propriétaire.
- `c_<identifiant complet>` pour tout autre conteneur : supprimé puis recréé par un tiers sous le même nom, c'est un AUTRE service (le détail ouvert n'est jamais basculé).
- `h_<ULID>` est seulement analysée (tranche 5).

Tant que l'unité existe pour le système, un service supervisé dont le conteneur disparaît RESTE listé (BR-SVC-001), d'après l'état de l'unité (BR-SVC-005), avec ses derniers réglages connus (`supervised_services`, migration `0009`). Quand le système ne connaît plus l'unité (non chargée, introuvable, masquée) et que son conteneur n'existe plus, le service sort de la liste et de la base. Le nom d'unité lu sur le système est VALIDÉ (`UnitName` : jeu de caractères de `systemd.unit(5)`, 255 caractères, suffixe `.service`, ni tiret ni point au début) avant d'entrer dans une clé, un argument de `systemctl` ou la base. Un conteneur sans propriétaire connu (`docker run -d` dont le client a déjà quitté, unité d'une autre nature) garde la clé `c_` : limite assumée (ADR-0037).

## Application (code)
- `crates/hearth-agent/src/domain/services/key.rs::ServiceKey::for_unit`
- `crates/hearth-agent/src/domain/services/unit.rs::{UnitName, unit_of_cgroup, client_refs, is_engine_client}`
- `crates/hearth-agent/src/infrastructure/supervisor/proc_systemd.rs::{ProcSystemd, scan}`
- `crates/hearth-agent/src/application/services.rs::ServicesRegistry::{resolve_owners, build_views, poll_units}`
- `crates/hearth-agent/src/application/ports/service_supervisor.rs::ServiceSupervisor`
- `crates/hearth-agent/src/infrastructure/sqlite/services_repo.rs::SqliteServiceRepo`

## Vérification
- `crates/hearth-agent/src/domain/services/unit.rs::tests::a_unit_name_is_validated_before_it_goes_anywhere`
- `crates/hearth-agent/src/domain/services/unit.rs::tests::the_unit_of_a_cgroup_is_the_first_system_service_component`
- `crates/hearth-agent/src/domain/services/key.rs::tests::a_supervised_service_key_is_u_then_the_unit_and_does_not_depend_on_the_container`
- `crates/hearth-agent/tests/supervisor_proc.rs::a_container_run_by_a_service_unit_is_found_and_its_key_is_u_unit`
- `crates/hearth-agent/tests/supervisor_proc.rs::only_the_containers_a_client_names_get_an_owner`
- `crates/hearth-agent/tests/supervisor_proc.rs::two_units_claiming_the_same_container_leave_it_without_an_owner`
- `crates/hearth-agent/tests/supervisor_proc.rs::a_unit_name_that_is_not_a_unit_name_never_leaves_the_scan`
- `crates/hearth-agent/src/application/services.rs::tests::a_container_under_a_unit_is_the_service_u_unit_whatever_container_the_system_creates`
- `crates/hearth-agent/src/application/services.rs::tests::a_supervised_service_whose_container_vanished_stays_listed_from_its_unit`
- `crates/hearth-agent/src/application/services.rs::tests::a_unit_the_system_forgets_is_dropped_with_its_memory`
- `crates/hearth-agent/src/application/services.rs::tests::the_owner_of_a_container_is_searched_once_for_its_whole_life`
- `crates/hearth-agent/tests/services_https.rs::a_service_run_by_a_unit_is_served_under_its_unit_and_stays_listed_when_its_container_goes`
- `crates/hearth-agent/tests/engine_supervised.rs (étape `supervised` du job `engine`, vrai moteur et vrai systemd, en CI Linux seulement)`

## Règles liées
- BR-SVC-001, BR-SVC-003, BR-SVC-005

## Historique
- 2026-10-13 — création (HRT-63, tranche 2 des déploiements : détail, superviseur, révélation des variables).
