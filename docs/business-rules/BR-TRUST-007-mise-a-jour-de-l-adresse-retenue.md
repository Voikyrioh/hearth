---
id: BR-TRUST-007
domaine: TRUST
titre: Le serveur retient l'adresse d'une connexion réussie ; une session valide accompagnée d'une preuve de clé fait retenir l'adresse, et l'usage d'une session la rafraîchit
statut: partielle
invariant: false
source: contexts/hearth/conceptions/2026-10-06-fonctionnelle-poste-de-confiance.md ; conception technique 2026-10-06 ; contexts/hearth/tickets/hrt/HRT-22.md ; ADR-0023
maj: 2026-10-07
---

# BR-TRUST-007 : Le serveur retient l'adresse d'une connexion réussie ; une session valide accompagnée d'une preuve de clé fait retenir l'adresse, et l'usage d'une session la rafraîchit

## Règle
Le serveur retient l'adresse IP **exacte** d'une connexion réussie (table `known_addresses`, ADR-0022). Elle est :

- **apprise** par deux preuves seulement : une connexion par mot de passe accordée (dans sa transaction ; liée au poste si une clé est prouvée), et une **session valide accompagnée d'une preuve de clé valide** (ouverture du flux, usage « session » lié au jeton) : l'adresse change, le poste reste reconnu et l'adresse retenue est mise à jour. **La preuve ne relie jamais la session à un poste** (BR-TRUST-048) : sur une session reliée, seule la clé de son poste sert, celle d'un autre poste du compte ne fait rien retenir ;
- **jamais apprise par une session seule** : une session volée, rejouée ailleurs, ne fait pas retenir l'adresse du voleur ;
- **rafraîchie** (durée repoussée, rien d'appris) quand une session valide est utilisée depuis une adresse déjà retenue, au rythme du renouvellement de session (5 minutes) : un poste utilisé chaque jour ne cesse pas d'être connu au bout de 30 jours ;
- retenue **30 jours** après le plus récent de la dernière connexion réussie et du dernier usage reconnu ; 8 par compte ; un poste à clé n'a qu'**une** adresse à la fois (sa ligne est déplacée quand il prouve sa clé depuis une autre adresse) ; oubliée avec son poste.

**Deux postes derrière la même adresse** : la table a une ligne par (compte, adresse), liée au **dernier poste qui a prouvé** sa clé depuis elle ; les deux postes restent inscrits, chacun garde sa dernière adresse dans la liste, et le premier retrouve l'adresse à sa prochaine preuve. Une preuve valide d'une clé non inscrite ne fait rien retenir, mais la session, elle, est renouvelée et son adresse déjà retenue est repoussée.

> **Partielle** : « si l'adresse change mais que le poste présente deux autres critères, il reste reconnu » est la règle 2 sur 3 (HRT-24). HRT-22 livre ce qui rend cette règle possible : l'apprentissage, le rafraîchissement et le lien au poste.

## Application (code)
- `crates/hearth-agent/src/application/trust.rs::TrustService::{on_login, on_session_proof}` ; `application/sessions.rs::SessionService::{login_in_turn, authenticate_inner}`.
- `crates/hearth-agent/src/domain/known_address.rs::learn` (inchangé) ; `infrastructure/sqlite/known_address_repo.rs::{replace, bind_device, touch}` (colonnes `device_id`, `last_used_at` de la migration 0005).
- `crates/hearth-agent/src/entrypoint/http/auth.rs::authenticate` (l'adresse de la connexion TCP atteint le contrôle de session) ; `entrypoint/ws/connection.rs`.

## Vérification
- `tests/device_proof.rs` : `a_device_has_one_address_at_a_time_the_old_one_is_forgotten`, `using_a_session_from_a_retained_address_refreshes_it_every_five_minutes_and_learns_nothing`, `a_session_with_a_valid_key_proof_makes_the_new_address_retained_and_moves_the_device`, `a_session_alone_never_enrolls_a_device_nor_retains_an_address`.
- `tests/device_stream.rs::the_key_is_proven_over_tls_enrolled_by_the_login_and_the_stream_proof_retains_the_address` (session seule, preuve fausse, mauvais usage, rejeu : rien n'est appris ; session + clé : retenue).

## Cas limites
- Adresse IPv6 temporaire qui change : la nouvelle adresse n'est retenue qu'à la prochaine connexion par mot de passe ou à la prochaine preuve de clé.
- Le poste sans clé (client actuel) garde le comportement d'avant : adresse apprise à la connexion, rafraîchie par l'usage de sa session.

## Règles liées
- BR-TRUST-005, BR-CONN-019, ADR-0022, ADR-0023.

## Historique
- 2026-10-07 : création (HRT-22, session 2026-10-04-hearth-creation, T32).
- 2026-10-07 : HRT-28 (tranche F) : une preuve de session ne relie plus la session à un poste (BR-TRUST-048).
