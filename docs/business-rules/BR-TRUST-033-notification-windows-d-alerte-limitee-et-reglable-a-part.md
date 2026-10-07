---
id: BR-TRUST-033
domaine: TRUST
titre: Les notifications Windows de sécurité suivent la limite d'une par minute et par serveur, avec priorité, et un réglage « Alertes de sécurité » à part
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-06-fonctionnelle-poste-de-confiance.md (BR-TRUST-033) ; conception technique 2026-10-06 (5.7) ; conception design 2026-10-07 (écran D) ; contexts/hearth/tickets/hrt/HRT-26.md ; ADR-0016
maj: 2026-10-07
---

# BR-TRUST-033 : Les notifications Windows de sécurité suivent la limite d'une par minute et par serveur, avec priorité, et un réglage « Alertes de sécurité » à part

## Règle
Deux natures de notification de sécurité : « attaque probable » (début d'un épisode d'alerte, BR-TRUST-009) et « le mode attaque s'est arrêté tout seul » (BR-TRUST-019, texte « L'attaque semble terminée. Le mode attaque s'est arrêté automatiquement. », seulement quand le client avait vu le mode actif ou suspendu juste avant).

- **Même limite que BR-RESIL-015** : au plus UNE notification par minute et par serveur, **toutes natures confondues** (lien et sécurité partagent la fenêtre).
- **Priorité de la sécurité** : retenue par la limite, une notification de sécurité part à l'échéance AVANT toute notification du lien, que celle-ci laisse derrière elle ; un changement d'état du lien ne la remplace jamais ; « attaque probable » part avant « arrêt du mode ». Un épisode d'alerte fini avant l'échéance retire sa notification retenue.
- **Réglage séparé** « Alertes de sécurité » (`settings.json`, `notifyOnSecurityAlert`), activé par défaut, distinct de « Notifier quand un serveur devient hors ligne ou revient » : couper l'un ne coupe pas l'autre et n'efface pas ce que l'autre retient. Le réglage coupe les notifications Windows seulement : les bandeaux de l'interface restent TOUJOURS affichés (la sécurité ne se masque pas). Les épisodes restent suivis réglage coupé : le réactiver en plein épisode ne le rejoue pas.
- Le texte ne contient jamais le nom d'un autre compte (l'agent n'en donne que le nombre).

## Application (code)
- Règles pures : `apps/desktop/src-tauri/src/presence.rs::{NotificationGate::{push_security, drop_security, clear_link, clear_security, poll}, SecurityWatch::observe}`.
- Colle : `alerts.rs::Alerts::{on_security, set_security_enabled}`. Réglage : `settings.rs::{notify_on_security_alert, set_notify_on_security_alert}`, commandes `get_notify_on_security_alert` / `set_notify_on_security_alert`, ligne de `pages/Settings.vue` (section « Client »).

## Vérification
- `apps/desktop/src-tauri/tests/security_alerts.rs` (fenêtre partagée, priorité, une par nature, épisode fini avant son tour, réglages indépendants, réactivation sans rejeu, serveur retiré).
- `src/pages/SecuritySettings.test.ts` (ligne dédiée, activée par défaut, commandes propres), `e2e/attack-mode.spec.ts` (réglage).

## Cas limites
- Le greffon de notifications ne dit pas si Windows a affiché la notification (ADR-0016) ; un clic n'ouvre pas la page Sécurité (le design le demande « à vérifier »).

## Règles liées
- BR-RESIL-015, BR-TRUST-009, 019, ADR-0016.

## Historique
- 2026-10-07 : création (HRT-26, session 2026-10-04-hearth-creation, T38).
