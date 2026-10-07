---
id: BR-TRUST-051
domaine: TRUST
titre: Chaque essai d'un acte confirmé prend un défi neuf et une clé d'opération neuve
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-07-technique-administration-mot-de-passe-et-cle.md ; contexts/hearth/tickets/hrt/HRT-30.md
maj: 2026-10-07
---

# BR-TRUST-051 : Chaque essai d'un acte confirmé prend un défi neuf et une clé d'opération neuve

## Règle
- Chaque appel d'`execute_act` demande **son** défi (`purpose: admin_act`) et prend **sa** clé d'opération. Un refus de confirmation est retenu côté agent sous la clé d'opération, et un défi consommé ne ressert pas : rejouer l'ancienne clé ou l'ancien défi rendrait le même refus (suivi de la revue r2 de la PR #34).
- Après un acte confirmé qui échoue (mot de passe faux, délai fermé `password_required`, attente), la fenêtre reste ouverte et l'essai suivant repart avec un défi NEUF et une clé NEUVE, jamais l'ancienne. Une action dont le lien tombe avant la réponse reste « résultat inconnu » : jamais rejouée (BR-RESIL-009).
- Rien ne repart automatiquement : c'est l'utilisateur qui renvoie.

## Application (code)
- `hearth-link` : `manager/reauth.rs::LinkManager::send_confirmed` (défi demandé à chaque appel) ; `manager/mod.rs::LinkManager::execute_unchecked` (clé d'opération `ulid` à chaque appel).

## Vérification
- `crates/hearth-link/tests/admin_reauth.rs::each_try_takes_a_new_challenge_and_a_new_operation_key_so_a_refusal_is_never_replayed` (compte des défis, `replayed == false`) ; `a_covered_act_passes_without_a_password_during_the_elevation_and_asks_again_when_it_closes` (renvoi après `password_required`).

## Règles liées
- BR-TRUST-039, 040, 046, ADR-0033.

## Historique
- 2026-10-07 : création (HRT-30, tranche B).
