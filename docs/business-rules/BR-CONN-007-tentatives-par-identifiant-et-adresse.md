---
id: BR-CONN-007
domaine: CONN
titre: Les tentatives échouées sont comptées par identifiant et adresse, et par adresse seule
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-connecter-serveur.md (BR-CONN-007, « par compte et par serveur »), technique-socle §6, HRT-04
maj: 2026-10-04
---

# BR-CONN-007 — Deux compteurs d'échecs de connexion

## Règle
La spec dit « par compte et par serveur » ; l'agent applique deux compteurs, que l'identifiant existe ou non (sinon le verrouillage révélerait quels comptes existent, BR-CONN-013) :

1. **Par couple (identifiant saisi, adresse IP du client)** : 5 échecs, puis attente 1 min doublée à chaque échec, plafond 15 min (BR-CONN-006). L'identifiant est normalisé (minuscules, espaces autour retirés, caractères de contrôle, séparateurs de ligne Unicode U+2028/U+2029 et caractères de format bidirectionnels retirés, 64 caractères au plus) : un retour à la ligne ne peut ni forger une ligne de journal ni brouiller la clé ; le séparateur de la clé est un caractère de contrôle, jamais présent dans l'identifiant.
2. **Par adresse seule, tous identifiants confondus** : 20 échecs en 10 minutes (la fenêtre s'ouvre au premier échec) bloquent l'adresse, mêmes paliers (1 min doublée, plafond 15 min). Il ferme le balayage d'identifiants (un identifiant différent à chaque requête échappe au compteur 1). Un succès ne le remet pas à zéro (sinon un attaquant intercalerait une connexion valide).

Une tentative est refusée dès que l'un des deux compteurs est en attente ; la réponse annonce l'attente la plus longue. L'adresse est celle de la connexion TCP (jamais un en-tête de mandataire, forgeable). Les connexions d'une même adresse sont traitées l'une après l'autre (un tour par adresse) : des tentatives simultanées ne dépassent pas les paliers. La tentative va jusqu'au bout même si le client coupe : l'échec est toujours compté. **File bornée** : une connexion est traitée, huit au plus attendent leur tour par adresse ; la suivante reçoit `503 BUSY` tout de suite, sans échec compté, pour que les mots de passe en attente ne s'accumulent pas en mémoire.

## Application (code)
- `crates/hearth-agent/src/domain/lockout.rs::{AttemptKey::new, AttemptKey::address, step, step_address, admits_in_queue}`, `crates/hearth-agent/src/domain/text.rs::is_unsafe_char`.
- `crates/hearth-agent/src/application/sessions.rs::SessionService::login` — lit les deux états, applique les deux fonctions, écrit dans la même transaction.
- `crates/hearth-agent/src/entrypoint/http/sessions.rs::ClientAddr` — adresse tirée de `ConnectInfo`.

## Vérification
- Tests : `domain::lockout::tests` (couple, adresse : fenêtre, paliers, plafond, succès sans effet, clés distinctes) ; `tests/sessions_use_cases.rs::an_address_trying_many_usernames_is_blocked_after_twenty_failures`, `::nine_simultaneous_wrong_logins_make_exactly_five_verifications`, `::the_ninth_waiting_connection_of_an_address_is_refused_busy_at_once`.

## Cas limites et limites connues
- **Contournement par changement d'adresse** : un attaquant qui dispose de nombreuses adresses garde 5 essais par identifiant et par adresse, et 20 par adresse. Aucun plafond par identifiant seul n'existe (il permettrait de verrouiller un compte à distance en le visant). Suivi : borne par identifiant tous clients confondus, à décider avec le journal d'activité (HRT-05).
- **Un poste légitime derrière la même adresse qu'un attaquant est bloqué avec lui** (le compteur par adresse ne distingue pas les personnes : NAT, poste partagé, mandataire).
- Derrière un mandataire, toutes les connexions partagent l'adresse du mandataire : le compteur par adresse bloquerait alors tout le monde. L'agent n'est pas prévu pour être derrière un mandataire.
- Purge des compteurs inactifs depuis 24 h : `application/maintenance.rs`.

## Règles liées
- BR-CONN-006, BR-CONN-013.

## Historique
- 2026-10-04 — création (HRT-04, session 2026-10-04-hearth-creation).
- 2026-10-04 — compteur par adresse seule ajouté, limites écrites (review Stephen, HRT-04).
- 2026-10-04 — file d'attente bornée par adresse (`503 BUSY`), nettoyage de l'identifiant étendu aux séparateurs Unicode et caractères de format (suivis review HRT-04, HRT-05).
