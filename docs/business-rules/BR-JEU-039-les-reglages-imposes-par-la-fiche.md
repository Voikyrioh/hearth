---
id: BR-JEU-039
domaine: JEU
titre: Un réglage imposé par la fiche n'est pas modifiable ; défait à la main, il est signalé et jamais réécrit en silence
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-serveur-de-jeu-au-quotidien.md (BR-JEU-039), contexts/hearth/conceptions/2026-10-13-technique-fiches-de-jeu.md, HRT-113
maj: 2026-10-13
---

# BR-JEU-039 — Un réglage imposé par la fiche n'est pas modifiable ; défait à la main, il est signalé et jamais réécrit en silence

## Règle
`imposed` porte exactement UNE origine (`value`, `derive` d'un préfixe + un réglage + un suffixe, `secret` généré, `port`) et une raison. Un réglage imposé n'est jamais `essential` et toute demande de le changer est refusée (`Reason::Imposed`). `imposed_broken` compare les réglages imposés au fichier et rend ceux qui ont été défaits : aucune écriture, le repère est affiché (« L'arrêt propre de ce jeu n'est plus garanti »).

## Application (code)
- `crates/hearth-agent/src/domain/catalog/settings_state.rs::{resolve_install, imposed_broken}`
- `crates/hearth-agent/src/domain/catalog/loading/loader.rs::check_imposed`
- `crates/hearth-agent/src/domain/catalog/loading/invariants.rs::check_derive`

## Vérification
- `crates/hearth-agent/src/domain/catalog/tests_state.rs::an_imposed_setting_undone_by_hand_is_reported_never_rewritten`
- `crates/hearth-agent/src/domain/catalog/tests_state.rs::a_frozen_an_imposed_a_retired_and_an_unknown_setting_cannot_be_changed`
- `crates/hearth-agent/src/domain/catalog/tests_translate.rs::the_rich_fiche_translates_ports_secrets_files_variants_and_never_publishes_the_console`

## Cas limites
- La fiche de 7 Days to Die (telnet, `querymode`) est HRT-115 ; Terraria n'a aucun réglage imposé de cette sorte.

## Règles liées
- BR-JEU-040, BR-FICHE-048

## Historique
- 2026-10-13 — création (HRT-113, format de fiche version 2, domaine pur).
