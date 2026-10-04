---
id: BR-ACCT-014
domaine: ACCT
titre: Le serveur refuse la gestion de comptes à un compte lecture seule
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-gerer-comptes.md (BR-ACCT-014), HRT-03
maj: 2026-10-04
---

# BR-ACCT-014 — Le serveur refuse la gestion de comptes à un compte lecture seule

## Règle
Même si le client est modifié ou contourné, l'agent refuse création, suppression, changement de rôle, changement du mot de passe d'autrui et révocation par un compte lecture seule. Le contrôle est fait par l'agent, jamais par le client.

## Application (code)
- `crates/hearth-agent/src/domain/accounts/role.rs::Role::can_manage_accounts` — décision pure.
- Application aux routes : à venir avec HRT-04 (une seule couche d'extraction, test de balayage de toutes les routes modifiantes).

## Vérification
- Tests : `domain::accounts::role::tests::only_an_administrator_manages_accounts` ; test de balayage des routes à écrire dans HRT-04.

## Cas limites
- La ligne de commande n'a pas de rôle : elle s'exécute avec les droits du système sur le serveur (BR-ACCT-015).

## Règles liées
- BR-ACCT-013, BR-ACCT-015.

## Historique
- 2026-10-04 — création (HRT-03, session 2026-10-04-hearth-creation).
