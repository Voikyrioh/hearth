---
id: BR-TRUST-046
domaine: TRUST
titre: Chaque acte confirmé et chaque refus de confirmation sont consignés au journal, sans secret
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-07-technique-administration-mot-de-passe-et-cle.md ; contexts/hearth/tickets/hrt/HRT-28.md
maj: 2026-10-07
---

# BR-TRUST-046 : Chaque acte confirmé et chaque refus de confirmation sont consignés au journal, sans secret

## Règle
- Acte confirmé et réussi : l'entrée « réussi » de l'acte, écrite par le cas d'usage dans sa transaction (inchangé). Réglage de fréquence : `reauth.setting`.
- Refus : sous l'action de l'acte, par la couche d'accès (regroupement des refus par adresse, ADR-0024) : confirmation absente à un agent qui l'exige (« confirmation absente : client trop ancien »), preuve absente, preuve invalide, mot de passe requis, mot de passe faux (échoué, « mot de passe actuel incorrect »), attente imposée (« trop de tentatives, attente de N s », qui n'était pas consignée sous l'acte).
- **Jamais** dans le journal : mot de passe, défi, signature, clé publique ou son empreinte, jeton, identifiant saisi.

## Application (code)
- `entrypoint/http/reauth.rs::{refused, password_refusal, too_old}` + `entrypoint/http/auth.rs::guard` (marque `OutcomeMark`) ; `domain/audit/event.rs::Reason::{ReauthMissing, ProofMissing, ProofInvalid, PasswordRequired}` ; `domain/audit/action.rs::AuditAction::ReauthSetting`.

## Vérification
- `tests/admin_reauth.rs::the_journal_keeps_no_password_no_challenge_no_signature_and_no_key` et les assertions de journal des tests de refus.

## Règles liées
- BR-AUDIT-003, 005, 021, BR-TRUST-036, ADR-0031.

## Historique
- 2026-10-07 : création (HRT-28, tranches A et D1).
- 2026-10-08 : l'ouverture d'une élévation a sa propre entrée `reauth.elevation` (BR-TRUST-053).
