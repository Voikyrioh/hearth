---
id: BR-CLIENT-014
domaine: CLIENT
titre: Windows 10 64 bits et 50 Mo libres sont vérifiés avant toute copie
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-installer-client.md (BR-CLIENT-014), HRT-08
maj: 2026-10-04
---

# BR-CLIENT-014 — Windows 10 64 bits et 50 Mo libres sont vérifiés avant toute copie

## Règle
Avant d'écrire le moindre fichier, l'installateur vérifie Windows 10 ou plus récent en 64 bits (« Hearth a besoin de Windows 10 ou plus récent, en 64 bits. Ta version ne fonctionne pas. ») et au moins 50 Mo libres (« Il manque de la place. Libère au moins 50 Mo. Espace disponible : X Mo. »). Un refus annule l'installation : rien n'a été modifié, relancer fonctionne normalement.

## Application (code)
- `apps/desktop/src-tauri/installer/hooks.nsh` : macro `NSIS_HOOK_PREINSTALL` (`AtLeastWin10`, `RunningX64`, `DriveSpace`).
- Limite : une interruption en cours de copie laisse les fichiers déjà copiés (NSIS n'est pas transactionnel) ; la réinstallation les remplace, voir ADR-0010.

## Vérification
- À la main : non automatisable ici ; la compilation NSIS valide la syntaxe, le contrôle d'espace se teste en lançant l'installateur sur un disque presque plein.

## Cas limites
- Mode silencieux : le message est ignoré (`/SD IDOK`) mais l'installation est annulée.

## Règles liées
- BR-CLIENT-001

## Historique
- 2026-10-04 — création (HRT-08, session 2026-10-04-hearth-creation).
