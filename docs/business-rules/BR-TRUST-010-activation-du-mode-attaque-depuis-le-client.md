---
id: BR-TRUST-010
domaine: TRUST
titre: Un administrateur active le mode attaque depuis le client, après confirmation, avec son mot de passe, sur un poste dont la clé est enregistrée
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-06-fonctionnelle-poste-de-confiance.md (BR-TRUST-010) ; conception technique 2026-10-06 (5.8, 9.1 à 9.4) ; conception design 2026-10-07 (écran A) ; Q14 point 3, Q16 ; contexts/hearth/tickets/hrt/HRT-26.md ; ADR-0025
maj: 2026-10-07
---

# BR-TRUST-010 : Un administrateur active le mode attaque depuis le client, après confirmation, avec son mot de passe, sur un poste dont la clé est enregistrée

## Règle
Depuis le bandeau d'alerte ou depuis la carte « Mode attaque » de la page Sécurité, l'administrateur demande l'activation ; une fenêtre de confirmation dit ce que le mode change, puis exige le **mot de passe actuel** (Q16). La preuve de la clé de CE PC est faite par la coquille, sans geste (acte `AttackMode`, usage `0x05`, liée au jeton et à l'acte ; la WebView ne voit jamais la clé, le défi ni la signature).

- **Poste sans clé enregistrée** (client mis à jour sans reconnexion par mot de passe, coffre sans clé, agent qui ne reconnaît pas le poste) : l'activation et la désactivation sont **indisponibles** ; le message de la spec (« Ce poste n'est pas encore enregistré, tu ne peux donc pas changer le mode attaque d'ici. Mets ton client à jour, puis reconnecte-toi avec ton mot de passe pour enregistrer ce poste. En attendant, tu peux utiliser la commande sur le serveur. ») est écrit sous le bouton ET dans son infobulle, sans toucher au serveur : sans clé au coffre la commande rend `NotRecognized` sans AUCUN appel d'écriture (pas même un défi). Un bouton « Se reconnecter » est proposé quand aucun mot de passe n'est mémorisé.
- **Agent trop ancien** : « L'agent de ce serveur est trop ancien pour avoir un mode attaque. »
- **Pendant l'action** : le bouton attend, un second envoi n'est pas possible ; le mot de passe est vidé après chaque envoi. Succès : confirmation écrite (« Mode attaque activé. … »), bandeau permanent (BR-TRUST-018 pour la sortie). Mot de passe faux : sous le champ, la fenêtre reste ouverte. Lien coupé avant la réponse : résultat inconnu dit, **jamais rejoué** (BR-RESIL-009).
- Trois raisons d'indisponibilité, UNE affichée, dans cet ordre : Lecture seule (BR-TRUST-029), agent trop ancien, poste sans clé.
- À l'écran quand le mode est actif : la sortie automatique est repoussée par les postes légitimes bloqués autant que par l'attaquant (BR-TRUST-019) ; un poste connu seulement par son adresse est bloqué dès qu'un essai raté a eu lieu depuis cette adresse, même si son titulaire tape le bon mot de passe au même moment (BR-TRUST-015) ; si l'adresse d'un poste change entre le défi et la connexion, sa preuve est ignorée.

## Application (code)
- `crates/hearth-link/src/manager/security.rs::LinkManager::{security, set_attack_mode, has_device_key}` ; `apps/desktop/src-tauri/src/security/{service,commands}.rs` (`set_attack_mode`, `interpret`) ; `link_dto.rs::LinkFailure::NotRecognized`.
- Interface : `security/gate.ts::attackModeBlock` (la raison), `components/organisms/{AttackModePanel,AttackModeDialog}.vue`, `composables/useAttackModeActions.ts`, `pages/Security.vue`.

## Vérification
- `crates/hearth-link/tests/attack_mode.rs` (activation et désactivation avec preuve ET mot de passe, sans clé rien n'est envoyé, hors « Connecté » rien n'est envoyé, Lecture seule).
- `apps/desktop/src-tauri/tests/security_runtime.rs` (contre un vrai agent), `src/security/security.test.ts`, `pages/SecurityMode.test.ts`, `e2e/attack-mode.spec.ts`.

## Cas limites
- Le message du flux ne dit rien du poste : la lecture `GET /security` donne `device` (`proven` ou `none`) ; tant qu'elle n'est pas faite, le bouton attend sans raison affichée.
- Activer un mode déjà actif est idempotent côté agent.

## Règles liées
- BR-TRUST-008, 009, 018, 019, 028, 029, ADR-0023, ADR-0025.

## Historique
- 2026-10-07 : création (HRT-26, session 2026-10-04-hearth-creation, T38).
- 2026-10-07 : HRT-28 : le client passera au contrat `reauth` (`0x05`) pour activer le mode attaque (HRT-30) ; l'agent accepte déjà les deux formes (BR-TRUST-036, 041).
