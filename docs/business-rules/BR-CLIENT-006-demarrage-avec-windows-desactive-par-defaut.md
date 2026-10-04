---
id: BR-CLIENT-006
domaine: CLIENT
titre: Le lancement au démarrage de Windows est désactivé par défaut
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-installer-client.md (BR-CLIENT-006), HRT-08
maj: 2026-10-04
---

# BR-CLIENT-006 — Le lancement au démarrage de Windows est désactivé par défaut

## Règle
Un client fraîchement installé ne se lance pas au démarrage de Windows. L'état réel est celui de l'entrée `Run` de l'utilisateur (greffon autostart), jamais un doublon dans les réglages. Au démarrage de Windows l'application est lancée avec `--minimized` et reste dans la zone de notification.

## Application (code)
- `apps/desktop/src-tauri/src/settings.rs::read` : lit l'état de l'entrée de démarrage (absente = faux).
- `apps/desktop/src-tauri/src/domain.rs::is_minimized_launch` : `--minimized` garde la fenêtre cachée.
- Écart : la case « Lancer Hearth au démarrage de Windows » dans l'installateur lui-même n'est pas encore proposée (réglage disponible dans l'application, voir ADR-0010).

## Vérification
- Test : `tests/domain.rs::minimized_launch_is_detected_from_the_startup_argument`.
- Front : `src/stores/settings.test.ts::starts with the autostart option off`.

## Cas limites
- Réinstallation : l'entrée existante n'est pas modifiée (BR-CLIENT-008).

## Règles liées
- BR-CLIENT-007
- BR-CLIENT-008

## Historique
- 2026-10-04 — création (HRT-08, session 2026-10-04-hearth-creation).
