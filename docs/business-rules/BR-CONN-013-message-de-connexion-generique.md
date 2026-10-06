---
id: BR-CONN-013
domaine: CONN
titre: Le refus de connexion ne dit pas si l'identifiant ou le mot de passe est faux
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-connecter-serveur.md (BR-CONN-013), technique-socle §7, HRT-04
maj: 2026-10-06
---

# BR-CONN-013 — Refus de connexion générique

## Règle
Un identifiant inconnu et un mot de passe faux produisent exactement la même réponse (`401 INVALID_CREDENTIALS`, même message) **et suivent le même chemin** : dans les deux cas l'agent vérifie le mot de passe contre un haché (un haché factice quand l'identifiant n'existe pas), compte l'échec dans la même table et écrit dans la même transaction. La durée de la réponse ne distingue donc pas les deux cas. Le verrouillage s'applique aussi aux identifiants inconnus (BR-CONN-007).

## Application (code)
- `crates/hearth-agent/src/application/sessions.rs::SessionService::login` — les refus (identifiant tenté, adresse, raison ; jamais le mot de passe) sont journalisés en `warn` ; le journal d'activité (HRT-05) s'y branchera. Le haché vérifié est comparé à celui relu dans la transaction de création de la session.
- `crates/hearth-agent/src/infrastructure/argon2.rs::Argon2Hasher::decoy_hash` — haché factice, de mêmes paramètres que les vrais.

## Interface (coquille et vue)
- `apps/desktop/src/link/messages.ts::failureMessage` : « Identifiant ou mot de passe incorrect. » sous le mot de passe, jamais le champ fautif ; `components/organisms/LoginForm.vue`. Test : `src/components/organisms/connect.test.ts`, `src/composables/useAddServer.test.ts` (même texte pour un identifiant inconnu).

## Vérification
- Tests : `tests/sessions_use_cases.rs::unknown_username_and_wrong_password_take_the_same_path` (compte les appels au hacheur, ne chronomètre pas) ; `tests/sessions_https.rs` (même réponse sur le fil).

## Cas limites
- Identifiant de format invalide (trop court…) : traité comme inconnu, jamais une erreur de format qui trahirait la règle.

## Règles liées
- BR-CONN-006, BR-CONN-007.

## Complément HRT-20 (énoncé ci-dessus INCHANGÉ, repris de la version validée)
Précision seulement. Le ralentissement par identifiant (BR-CONN-018) s'applique à l'identique à un identifiant inexistant : mêmes compteurs, mêmes écritures, mêmes attentes annoncées, même trace au journal, et il refuse **après** la vérification du mot de passe (haché factice si l'identifiant n'existe pas), donc même durée. Aucune exemption des compteurs du couple et de l'adresse pour une adresse connue (BR-CONN-019). Les échecs d'une adresse connue d'un compte **quelconque** ne nourrissent pas le ralentissement, que l'identifiant existe ou non, et pendant l'attente un mot de passe faux compte dans le couple pour tout le monde : la progression des compteurs ne dit pas si une adresse est connue du compte visé. La réservation de places des connexions en cours dépend de l'adresse seule (BR-CONN-020).

**Reste observable, par construction** : le titulaire du mot de passe, depuis une adresse connue de son compte, passe pendant l'attente du ralentissement ; personne d'autre ne peut le constater sans ce mot de passe.

Tests : `tests/login_review.rs::{twenty_requests_from_a_known_address_do_not_tell_an_existing_identifier_from_a_missing_one, a_slowed_identifier_answers_a_known_address_like_a_missing_one_unless_the_password_is_right}` (la suite de 21 requêtes du reviewer, rouge avant correction), `tests/login_lockout.rs::{an_existing_and_a_missing_identifier_get_the_same_answers_the_same_path_and_the_same_trace, an_unknown_identifier_is_looked_up_with_the_same_query_as_a_known_one}`.

## Historique
- 2026-10-04 — création (HRT-04, session 2026-10-04-hearth-creation).
- 2026-10-05 : section Interface (HRT-10).
- 2026-10-06 — complément HRT-20 (section ajoutée, énoncé inchangé).
- 2026-10-06 — complément HRT-20 (section ajoutée, énoncé inchangé).
