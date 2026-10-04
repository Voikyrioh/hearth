---
id: BR-CONN-007
domaine: CONN
titre: Les tentatives échouées sont comptées par identifiant et par adresse du client
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-connecter-serveur.md (BR-CONN-007), technique-socle §6, HRT-04
maj: 2026-10-04
---

# BR-CONN-007 — Comptage par couple identifiant + adresse

## Règle
Le compteur d'échecs est propre à chaque couple (identifiant saisi, adresse IP du client), pas global. L'adresse est celle de la connexion TCP (jamais un en-tête de mandataire, forgeable). L'identifiant est normalisé (minuscules, espaces autour retirés, 64 caractères au plus) et compté **qu'il existe ou non** : sinon le verrouillage révélerait quels comptes existent.

## Application (code)
- `crates/hearth-agent/src/domain/lockout.rs::AttemptKey::new`.
- `crates/hearth-agent/src/entrypoint/http/sessions.rs` — adresse tirée de `ConnectInfo`.

## Vérification
- Tests : `domain::lockout::tests::the_key_is_per_username_and_address_and_ignores_case`, `::the_key_stays_bounded_whatever_the_username`.

## Cas limites
- Derrière un mandataire, toutes les connexions partagent l'adresse du mandataire : le verrouillage est alors par identifiant seul. L'agent n'est pas prévu pour être derrière un mandataire.
- Purge des compteurs inactifs depuis 24 h : `application/maintenance.rs`.

## Règles liées
- BR-CONN-006.

## Historique
- 2026-10-04 — création (HRT-04, session 2026-10-04-hearth-creation).
