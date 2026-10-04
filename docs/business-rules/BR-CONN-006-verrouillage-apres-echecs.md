---
id: BR-CONN-006
domaine: CONN
titre: Après 5 échecs de connexion, une attente doublée à chaque échec (plafond 15 minutes) est imposée
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-connecter-serveur.md (BR-CONN-006), technique-socle §6-7, HRT-04
maj: 2026-10-04
---

# BR-CONN-006 — Verrouillage progressif des connexions

## Règle
Le 5e échec de connexion consécutif d'un même couple identifiant + adresse du client impose une attente de 1 minute avant la tentative suivante. Chaque échec fait ensuite (après l'attente) double l'attente : 1, 2, 4, 8 puis 15 minutes, plafond 15 minutes. Un succès remet le compteur et l'attente à zéro. Pendant l'attente, toute tentative est refusée sans même vérifier le mot de passe : `429 TOO_MANY_ATTEMPTS` avec `details.retry_after_s` (arrondi au-dessus). L'échec qui déclenche l'attente reçoit déjà cette réponse. Un second compteur par adresse seule ferme le balayage d'identifiants (BR-CONN-007).

Un compteur sans activité depuis 24 h (`domain::lockout::ATTEMPT_RETENTION`, et sans attente en cours) est purgé par la tâche périodique (BR-CONN-007).

## Application (code)
- `crates/hearth-agent/src/domain/lockout.rs::step` — fonction pure `(état, événement, maintenant) → (état, décision)` ; `retry_after_seconds` pour l'annonce.
- `crates/hearth-agent/src/application/sessions.rs::SessionService::login` — lit l'état, applique `step`, écrit dans la même transaction que l'issue.
- Route : `POST /api/v1/sessions` (`docs/open-api/sessions.md`).

## Vérification
- Tests : `domain::lockout::tests` (chaque palier, plafond, remise à zéro, attente restante, débordement) ; `tests/sessions_use_cases.rs` ; `tests/sessions_https.rs` (verrouillage avec `retry_after_s`).

## Cas limites
- Une tentative faite pile à la fin de l'attente est admise.
- Une tentative refusée pendant l'attente ne change pas l'état : elle ne rallonge pas l'attente.
- L'attente est comptée par couple, pas globalement : un autre identifiant ou une autre adresse n'est pas verrouillé.

## Règles liées
- BR-CONN-007, BR-CONN-013.

## Historique
- 2026-10-04 — création (HRT-04, session 2026-10-04-hearth-creation).
