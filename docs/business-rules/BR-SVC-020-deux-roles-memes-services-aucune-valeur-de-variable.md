---
id: BR-SVC-020
domaine: SVC
titre: Mêmes services pour les deux rôles, aucune valeur de variable ne voyage
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-voir-les-services.md (BR-SVC-020), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §5, §7, §9, HRT-61
maj: 2026-10-13
---

# BR-SVC-020 — Mêmes services pour les deux rôles, aucune valeur de variable ne voyage

## Règle
Les deux rôles voient les mêmes services, les mêmes états et les mêmes ports. Un compte Lecture seule ne voit jamais la valeur d'une variable : cette tranche ne transporte aucune valeur ni aucun nom de variable, ni la commande de lancement, pour aucun rôle (`ServiceItem` n'a pas ces champs, `Observation` non plus, l'adaptateur ne les copie pas). Le texte « Réservé aux administrateurs » est porté par l'interface.

## Application (code)
- `crates/hearth-proto/src/api/services.rs::ServiceItem`
- `crates/hearth-agent/src/infrastructure/engine/translate.rs::observation_from`
- `crates/hearth-agent/src/domain/services/state.rs::Observation`

## Vérification
- `crates/hearth-agent/tests/services_https.rs::no_variable_value_name_or_command_reaches_a_message_a_response_or_a_trace`
- `crates/hearth-agent/src/infrastructure/engine/translate.rs::tests::nothing_from_the_environment_or_the_command_survives_the_translation`
- `crates/hearth-agent/tests/support/engine_contract.rs (étape 5, contre un vrai moteur en CI)`

## Règles liées
- BR-ENGINE-016

## Historique
- 2026-10-13 — création (HRT-61, tranche 1 des déploiements : moteur détecté et liste en lecture seule).
