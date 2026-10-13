---
id: BR-JEU-040
domaine: JEU
titre: Une console distante est déclarée, son mot de passe est un secret généré, son port n'est jamais publié
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-serveur-de-jeu-au-quotidien.md (BR-JEU-040), contexts/hearth/conceptions/2026-10-13-technique-fiches-de-jeu.md, HRT-113
maj: 2026-10-13
---

# BR-JEU-040 — Une console distante est déclarée, son mot de passe est un secret généré, son port n'est jamais publié

## Règle
`[remote_console]` désigne un port de rôle `remote_console` (`publish = "never"` obligatoire, sans exception : `ConsolePubliee`) et un secret `[[secret]]` généré par l'agent (24 à 64 caractères alphanumériques, tiré par l'appelant, vérifié par `translate`). Le mot de passe est écrit par une variable ou un réglage imposé, sinon la fiche est refusée. Le brouillon de service ne contient jamais le port de la console. Hearth ne s'y connecte pas dans ce format (ADR-0052).

## Application (code)
- `crates/hearth-agent/src/domain/catalog/loading/invariants.rs::{check_ports, check_remote_console, check_cross}`
- `crates/hearth-agent/src/domain/catalog/translate.rs::translate (secrets générés, ports publiés)`

## Vérification
- `crates/hearth-agent/src/domain/catalog/tests_fiches.rs::i7_a_remote_console_is_never_published_and_its_password_is_generated`
- `crates/hearth-agent/src/domain/catalog/tests_translate.rs::a_request_that_the_fiche_cannot_honour_is_refused_with_a_typed_error`
- `crates/hearth-agent/src/domain/catalog/tests_translate.rs::property_every_accepted_fiche_and_valid_answers_give_a_closed_description`

## Cas limites
- Aucun test ne peut dire que le réglage qui ACTIVE la console est imposé : le format ne marque pas lequel c'est ; seule l'écriture du mot de passe généré est exigée.

## Règles liées
- BR-FICHE-007, BR-JEU-039

## Historique
- 2026-10-13 — création (HRT-113, format de fiche version 2, domaine pur).
