---
id: ADR-0002
titre: Client Tauri 2 + Vue 3, réseau dans Rust
type: librairie
statut: acceptée
date: 2026-10-04
portee: projet
remplace: —
liens: [conception technique 2026-10-04, section 3]
---

# ADR-0002 — Client Tauri 2 + Vue 3, réseau dans Rust

## Contexte

Client desktop Windows doit : fenêtre visuelle (pas de terminal), notifications système, instance unique, coffre de secrets, mise à jour signée, épinglage TLS, résilience à la coupure réseau. Trois choix : Electron (poids, pas de Rust), rendu natif Slint/egui (écosystème composants limité, pas de revue navigateur), **Tauri 2 + web frontend**. Tauri offre : WebView2 (déjà installé W11), rustls embarqué, plugins officiels, léger, stack Vue conforme ADR-0004.

**Question Q1 (Voiky 2026-10-04)** : Tauri appliqué par défaut ici ; décision de diffusion (« Où publier ») laissée ouverte.

## Décision

Client : coquille Tauri 2.x (Rust) + frontend Vue 3. Tous les appels réseau (épinglage, connexion, reconnexion, opérations idempotentes, machine à états) vivent dans `crates/hearth-link` ; Tauri et Vue les invoquent par événement typé (via `tauri-specta` génération TypeScript).

## Comment l'appliquer

- `apps/desktop/src-tauri/main.rs` : initialise la fenêtre, zone notification, coffre, lecteur mises à jour.
- `apps/desktop/src/main.ts` : initialise Vue 3, hydrate stores depuis état Tauri.
- Tout appel réseau : `invoke()` → Tauri command → `crates/hearth-link` (async).
- Secrets (mots de passe, tokens) : reçus par commande Tauri, stockés dans le Gestionnaire d'identification Windows (crate `keyring`, côté cœur Rust), jamais en localStorage.

## Quand NE PAS l'appliquer / limites

- Pas de desktop web-view : refuser Electron en faveur de ce choix.
- Tauri nécessite un compilateur Rust et Microsoft Visual C++ runtime pour la cible Windows ; documenté dans `INSTALL.md`.
- Revue Nora reste possible via Playwright servant le front depuis Vite avec pont Tauri simulé.

## Alternatives rejetées

- **Electron** : poids (100+ Mo), engine Chromium redondant, pas de Rust.
- **Slint** : écosystème composants pauvre, rendu non standard pas revue navigateur.
- **egui / Iced** : instables, outil lookandfeel pas professionnel.
- **Réseau dans le front JavaScript** : impossible d'épingler TLS, secrets exposés, pas de reconnexion typée.

## Conséquences

- Build : `npm run tauri build` combine Rust et npm, génère NSIS Windows.
- Génération types : `tauri-specta` + `rustc` proc macro sync TypeScript ↔ Rust au compile.
- Performance : l'interface est limitée par le rendu web, pas d'accélération GPU pour saisies texte.

## Références

- Tauri 2 : https://tauri.app
- tauri-specta (IPC typé) : https://github.com/oscartbeaumont/tauri-specta
- Vue 3 : https://vuejs.org
- ADR-0004 convention (Vue) : `orga-global/docs/adr/ADR-0004-conventions-vue.md`
- Plugins Tauri : https://tauri.app/en/v1/api/js/
