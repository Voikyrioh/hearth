---
id: BR-MOD-032
domaine: MOD
titre: L'état activé d'un mod est le nom de son fichier (`.jar` actif, `.jar.disabled` désactivé)
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-11-16-fonctionnelle-gerer-les-mods-d-un-serveur-minecraft.md
maj: 2026-11-18
---

# BR-MOD-032 : L'état activé d'un mod est le nom de son fichier

## Règle
Désactiver = renommer `x.jar` en `x.jar.disabled` dans le même dossier ; réactiver = l'inverse. L'état n'est copié nulle part (ni base, ni fichier) : le nom est la seule vérité que le jeu lit, Hearth lit la même. La lecture (HRT-256) rend `enabled = false` pour un `.jar.disabled`, lui garde son verdict et son identité, et ne le compte ni pour les dépendances ni pour « Installé deux fois » (BR-MOD-021). Un fichier retiré (dossier « Mods retirés ») n'est jamais actif. Le geste de renommage (HRT-258) n'est pas encore livré.

## Application (code)
- `crates/hearth-agent/src/domain/mods/listing.rs::{is_active_jar, is_disabled_jar, active_name, disabled_name}`
- `crates/hearth-agent/src/application/mods.rs::assemble` (`enabled && place == Active`)

## Vérification
- `tests/mods_list.rs::two_versions_of_a_mod_are_a_duplicate_unless_one_is_disabled`

## Règles liées
- BR-MOD-020, BR-MOD-021, BR-MOD-024, BR-MOD-033

## Historique
- 2026-11-18 : création (HRT-256), partie lecture.
