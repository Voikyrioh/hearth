---
id: BR-CLIENT-001
domaine: CLIENT
titre: L'installation se déroule sans droits administrateur ni logiciel tiers
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-installer-client.md (BR-CLIENT-001), HRT-08
maj: 2026-10-04
---

# BR-CLIENT-001 — L'installation se déroule sans droits administrateur ni logiciel tiers

## Règle
L'installateur ne demande jamais les droits d'administrateur Windows et ne dépend d'aucun logiciel tiers. WebView2 est embarqué dans l'installateur (bootstrapper), jamais à installer à la main.

## Application (code)
- `apps/desktop/src-tauri/tauri.conf.json` : `bundle.windows.nsis.installMode = "currentUser"` et `bundle.windows.webviewInstallMode = embedBootstrapper` (configuration, pas de fonction `domain/` : règle d'empaquetage).

## Vérification
- À la main : lancer l'installateur sans session administrateur, aucune fenêtre UAC ; `unzip -l`/7-Zip n'est pas nécessaire.

## Cas limites
- Windows 10 sans WebView2 : le bootstrapper intégré l'installe.

## Règles liées
- BR-CLIENT-002

## Historique
- 2026-10-04 — création (HRT-08, session 2026-10-04-hearth-creation).
