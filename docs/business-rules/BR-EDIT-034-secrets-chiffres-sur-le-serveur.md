---
id: BR-EDIT-034
domaine: EDIT
titre: Les secrets d'un service sont gardés scellés sur le serveur, jamais en clair dans la base, une trace, un message d'erreur ni le fil
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-modifier-un-service.md (BR-EDIT-034), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (sections 0, 0 bis et 6), HRT-83
maj: 2026-10-13
---

# BR-EDIT-034 — Les secrets d'un service sont gardés scellés sur le serveur, jamais en clair dans la base, une trace, un message d'erreur ni le fil

## Règle
Valeurs ET noms de variables, commande de lancement et réponses secrètes d'une fiche (mot de passe d'un serveur de jeu) sont des `Secret` en mémoire (jamais affichés, effacés à la libération) et, en base, des scellés ChaCha20-Poly1305 (`ring`), un par valeur, nonce de 96 bits tiré au hasard pour CE scellement et jamais réutilisé. Les données associées lient chaque scellé à SON service, à SA révision et à SON champ : un scellé recollé ailleurs ne s'ouvre pas. La clé (32 octets) vit dans `services.key` à côté de la base (`0600`, créée de façon atomique à la première nécessité, jamais dans la base, jamais dans une trace). Chaque révision qui porte un secret porte aussi un MANIFESTE scellé (nombre et noms ordonnés des autres champs) vérifié à l'ouverture : un scellé retiré, ajouté, recopié ou permuté en base est une description altérée (`Tampered`), jamais une description tronquée rendue comme valide. Fichier manquant, illisible, de mauvaise taille, à droits ouverts, qui n'est pas un fichier ordinaire (lien symbolique jamais suivi, dossier, tube), d'un autre propriétaire que l'utilisateur effectif de l'agent, ou qui ne correspond pas à la base (vérification HMAC gardée en base) : l'agent démarre, dit que les secrets sont illisibles, ne supprime rien, ne régénère PAS une clé par-dessus ; seule la fonction qui avait besoin des secrets est refusée, et une clé illisible ne fait JAMAIS disparaître de secret : écrire une révision sans secret quand la courante en garde, ou purger une révision dont la clé n'ouvre pas les scellés, est refusé (`SecretsUnreadable`) ; retirer volontairement les secrets, clé en main, reste permis. Aucune valeur n'est jamais envoyée au client hors de la révélation (BR-SVC-009), ni incluse dans une sauvegarde.

Rotation : non prévue dans cette tranche (le champ `format` des scellés et les données associées versionnées la rendront possible : re-sceller toutes les lignes sous une clé neuve dans une transaction).

Ce que cela protège : une copie de la base ou de sa sauvegarde, ses pages libres et son journal d'écriture. Ce que cela ne protège pas : un compte qui a déjà tous les droits sur le serveur (la clé est à côté ; le moteur garde d'ailleurs ces valeurs en clair dans son dossier).

## Application (code)
- `crates/hearth-agent/src/domain/sealed.rs::{associated_data, decide_key, SecretField, KeyProblem}`
- `crates/hearth-agent/src/domain/services/spec.rs::{SpecSecrets, EnvEntry, AnswerEntry, manifest_plain, manifest_matches}`
- `crates/hearth-agent/src/infrastructure/sealer.rs::RingSealer`
- `crates/hearth-agent/src/infrastructure/services_key.rs::FileServicesKeyStore`
- `crates/hearth-agent/src/application/secret_vault.rs::SecretVault`
- `crates/hearth-agent/src/application/service_specs.rs::ServiceSpecs`
- `crates/hearth-agent/src/app.rs::service_foundations`

## Vérification
- `crates/hearth-agent/tests/service_spec_secrets.rs::no_secret_value_reaches_a_trace_the_database_its_journal_or_a_debug_output`
- `crates/hearth-agent/tests/service_spec_secrets.rs::the_sentinel_scan_does_see_a_clear_value_in_the_database_the_journal_and_the_traces`
- `crates/hearth-agent/tests/service_spec_secrets.rs::a_sealed_value_pasted_in_another_service_revision_or_field_does_not_open`
- `crates/hearth-agent/tests/service_spec_secrets.rs::a_missing_wrong_or_unusable_key_makes_secrets_unreadable_and_destroys_nothing`
- `crates/hearth-agent/tests/service_spec_secrets.rs::an_agent_updated_from_main_starts_without_a_key_and_without_any_description`
- `crates/hearth-agent/tests/service_spec_secrets.rs::an_unusable_key_never_lets_secrets_disappear_by_a_secretless_revision_or_a_purge`
- `crates/hearth-agent/tests/service_spec_secrets.rs::removing_any_sealed_field_from_the_database_is_a_tampered_description`
- `crates/hearth-agent/tests/service_spec_secrets.rs::adding_duplicating_or_permuting_a_sealed_field_is_a_tampered_description`
- `crates/hearth-agent/src/domain/services/spec.rs::tests::the_manifest_matches_exactly_the_fields_it_lists`
- `crates/hearth-agent/src/infrastructure/services_key.rs::tests::{a_symbolic_link_is_never_followed, a_named_pipe_in_its_place_is_refused_without_blocking}` (Linux, CI seulement)
- `crates/hearth-agent/src/domain/sealed.rs::tests::a_key_is_created_only_when_nothing_in_the_database_ever_depended_on_one`
- `crates/hearth-agent/src/infrastructure/sealer.rs::tests::every_sealing_draws_a_new_nonce_even_for_the_same_value_and_context`

## Cas limites
- Une base d'un agent venu de `main` n'a ni clé ni description : l'agent démarre, aucun fichier n'est créé, la clé naît au premier secret à sceller.
- Une description sans secret n'a pas besoin de clé : elle s'écrit et se lit même si la clé est inutilisable.
- Retour arrière de la base (BR-UPDATE-029) : une clé créée après la copie reste sur le disque ; si la base n'a rien scellé, elle est adoptée et sa vérification retenue.
- Droits `0600`, création par lien dur, synchronisation du dossier, `O_NOFOLLOW`/`O_NONBLOCK`, propriétaire et droits relus sur le descripteur : code Linux, prouvé par la CI seulement.
- Une description sans secret n'a ni clé ni manifeste ; retirer d'un coup toutes les lignes de scellés d'une révision, manifeste compris, y ressemble (limite assumée, ADR-0044).

## Règles liées
- BR-EDIT-035, BR-DEPLOY-037, BR-SVC-009

## Historique
- 2026-10-13 — création (HRT-83, ADR-0044 et ADR-0045 : socle des services créés par Hearth, côté agent).
