---
id: BR-SLOG-009
domaine: SLOG
titre: Quand un service redémarre, ses nouvelles lignes arrivent à la suite, précédées d'un repère « redémarré » ; la lecture suit la clé du service, pas le conteneur
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-journaux-service.md (BR-SLOG-009), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §7, §9, §10, HRT-66
maj: 2026-10-13
---

# BR-SLOG-009 — Quand un service redémarre, ses nouvelles lignes arrivent à la suite, précédées d'un repère « redémarré » ; la lecture suit la clé du service, pas le conteneur

## Règle
La lecture suit la CLÉ du service (`u_...`, `c_...`), jamais l'identifiant d'un conteneur : un service lancé par une unité systemd change de conteneur à chaque démarrage (BR-SVC-016). Quand l'agent voit le même conteneur démarré de nouveau, ou un autre conteneur sous la même clé, il envoie `marker { kind: "restarted", at }` puis lit la nouvelle exécution : un autre conteneur, repère d'abord puis ses lignes depuis son début ; le même conteneur, à la suite de la dernière ligne envoyée sans doublon, le repère posé juste avant la première ligne écrite à partir du nouveau démarrage (les dernières lignes de l'exécution précédente que le client n'a pas encore le précèdent). Quand le flux d'un conteneur qui tournait se ferme, l'agent attend (2 s au plus) que le registre publie du nouveau sur ce service avant de relire : le registre regroupe les événements du moteur sur 250 ms, relire plus tôt ferait lire une nouvelle exécution sans repère de redémarrage. Les lignes d'un conteneur supprimé ne reviennent pas. Le séparateur « Le service a redémarré à {heure} » est un repère d'écran du client.

## Application (code)
- `crates/hearth-agent/src/application/service_logs.rs::Reader::{plan, wait_registry_moves}`
- `crates/hearth-agent/src/domain/services/logs.rs::{LogContainer::relation_to, Relation}`

## Vérification
- `crates/hearth-agent/tests/service_logs_ws.rs::a_service_that_starts_again_continues_after_a_restart_marker_without_repeating_lines`
- `crates/hearth-agent/tests/service_logs_ws.rs::a_service_run_by_a_unit_is_followed_across_its_containers_by_its_key`
- `crates/hearth-agent/tests/service_logs_ws.rs::a_resume_across_a_restart_puts_the_marker_between_the_two_runs`
- `crates/hearth-agent/tests/service_logs_ws.rs::a_restart_the_registry_has_not_published_yet_still_gets_its_marker`
- `crates/hearth-agent/src/domain/services/logs.rs::tests::a_container_is_new_restarted_or_the_same_from_the_cursor`

## Règles liées
- BR-SLOG-008, BR-SLOG-014, BR-SVC-016

## Historique
- 2026-10-13 — création (HRT-66, tranche 4 des déploiements : journaux en direct, côté agent et protocole).
- 2026-10-13 — corrections de la review locale de HRT-66 : le repère n'est plus perdu quand le conteneur repart avant que le registre ait publié son nouveau démarrage.
