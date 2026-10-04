---
id: BR-CLIENT-002
domaine: CLIENT
titre: L'installation se fait pour le compte Windows courant seulement
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-installer-client.md (BR-CLIENT-002), HRT-08
maj: 2026-10-04
---

# BR-CLIENT-002 — L'installation se fait pour le compte Windows courant seulement

## Règle
Le client s'installe dans le profil de l'utilisateur courant (`%LOCALAPPDATA%\Hearth`), pas au niveau du système. Les raccourcis et l'entrée de désinstallation sont créés pour cet utilisateur seul.

## Application (code)
- `apps/desktop/src-tauri/tauri.conf.json` : `bundle.windows.nsis.installMode = "currentUser"` (configuration).

## Vérification
- À la main : après installation, le dossier est sous `%LOCALAPPDATA%` et `HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall` porte l'entrée.

## Cas limites
- Politique d'administration stricte : rien à contourner, l'installation par utilisateur n'a pas besoin de droits.

## Règles liées
- BR-CLIENT-001

## Historique
- 2026-10-04 — création (HRT-08, session 2026-10-04-hearth-creation).
