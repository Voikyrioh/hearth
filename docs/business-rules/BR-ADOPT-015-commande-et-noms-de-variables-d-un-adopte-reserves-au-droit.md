---
id: BR-ADOPT-015
domaine: ADOPT
titre: Pour un service adopté, la commande de lancement et les noms de variables sont réservés aux comptes qui ont le droit de gérer les services
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-adopter-un-service.md (BR-ADOPT-015), docs/adr/ADR-0043-par-defaut-hearth-ne-gere-que-ses-services-un-conteneur-hors-hearth-s-adopte.md, HRT-79
maj: 2026-10-13
---

# BR-ADOPT-015 — Pour un service adopté, la commande de lancement et les noms de variables sont réservés aux comptes qui ont le droit de gérer les services

## Règle
Pour un service adopté, la commande de lancement, les noms de variables, les valeurs de variables et les journaux ne sortent de l'agent que pour les comptes qui ont le droit « gérer les services » (`Access::Services`, quel que soit le rôle). Aux autres, `commande` et `env_names` sont retirés du message et `details_reserved` vaut `true` (un client ne lit pas « aucune commande » pour « commande réservée »). Le reste du détail (repères, état, image, ports, dossiers montés, règle de redémarrage, délai d'arrêt) et le repère `adopted` restent ouverts à tous. Les valeurs ne se lisent que par `GET /services/{key}/env`, réservée au droit. Les journaux (`GET /services/{key}/logs`) sont jugés par `domain::services::logs::may_read` (BR-SLOG-011) : un compte sans le droit reçoit `SERVICE_NOT_FOUND` pour un service adopté, et la lecture en cours se ferme dès que le droit est retiré.

Le contenu est décidé par destinataire, au moment d'écrire le message (`services_wire::Reader`) : `GET /services` d'après le compte de la requête, le flux d'après le droit de la session au dernier contrôle ; quand ce droit change pendant un flux, un instantané complet repart au contrôle suivant de la session.

## Application (code)
- `crates/hearth-agent/src/domain/services/visibility.rs::may_read_details`
- `crates/hearth-agent/src/entrypoint/services_wire.rs::{Reader, service_item}`
- `crates/hearth-agent/src/entrypoint/http/services.rs::list`
- `crates/hearth-agent/src/entrypoint/ws/connection.rs::reader_of`
- `crates/hearth-agent/src/domain/services/logs.rs::may_read`

## Vérification
- `crates/hearth-agent/tests/services_https.rs::the_command_and_variable_names_of_an_adopted_service_are_for_the_right_only_the_rest_is_for_all_and_no_value_is_sent`
- `crates/hearth-agent/src/entrypoint/services_wire.rs::tests::an_adopted_service_gives_its_command_and_variable_names_to_the_right_only`
- `crates/hearth-agent/tests/service_logs_ws.rs::an_adopted_service_is_read_with_the_right_to_manage_services_whatever_the_role`
- `crates/hearth-agent/tests/service_logs_ws.rs::a_right_withdrawn_during_a_reading_closes_it_with_service_not_found`
- `crates/hearth-agent/src/domain/services/visibility.rs::tests::details_of_an_adopted_service_are_for_the_right_only_and_those_of_a_hearth_service_for_all`

## Cas limites
- Un droit retiré pendant un flux ouvert prend effet au contrôle suivant de la session (période de contrôle du flux), pas à la demande suivante comme sur HTTP : jusqu'à ce contrôle, les lots suivants gardent le contenu de l'ancien droit.
- Le client actuel ne lit ni `details_reserved` ni `adopted` : il n'affiche pas la raison de la réserve.

## Règles liées
- BR-ADOPT-016, BR-SVC-007, BR-SVC-008, BR-SVC-009, BR-GRANT-003

## Historique
- 2026-10-13 — HRT-80 : le droit est prouvé AUSSI sur le flux, composé par lecteur (deux comptes abonnés en même temps, instantané, lot, droit retiré puis donné en cours d'abonnement) : `crates/hearth-agent/tests/services_https.rs::the_stream_is_composed_for_each_reader_so_the_detail_of_an_adopted_service_reaches_only_the_right`. Mutations vérifiées : droit ignoré pour tout le flux, pour les lots seuls, pour l'instantané de changement de droit.
- 2026-10-13 — création (HRT-79, ADR-0043 : ce que l'agent publie, moitié agent, première partie).
- 2026-10-13 — HRT-79 : les journaux d'un service adopté sont réservés au droit (`may_read`).
