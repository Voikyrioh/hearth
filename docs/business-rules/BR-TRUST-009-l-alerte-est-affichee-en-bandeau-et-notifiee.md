---
id: BR-TRUST-009
domaine: TRUST
titre: L'alerte d'attaque probable est affichée en bandeau sur toutes les pages du serveur et notifiée par Windows, une fois par épisode
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-06-fonctionnelle-poste-de-confiance.md (BR-TRUST-009) ; conception technique 2026-10-06 (5.7, 9.3) ; conception design 2026-10-07 (écrans B et D) ; contexts/hearth/tickets/hrt/HRT-26.md ; ADR-0024
maj: 2026-10-07
---

# BR-TRUST-009 : L'alerte d'attaque probable est affichée en bandeau sur toutes les pages du serveur et notifiée par Windows, une fois par épisode

## Règle
L'alerte est celle de l'agent (BR-TRUST-008) : le client ne la calcule jamais, il la reçoit dans le message `security` du flux (puis la relit par `GET /security` à chaque retour du lien).

- **Bandeau** : « Attaque probable détectée », sur toutes les pages du serveur concerné, tant que l'alerte dure, **hors** de la surface des données périmées (une alerte ne s'estompe pas quand le lien tombe : le bandeau dit « Dernier état connu à {heure} »). Non fermable. Le texte suit le rôle : le titulaire lit « ton identifiant » ; un administrateur lit en plus le NOMBRE d'autres comptes visés (jamais leurs noms) ; un compte Lecture seule lit « Tu vois l'alerte, mais seul un administrateur peut activer le mode attaque. » (BR-TRUST-029). Le bouton « Activer le mode attaque » ouvre la confirmation (BR-TRUST-010) ; il n'est plus proposé quand le mode est déjà actif.
- **Notification Windows** : UNE par épisode d'alerte (début d'un épisode : l'alerte devient visible ; le même épisode répété par des messages successifs ne notifie pas, un nouvel épisode notifie de nouveau). Elle suit la limite et le réglage de BR-TRUST-033. Textes : titre « Hearth : {nom du serveur} », corps « Attaque probable sur ton compte {identifiant}. Clique pour activer le mode attaque. » (titulaire administrateur avec la clé de ce PC), « … Clique pour voir les détails. » (Lecture seule, poste sans clé), « Attaque probable sur {n} compte(s) du serveur. … » (administrateur dont l'identifiant n'est pas visé).
- **Marque** : le serveur en alerte porte un triangle sur son avatar (barre des serveurs) et une étiquette « Alerte de sécurité » dans la liste ; priorité : mode attaque, suspendu, alerte.

## Application (code)
- Épisodes : `apps/desktop/src-tauri/src/presence.rs::SecurityWatch::observe`. Colle : `alerts.rs::Alerts::on_security` (observateur `link.rs::StateObserver::on_security`, `LinkRuntime::observe_security`). Textes : `texts.rs::{security_title, security_alert_body}`.
- Événement `link://security` : `link.rs::LinkRuntime::relay` (`Event::Security`), état tenu et rejoué : `security/book.rs::SecurityBook`.
- Interface : `components/organisms/SecurityAlertBanner.vue`, `molecules/SecurityBanner.vue`, `layouts/ServerLayout.vue`, `molecules/ServerAvatar.vue` (`mark`), `security/mark.ts::markOf`, `stores/security.ts`.

## Vérification
- `tests/security_alerts.rs` (épisodes, texte selon le rôle, jamais un nom, singulier et pluriel), `tests/security_runtime.rs::the_relay_keeps_the_last_state_and_replays_it_to_a_late_window`.
- `crates/hearth-link/tests/attack_mode.rs::the_stream_announces_the_security_state_right_after_the_authentication`.
- `pages/SecurityMode.test.ts` (« Alerte d'attaque probable », « Marque de sécurité sur un serveur »), `e2e/attack-mode.spec.ts`.

## Cas limites
- Au lancement du client pendant une alerte en cours, l'épisode est vu pour la première fois : une notification part (une par exécution et par épisode).
- Un clic sur la notification ramène la fenêtre au premier plan ; ouvrir directement la page Sécurité dépend du greffon de notifications (non fait, voir ADR-0016).
- Pas de notification quand la fenêtre est au premier plan et le bandeau visible : non fait (« à confirmer » du design).

## Règles liées
- BR-TRUST-008, 010, 029, 033, BR-RESIL-015, ADR-0024.

## Historique
- 2026-10-07 : création (HRT-26, session 2026-10-04-hearth-creation, T38).
