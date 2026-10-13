---
id: BR-SVC-008
domaine: SVC
titre: Le détail montre les noms de toutes les variables, jamais une valeur ; les valeurs sont masquées par défaut
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-voir-les-services.md (BR-SVC-008), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §7, §9, HRT-63
maj: 2026-10-13
---

# BR-SVC-008 — Noms de variables visibles, valeurs masquées par défaut

## Règle
Le détail d'un service montre les NOMS de toutes ses variables d'environnement, celles que l'image fournit comprises, pour les DEUX rôles (BR-SVC-020). Amendé par HRT-79 (ADR-0043, BR-ADOPT-015) : pour un service ADOPTÉ, les noms sont réservés aux comptes qui ont le droit « gérer les services » (retirés du message, `details_reserved`) ; pour un service créé par Hearth rien ne change (BR-ADOPT-016). Leurs VALEURS ne sont jamais dans la liste, le détail ni le flux : elles ne quittent le moteur que par `GET /services/{key}/env`, à la demande d'un administrateur (BR-SVC-009). L'interface les masque par un masque de longueur fixe (huit puces) qui ne révèle pas la longueur de la valeur.

- Côté agent, un nom est un `Hidden` (domaine) puis un `Concealed` (fil) : `Debug` ne montre rien, il n'y a pas de `Display`. Le nom n'entre dans aucune trace, aucune entrée du journal d'activité, aucune table de la base, aucun message d'erreur (test sentinelle).
- Nettoyés des caractères de contrôle et plafonnés : 255 caractères par nom, 256 noms au plus par service ; le surplus est compté dans `env_names_omitted` (« et N autres »).
- La valeur est coupée de l'entrée `NOM=valeur` sur place, dès l'inspection : elle n'est jamais copiée dans une `Observation`.
- Un service supervisé dont le conteneur n'existe plus n'a plus de noms (ils ne sont pas retenus par `supervised_services`).

## Application (code)
- `crates/hearth-agent/src/infrastructure/engine/translate.rs::{observation_from, env_names_of}`
- `crates/hearth-agent/src/domain/services/hidden.rs::Hidden`
- `crates/hearth-agent/src/domain/services/limits.rs::{MAX_ENV_NAMES, MAX_ENV_NAME_CHARS}`
- `crates/hearth-proto/src/api/services.rs::{Concealed, ServiceItem}`
- `crates/hearth-agent/src/entrypoint/services_wire.rs::service_item`

## Vérification
- `crates/hearth-agent/src/infrastructure/engine/translate.rs::tests::names_and_command_are_kept_hidden_and_no_value_survives_the_translation`
- `crates/hearth-agent/src/infrastructure/engine/translate.rs::tests::the_stop_timeout_the_names_and_the_command_are_bounded`
- `crates/hearth-agent/tests/services_https.rs::no_variable_value_reaches_a_message_a_response_or_a_trace_and_no_name_or_command_reaches_a_trace`
- `crates/hearth-agent/tests/support/engine_contract.rs (étape 5 bis, contre le faux moteur, le faux démon et un vrai moteur en CI)`

## Règles liées
- BR-SVC-007, BR-SVC-009, BR-SVC-020

## Historique
- 2026-10-13 — création (HRT-63, tranche 2 des déploiements : détail, superviseur, révélation des variables).
- 2026-10-13 — HRT-79 (ADR-0043) : noms de variables réservés au droit pour un service adopté.
