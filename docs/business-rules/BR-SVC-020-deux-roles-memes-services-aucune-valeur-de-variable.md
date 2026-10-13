---
id: BR-SVC-020
domaine: SVC
titre: Mêmes services et mêmes détails pour les deux rôles, aucune valeur de variable ne voyage
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-voir-les-services.md (BR-SVC-020), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §5, §7, §9, HRT-61
maj: 2026-10-13
---

# BR-SVC-020 — Mêmes services et mêmes détails pour les deux rôles, aucune valeur de variable ne voyage

## Règle
Les deux rôles voient les mêmes services, les mêmes états, les mêmes détails et les mêmes ports. Un compte Lecture seule ne voit jamais la valeur d'une variable : aucune valeur ne voyage dans `ServiceItem`, pour aucun rôle ; elles ne se lisent que par `GET /services/{key}/env`, réservée aux comptes qui ont le droit de gérer les services (BR-SVC-009, BR-GRANT-003). Depuis HRT-63, les NOMS des variables et la commande de lancement sont dans le détail (`env_names`, `command`), pour les deux rôles (BR-SVC-007, BR-SVC-008), dans le type `Concealed` (`Debug` masqué) ; ils n'entrent dans aucune trace. Risque assumé par la spec : une commande peut contenir un secret passé en argument (`--token=...`) ; un compte en lecture seule la voit. Ce qui distingue les comptes n'est plus le rôle mais le droit « gérer les services » (HRT-76, ADR-0041) : un compte sans le droit voit tout cela (liste, états, ports, noms de variables, commande) mais ne peut ni révéler les valeurs ni agir ; la raison affichée sous un bouton désactivé est portée par l'interface (lot client à venir ; la règle d'interface n'a pas encore de fiche). Le filtre de publication et la restriction du détail d'un service adopté arrivent avec l'adoption (tickets suivants).

## Application (code)
- `crates/hearth-proto/src/api/services.rs::ServiceItem`
- `crates/hearth-agent/src/infrastructure/engine/translate.rs::observation_from`
- `crates/hearth-agent/src/domain/services/state.rs::Observation`

## Vérification
- `crates/hearth-agent/tests/services_https.rs::no_variable_value_reaches_a_message_a_response_or_a_trace_and_no_name_or_command_reaches_a_trace`
- `crates/hearth-agent/tests/services_https.rs::the_detail_of_a_service_has_names_command_and_grace_for_both_roles_and_never_a_value`
- `crates/hearth-agent/src/infrastructure/engine/translate.rs::tests::names_and_command_are_kept_hidden_and_no_value_survives_the_translation`
- `crates/hearth-agent/tests/support/engine_contract.rs (étape 5, contre un vrai moteur en CI)`

## Règles liées
- BR-ENGINE-016, BR-SVC-007, BR-SVC-008, BR-SVC-009, BR-GRANT-003

## Historique
- 2026-10-13 — création (HRT-61, tranche 1 des déploiements : moteur détecté et liste en lecture seule).
- 2026-10-13 — HRT-63 : les noms de variables et la commande entrent dans le détail pour les deux rôles (BR-SVC-008) ; les valeurs restent hors du fil sauf par la révélation (BR-SVC-009).
