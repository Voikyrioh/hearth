---
id: BR-CONN-013
domaine: CONN
titre: Le refus de connexion ne dit pas si l'identifiant ou le mot de passe est faux
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-connecter-serveur.md (BR-CONN-013), technique-socle §7, HRT-04
maj: 2026-10-04
---

# BR-CONN-013 — Refus de connexion générique

## Règle
Un identifiant inconnu et un mot de passe faux produisent exactement la même réponse (`401 INVALID_CREDENTIALS`, même message) **et suivent le même chemin** : dans les deux cas l'agent vérifie le mot de passe contre un haché (un haché factice quand l'identifiant n'existe pas), compte l'échec dans la même table et écrit dans la même transaction. La durée de la réponse ne distingue donc pas les deux cas. Le verrouillage s'applique aussi aux identifiants inconnus (BR-CONN-007).

## Application (code)
- `crates/hearth-agent/src/application/sessions.rs::SessionService::login`.
- `crates/hearth-agent/src/infrastructure/argon2.rs::Argon2Hasher::decoy_hash` — haché factice, de mêmes paramètres que les vrais.

## Vérification
- Tests : `tests/sessions_use_cases.rs::unknown_username_and_wrong_password_take_the_same_path` (compte les appels au hacheur, ne chronomètre pas) ; `tests/sessions_https.rs` (même réponse sur le fil).

## Cas limites
- Identifiant de format invalide (trop court…) : traité comme inconnu, jamais une erreur de format qui trahirait la règle.

## Règles liées
- BR-CONN-006, BR-CONN-007.

## Historique
- 2026-10-04 — création (HRT-04, session 2026-10-04-hearth-creation).
