---
id: BR-MOD-030
domaine: MOD
titre: Un moteur qui ne charge pas de mods le dit, sans lire aucun dossier
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-11-16-fonctionnelle-gerer-les-mods-d-un-serveur-minecraft.md
maj: 2026-11-18
---

# BR-MOD-030 : Un moteur sans mods le dit, sans lire de dossier

## Règle
Vanilla : `applicable = no_mods`. Paper et Purpur : `plugins` (Hearth ne gère pas les greffons). Une fiche sans `[mods.jars]` ou un service sans variante : `no_way`. Dans ces trois cas la réponse ne contient aucun mod et l'agent n'ouvre ni ne liste aucun dossier du service. Le moteur d'un serveur ne change pas après l'installation.

## Application (code)
- `crates/hearth-agent/src/application/mods.rs::judge_applicable`

## Vérification
- `tests/mods_list.rs::a_mods_folder_that_is_a_link_is_blocked_and_vanilla_paper_have_no_mods_and_no_read`

## Cas limites
- Une variante absente de `variants` de la fiche et inconnue : `no_mods`.

## Règles liées
- BR-MOD-001, BR-MOD-014, BR-MOD-020

## Historique
- 2026-11-18 : création (HRT-256).
