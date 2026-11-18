---
id: BR-CONSOLE-006
domaine: CONSOLE
titre: On n'écrit que dans la console d'un serveur en marche et prêt, hors opération exclusive
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-11-17-fonctionnelle-console-et-joueurs-minecraft.md
maj: 2026-11-18
---

# BR-CONSOLE-006 : On n'écrit que dans la console d'un serveur en marche et prêt, hors opération exclusive

## Règle
Une commande n'est envoyée qu'à un service installé par Hearth (un service adopté : `SERVICE_ADOPTED`), dont la fiche déclare `[remote_console.use]` (sinon `CONSOLE_NOT_AVAILABLE`), EN MARCHE (sinon `SERVICE_NOT_RUNNING`) et dont le jeu n'est pas en démarrage (`SERVICE_STARTING` ; un jeu dont l'agent n'a pas vu le signe « prêt » depuis son propre redémarrage est « inconnu », pas « en démarrage » : la commande part, et la console injoignable est une réponse honnête). Le verrou du service est pris en mode PARTAGÉ sans attendre pendant l'échange (10 s au plus) : une opération exclusive (mods, sauvegarde avec arrêt, modification, restauration, reprise, suppression) refuse la commande `OPERATION_IN_PROGRESS`, et inversement. Refus, dans cet ordre, rien n'est envoyé : inconnu, adopté, console non ouverte, forme, verbe refusé, classe, arrêté, pas prêt, verrou, débit, injoignable.

## Application (code)
- `crates/hearth-agent/src/application/console.rs::{ConsoleService::send, ConsoleService::readiness, SharedGuard}`
- `crates/hearth-agent/src/entrypoint/http/console.rs::refusal`

## Vérification
- `crates/hearth-agent/tests/console_https.rs::{a_stopped_server_refuses_and_nothing_is_sent, an_exclusive_operation_on_the_service_refuses_the_command, an_unknown_service_is_not_found_and_a_service_without_a_console_is_not_available, a_hostile_form_is_refused_with_its_reason_and_nothing_is_sent}`

## Règles liées
- BR-CONSOLE-001, BR-CONSOLE-003, BR-JEU-001

## Historique
- 2026-11-18 : création (HRT-268). Écart : la phase inconnue (agent redémarré) laisse passer la commande ; seule une phase vue « en démarrage » la refuse.
