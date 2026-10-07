---
id: BR-TRUST-053
domaine: TRUST
titre: L'ouverture d'une élévation est consignée : qui, quel poste, d'où, quand ; jamais le mot de passe
statut: active
invariant: true
source: contexts/hearth/tickets/hrt/HRT-30.md ; revue de la PR #34 (journal) ; HRT-18 tranche 3
maj: 2026-10-08
---

# BR-TRUST-053 : L'ouverture d'une élévation est consignée

## Règle
- Chaque fois qu'un mot de passe juste ouvre (ou renouvelle) le délai de 5 minutes, l'agent écrit une entrée `reauth.elevation` (« Délai du mot de passe ouvert (5 minutes) », résultat « réussi ») : **qui** (le compte), **quel poste** (cible « poste {nom} », le nom annoncé à l'inscription de la clé prouvée), **d'où** (origine : nom du client et adresse), **quand** (horodatage de l'entrée). Jamais le mot de passe, le défi ni la signature.
- Un acte couvert qui passe SOUS le délai n'ouvre rien et n'écrit pas cette entrée (il garde l'entrée de son acte). Pas d'élévation (réglage `each`, mode attaque) : pas d'entrée.
- Une écriture hors transaction de l'acte (regroupement du journal) : si elle échoue, l'acte n'est pas défait.

## Application (code)
- `application/sessions.rs::SessionService::reauthenticate` (après `Elevations::open`) ; `domain/audit/action.rs::AuditAction::ReauthElevation` ; code `hearth_proto::api::audit::action::REAUTH_ELEVATION`.

## Vérification
- `tests/admin_reauth.rs::opening_an_elevation_is_journaled_with_who_which_device_and_when_never_the_password`.

## Règles liées
- BR-TRUST-043, 046, BR-AUDIT-003, ADR-0032.

## Historique
- 2026-10-08 : création (HRT-18 tranche 3, suivi de la revue r1 de la PR #34).
