---
id: BR-FICHE-047
domaine: FICHE
titre: Un réglage inconnu, un commentaire et toute ligne incomprise ne sont jamais perdus ; une ligne commentée n'est jamais réactivée
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-047), contexts/hearth/conceptions/2026-10-13-technique-fiches-de-jeu.md, HRT-113
maj: 2026-10-13
---

# BR-FICHE-047 — Un réglage inconnu, un commentaire et toute ligne incomprise ne sont jamais perdus ; une ligne commentée n'est jamais réactivée

## Règle
Hearth ne retouche que la valeur du réglage demandé : tout le reste du fichier survit, à sa place, octet pour octet, à un enregistrement, une remise aux valeurs proposées, un import. Une ligne mise en commentaire reste un commentaire : régler ce réglage ajoute une vraie ligne. Les clés du fichier que la fiche ne connaît pas sont rendues par `settings_state::drift` (le fichier gagne).

## Application (code)
- `crates/hearth-agent/src/domain/catalog/formats/ (`set` et `unset` ne touchent que leur entrée)`
- `crates/hearth-agent/src/domain/catalog/settings_state.rs::drift`

## Vérification
- `crates/hearth-agent/src/domain/catalog/tests_state.rs::applying_edits_to_the_game_file_keeps_every_unknown_line_through_every_gesture`
- `crates/hearth-agent/src/domain/catalog/formats/kv_lines.rs::tests::a_commented_line_is_never_reactivated`
- `crates/hearth-agent/src/domain/catalog/formats/contract.rs::ten_successive_settings_keep_comments_unknown_lines_commented_lines_and_line_endings`

## Cas limites
- Le scellement des valeurs inconnues au repos et le masquage à l'affichage sont posés par HRT-108.

## Règles liées
- BR-FICHE-046, BR-FICHE-048

## Historique
- 2026-10-13 — création (HRT-113, format de fiche version 2, domaine pur).
