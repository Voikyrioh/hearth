---
id: BR-FICHE-049
domaine: FICHE
titre: La version du jeu est le texte compris entre deux repères littéraux, lu seulement si la lecture a été essayée
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-049), contexts/hearth/conceptions/2026-10-13-technique-fiches-de-jeu.md, HRT-113
maj: 2026-10-13
---

# BR-FICHE-049 — La version du jeu est le texte compris entre deux repères littéraux, lu seulement si la lecture a été essayée

## Règle
`[game_version]` déclare un repère de début littéral, de un à huit caractères de fin d'un seul caractère chacun, et un statut de preuve. `extract_game_version` rend le texte entre les deux, 32 caractères au plus, alphabet `[0-9A-Za-z._-]`, sinon rien ; la recherche est linéaire, sans expression régulière. Une lecture `untested` n'est pas crue. Les versions des réglages (`since`, `until`) se comparent numériquement (`10.0` > `9.9`) ; un réglage qui n'existe plus dans la version connue est refusé en écriture (`Reason::Version`), et sans version connue le réglage daté reste « inconnu ».

## Application (code)
- `crates/hearth-agent/src/domain/catalog/loading/value.rs::{extract_game_version, compare_versions}`
- `crates/hearth-agent/src/domain/catalog/settings_state.rs::version_status`
- `crates/hearth-agent/src/domain/catalog/stop_plan.rs::game_version_reader`

## Vérification
- `crates/hearth-agent/src/domain/catalog/loading/value.rs::tests::the_game_version_is_the_text_between_two_literals`
- `crates/hearth-agent/src/domain/catalog/loading/value.rs::tests::versions_compare_numerically_not_textually`
- `crates/hearth-agent/src/domain/catalog/tests_state.rs::cross_checks_variants_retired_settings_and_game_versions_are_enforced`
- `crates/hearth-agent/src/domain/catalog/tests_fiches.rs::i6_neither_regular_expression_nor_template_signs_and_versions_are_literals`


## Cas limites
- **Réglage disparu de la version (HRT-108)** : l'agent n'a pas de lecteur de la version INSTALLÉE du jeu branché (il lit les journaux du jeu, ce que la fiche de 7 Days to Die livre avec HRT-115). Tant que la version est inconnue, `version_status` rend « inconnue » et le champ `gone` de `GET /services/{key}/settings` vaut toujours faux : ce champ n'est PAS livré fonctionnel. `Reason::Version` refuse déjà l'écriture d'un réglage disparu quand une version est donnée au domaine.
- La ligne de journal qui annonce la version de 7 Days to Die n'est pas vérifiée (HRT-115).

## Règles liées
- BR-FICHE-037

## Historique
- 2026-10-13 — création (HRT-113, format de fiche version 2, domaine pur).
