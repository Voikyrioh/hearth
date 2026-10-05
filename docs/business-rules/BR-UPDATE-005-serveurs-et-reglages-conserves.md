---
id: BR-UPDATE-005
domaine: UPDATE
titre: Après la mise à jour et le redémarrage, les serveurs et les réglages sont conservés
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-mises-a-jour.md (BR-UPDATE-005), HRT-16
maj: 2026-10-05
---

# BR-UPDATE-005 : Après la mise à jour et le redémarrage, les serveurs et les réglages sont conservés

## Règle
Les serveurs enregistrés, les réglages et l'état de la mise à jour vivent dans le dossier de données de l'application (`%APPDATA%\fr.voikyrioh.hearth`), les mots de passe dans le coffre Windows : la mise à jour ne touche à aucun des deux. L'installateur de mise à jour est lancé en mode `/UPDATE` (réinstallation par-dessus, sans la case « Tout effacer », BR-CLIENT-008). Au démarrage, la version retenue comme « disponible » qui est désormais celle qui tourne est oubliée.

## Application (code)
- `apps/desktop/src-tauri/src/update/domain.rs::forget_installed`.
- `apps/desktop/src-tauri/installer/hooks.nsh` (aucun effacement hors désinstallation) ; mode `/UPDATE` du greffon.
- BR-CLIENT-008 pour la réinstallation conservant les données.

## Vérification
- `apps/desktop/src-tauri/tests/update_service.rs` : `after_an_update_the_installed_release_is_forgotten_at_startup`.
- `apps/desktop/src-tauri/tests/update_domain.rs` : `a_release_already_installed_is_forgotten`.
- À vérifier sur une vraie publication : serveurs, réglages et entrée de démarrage de Windows après une mise à jour NSIS (runbook, § « Premier essai »).

## Cas limites
- L'entrée de démarrage avec Windows (clé `Run`) doit survivre à l'installateur de mise à jour : à contrôler au premier essai réel.

## Règles liées
- BR-CLIENT-008, ADR-0017

## Historique
- 2026-10-05 : création (HRT-16, session 2026-10-04-hearth-creation).
