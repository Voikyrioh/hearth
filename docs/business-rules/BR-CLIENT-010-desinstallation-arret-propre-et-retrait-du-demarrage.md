---
id: BR-CLIENT-010
domaine: CLIENT
titre: La désinstallation arrête l'application et retire l'entrée de démarrage
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-installer-client.md (BR-CLIENT-010), HRT-08
maj: 2026-10-04
---

# BR-CLIENT-010 — La désinstallation arrête l'application et retire l'entrée de démarrage

## Règle
Si Hearth tourne, l'installateur de désinstallation demande de la fermer et l'arrête avant de supprimer les fichiers. L'entrée de démarrage de Windows est retirée (hors mise à jour).

## Application (code)
- Modèle NSIS de Tauri : `CheckIfAppIsRunning` et suppression de `HKCU\...\Run\Hearth` ; l'entrée porte le nom de produit, comme l'écrit l'application (`startup.rs`, forme entre guillemets depuis HRT-29). Aucune fonction `domain/`.

## Vérification
- À la main : activer le démarrage, désinstaller, vérifier l'absence de la valeur `Hearth` dans `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`.

## Cas limites
- Application dans la zone de notification : arrêtée comme n'importe quelle instance.

## Règles liées
- BR-CLIENT-009

## Historique
- 2026-10-04 — création (HRT-08, session 2026-10-04-hearth-creation).
