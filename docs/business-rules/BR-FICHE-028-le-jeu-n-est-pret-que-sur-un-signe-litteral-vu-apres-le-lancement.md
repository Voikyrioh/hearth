---
id: BR-FICHE-028
domaine: FICHE
titre: Le jeu est « prêt » seulement sur un signe littéral, essayé sur la vraie image, vu dans une ligne postérieure au lancement
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-028), HRT-101
maj: 2026-10-24
---

# BR-FICHE-028

## Règle
Quatre phases : « Premier démarrage », « Démarrage », « Prêt », « Hearth ne peut pas savoir si le jeu est prêt ». Le signe est une sous-chaîne littérale (jamais une expression régulière) cherchée dans les seules lignes écrites à partir du lancement ; un signe `untested` n'est jamais cru. Passé le délai de la fiche sans le signe, la phase devient « Hearth ne peut pas savoir » sans que rien soit arrêté ni relancé. Les phases vivent en mémoire de l'agent, jamais en base.

## Application (code)
- `crates/hearth-agent/src/domain/catalog/readiness.rs::{ready_since, ready_in, phase_of, ReadyRule}`
- `crates/hearth-agent/src/application/game_phase.rs::GamePhases`

## Vérification
- `crates/hearth-agent/src/domain/catalog/readiness.rs::the_sign_is_searched_only_in_lines_after_the_launch_and_as_a_literal`
- `crates/hearth-agent/tests/game_phase.rs::a_game_is_ready_only_when_the_sign_appears_in_a_line_written_after_the_launch`

## Cas limites
- La phase n'est pas encore projetée dans la liste des services ni dans le flux (champ `game` de `ServiceItem`) : elle l'est avec le client (HRT-102).

## Règles liées
- BR-FICHE-056, BR-JEU-038

## Historique
- 2026-10-24 — création (HRT-101).
