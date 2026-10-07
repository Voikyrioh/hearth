---
id: BR-TRUST-029
domaine: TRUST
titre: Un compte Lecture seule voit l'alerte et l'état du mode attaque mais ne peut ni l'activer ni le désactiver
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-06-fonctionnelle-poste-de-confiance.md (BR-TRUST-029) ; conception technique 2026-10-06 (5.7, 5.8, 8.4) ; contexts/hearth/tickets/hrt/HRT-26.md ; ADR-0025
maj: 2026-10-07
---

# BR-TRUST-029 : Un compte Lecture seule voit l'alerte et l'état du mode attaque mais ne peut ni l'activer ni le désactiver

## Règle
Le rôle est arbitré par l'AGENT : `PUT /security/attack-mode` est en `Access::Admin`, un compte Lecture seule reçoit `403 FORBIDDEN_ROLE`, consigné au journal (BR-TRUST-028). Il lit pourtant `GET /security` et le message `security` du flux : l'alerte de son propre identifiant et l'état du mode ; **jamais** `alert.others` (un compte n'apprend jamais qu'un autre identifiant est visé).

Le client n'arbitre rien : il dit ce que l'agent dira, sans lui envoyer ce qui serait refusé pour ce rôle. Pour un compte Lecture seule :
- bandeau d'alerte : « Tu vois l'alerte, mais seul un administrateur peut activer le mode attaque. », bouton « Activer le mode attaque » grisé (focusable) dont l'infobulle dit « Tu vois l'alerte mais ne peux pas activer le mode attaque. Contacte un administrateur. » ;
- carte « Mode attaque » : bouton grisé, la raison écrite dessous : « Tu n'as pas la permission d'activer le mode attaque. C'est réservé aux administrateurs. » (même texte pour désactiver) ;
- la page Sécurité et son entrée de menu lui sont ouvertes (sa liste de postes, l'état du mode).

Si l'agent refuse malgré tout (rôle changé depuis la dernière connexion), le refus est typé (`LinkFailure::Forbidden`) et montré avec le même texte.

## Application (code)
- `apps/desktop/src-tauri/src/security/service.rs::interpret` (`ForbiddenRole` rend `LinkFailure::Forbidden`) ; `security/dto.rs` (`AlertDto.others` absent pour ce rôle, tel que l'agent le rend).
- Interface : `security/gate.ts::attackModeBlock` (priorité à `readonly`), `components/organisms/SecurityAlertBanner.vue`, `AttackModePanel.vue`.

## Vérification
- `apps/desktop/src-tauri/tests/security_runtime.rs::a_read_only_account_sees_the_state_but_is_forbidden_to_change_it` ; `crates/hearth-link/tests/attack_mode.rs::a_read_only_account_is_refused_by_the_agent_and_changes_nothing`.
- `src/security/security.test.ts`, `pages/SecurityMode.test.ts` (« lets a read-only account see the alert… »), `e2e/attack-mode.spec.ts` (« compte en lecture seule »).

## Cas limites
- Sans clé au coffre, `NotRecognized` est rendu avant le refus de rôle (rien n'est envoyé, même pour un Lecture seule) ; l'interface, elle, affiche d'abord la raison « Lecture seule ».

## Règles liées
- BR-TRUST-008, 009, 010, 028, ADR-0025.

## Historique
- 2026-10-07 : création (HRT-26, session 2026-10-04-hearth-creation, T38).
