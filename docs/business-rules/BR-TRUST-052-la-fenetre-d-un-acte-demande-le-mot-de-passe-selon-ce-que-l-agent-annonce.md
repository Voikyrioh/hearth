---
id: BR-TRUST-052
domaine: TRUST
titre: La fenêtre d'un acte demande le mot de passe selon ce que l'agent annonce, lu à chaque ouverture ; une seule fenêtre pour tous les actes
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-07-technique-administration-mot-de-passe-et-cle.md ; contexts/hearth/tickets/hrt/HRT-30.md
maj: 2026-10-23
---

# BR-TRUST-052 : La fenêtre d'un acte demande le mot de passe selon ce que l'agent annonce, lu à chaque ouverture ; une seule fenêtre pour tous les actes

## Règle
- Une seule fenêtre de confirmation (`AdminActDialog`) pour tous les actes. À chaque ouverture elle **lit** de l'agent (`get_reauth_state` : `GET /security`, champ `admin_reauth`) : l'interface ne devine ni la capacité de l'agent ni l'élévation de 5 minutes.
  - agent sans `admin_reauth` : aucun champ en plus, l'acte part comme avant ;
  - ce PC sans clé au coffre : explication et bouton « Me reconnecter pour enregistrer ce poste », rien ne peut partir, quel que soit l'état de confiance ;
  - **état de confiance** (HRT-194, ADR-0032 point 6) : `AdminReauthInfo` porte `trust` (`normal`, `alert`, `attack`). Le client ne lit plus `covered_by_elevation` en direct : il décide par `hearth_proto::admin_act::password_need`. Pilotage en NORMAL : aucun champ ; les autres actes gardent leur règle quel que soit `trust`. Valeur inconnue : traitée comme `attack`. Champ absent (agent plus ancien) : traité comme `alert`. L'agent décide à l'acte ; état changé entre la lecture et l'envoi : `password_required`, saisie conservée ;
  - délai de 5 minutes ouvert ET acte couvert (la règle est celle de l'agent, `hearth_proto::admin_act::password_need`, qui s'appuie sur `covered_by_elevation`, lue par `reauth_covers`) : pas de champ, le temps restant est écrit ; un acte non couvert (retirer un poste, mode attaque, mot de passe, rôle Administrateur, mise à jour de l'agent, réglage) demande toujours le mot de passe ;
  - sinon : le champ « Ton mot de passe », vidé après chaque envoi et à chaque ouverture.
- `password_required` (délai fermé entre la lecture et l'envoi) : la fenêtre redemande le mot de passe sans perdre la saisie de l'acte. Mot de passe faux ou attente : sous le champ, fenêtre ouverte.
- Réglage « Demander mon mot de passe » (page Sécurité, `window` ou `each`) : lu de l'agent, changé par un acte confirmé qui n'est jamais couvert. Le mode attaque est activable avec la clé de n'importe quel poste inscrit du compte (Q18), l'interface n'exige donc que la clé au coffre.
- Textes en tutoiement, sans tiret cadratin.

## Application (code)
- `hearth_proto::admin_act::covered_by_elevation` (couverture par l'élévation, source unique) et `password_need` (décision, agent et client ; HRT-194, pas encore dans le code) ; interface : `apps/desktop/src/components/organisms/AdminActDialog.vue`, `composables/useReauth.ts` ; coquille : `reauth/service.rs::{state, covers, set_setting}`.

## Vérification
- Vitest `components/organisms/adminAct.test.ts`, `reauthSetting.test.ts`, `link/simulated-reauth.test.ts`, `link/tauri-reauth.test.ts`, `security/security.test.ts` (alignement Q18) ; Playwright `e2e/reauth.spec.ts`, `e2e/accounts.spec.ts` ; coquille `reauth::service::tests`.

## Règles liées
- BR-TRUST-042, 043, 049, BR-LIFE-016, ADR-0032, ADR-0033.

## Historique
- 2026-10-07 : création (HRT-30, tranche B).
- 2026-10-23 : HRT-194 (décision Voiky 2026-10-20) : la fenêtre décide par `password_need` et lit `trust` dans `AdminReauthInfo` (ADR-0032, amendement, point 6). Implémenté par HRT-194, pas encore dans le code.
