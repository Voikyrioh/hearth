---
id: BR-GRANT-005
domaine: GRANT
titre: Le droit se donne et se retire depuis le serveur seulement : aucune route ne l'écrit, et aucune route ne le fait prendre
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-droit-de-gerer-les-services.md (BR-GRANT-005), contexts/hearth/conceptions/2026-10-13-adr-droit-de-gerer-les-services-brouillon.md, HRT-76, revue locale du 2026-10-13 (B1)
maj: 2026-10-13
---

# BR-GRANT-005 — Le droit se donne et se retire depuis le serveur seulement : aucune route ne l'écrit, et aucune route ne le fait prendre

## Règle
Aucun écran du client, aucune action d'un administrateur, aucune demande réseau n'accorde ni ne retire le droit, y compris pour un compte qui se l'accorderait à lui-même. Les corps des routes (`POST /accounts`, `PATCH /accounts/{id}`) ne lisent aucun champ de droit : un `rights` ou `services_right` glissé par un client modifié est ignoré. La colonne n'est écrite par AUCUN code de l'agent en fonctionnement. Les deux seuls chemins qui la donnent ou la retirent sont locaux au serveur : la commande `hearth-agent account services` et la question de l'installation (HRT-77, BR-GRANT-008). Ils passent par UNE requête, dans UN fichier (`infrastructure/sqlite/services_right_repo.rs`), seule entrée de la liste des écrivains autorisés du test de sources ; un second test borne les fichiers qui nomment l'écriture et le cas d'usage `ServicesRightService` (jamais le serveur HTTP). L'écriture légitime contrôle qu'une seule ligne est écrite : un compte disparu entre la lecture et l'écriture est une erreur et rien n'est consigné comme réussi.

**Le droit ne se prend pas non plus par le réseau** (défaut à confirmer par le propriétaire, posé avec la revue locale du 2026-10-13). Un compte qui porte le droit ne voit ses moyens de connexion modifiés par le réseau que par lui-même ou par un compte qui porte lui aussi le droit. Sinon l'acte est refusé (`403`, droit manquant `services` en détail, entrée « refusé : droits insuffisants » au journal) avec le texte : « Ce compte peut gérer les services de ce serveur : seul un compte qui a ce droit, ou la ligne de commande du serveur, peut modifier ses moyens de connexion. » La ligne de commande locale reste permise (c'est le serveur). La règle suit le droit lu au moment de l'acte, dans la transaction de l'écriture. Sans cette règle, un administrateur sans le droit définirait le mot de passe d'un titulaire, ouvrirait une session à son nom et obtiendrait l'effet du droit sans que le serveur l'ait donné.

### Inventaire des actes qui peuvent mener d'un compte à un autre

| Acte (route) | Verdict | Pourquoi |
|---|---|---|
| Définir le mot de passe d'autrui (`PUT /accounts/{id}/password`) | **protégé** | refusé sur un titulaire sauf lui-même, un autre titulaire, ou la ligne de commande ; jugé dans la transaction |
| Changer son propre mot de passe (`PUT /me/password`) | sans danger | exige l'ancien mot de passe, ne vise que l'appelant |
| Changer le rôle (`PATCH /accounts/{id}`) | sans danger | le rôle est un axe indépendant du droit ; ne donne ni mot de passe, ni session, ni droit |
| Supprimer un compte (`DELETE /accounts/{id}`) | sans danger | le droit est une colonne de la ligne du compte et disparaît avec elle ; un compte recréé sous le même nom a un autre identifiant et naît sans le droit (BR-GRANT-002). Limite : l'octroi sur le serveur se fait PAR NOM. Un administrateur sans le droit peut supprimer le compte d'un titulaire et recréer un compte du même nom ; si le propriétaire relance ensuite l'octroi sur ce nom sans vérifier, il le donne au nouveau compte. Avant d'accorder, vérifier que le compte est bien celui qu'on croit (date de création, journal d'activité) |
| Créer un compte (`POST /accounts`) | sans danger | naît sans le droit quel que soit le rôle ; le créateur choisit le mot de passe d'un compte NEUF |
| Fermer les sessions d'autrui (`DELETE /accounts/{id}/sessions`) | sans danger | ferme des sessions et oublie des postes de confiance et des adresses retenues, n'en ajoute aucun ; refuse plus de monde, n'en laisse entrer aucun. Limite : c'est un moyen de gêner un titulaire (déni de service), et combiné au mode attaque il peut le tenir dehors tant qu'il n'a pas un poste reconnu ; le recours est la ligne de commande du serveur |
| Inscrire un poste de confiance ou une clé pour autrui | n'existe pas | un poste ne s'inscrit qu'à la connexion, avec le mot de passe du titulaire (BR-TRUST) |
| Retirer un poste (`DELETE /me/devices/{id}`) | sans danger | ne vise que les postes de l'appelant (`404` sinon) |
| Réinitialiser une clé | n'existe pas | pas de route de réinitialisation ; le retrait d'un poste ci-dessus |
| Alerte de sécurité (`GET /security`) | sans danger | lecture, ne vise que le compte de l'appelant |
| Mode attaque (`PUT /security/attack-mode`) | sans danger | réglage du serveur entier, ne vise aucun compte, n'ajoute de moyen de connexion à personne, exige la preuve de clé de l'appelant |
| Réglage de fréquence du mot de passe (`PUT /me/reauth`) | sans danger | ne vise que l'appelant |
| Récupération de compte | n'existe pas | seule voie : la ligne de commande du serveur (`account passwd`, `account revoke`) |
| Mise à jour de l'agent (`POST /agent/update`) | sans danger | n'installe qu'un binaire signé par la clé de publication ; la migration `0010` laisse le défaut « aucun » |
| Ligne de commande locale (`account passwd`, `role`, `remove`, `revoke`, `services`) | permise | c'est le serveur (root, accès à la base) |
| Flux (`/stream`) | sans danger | n'accepte que des abonnements, aucun acte |

Le classement est vérifié par construction : chaque route de `ENDPOINTS` est dans la table `CLASSES` de `entrypoint/http/mod.rs` avec son verdict ; une route nouvelle non classée fait échouer le test. Une garde générale (`no_chain_of_calls_from_an_administrator_without_the_right_reaches_a_services_request`) balaie les routes d'AUJOURD'HUI avec un corps fixe, celui d'un changement de mot de passe : un administrateur sans le droit appelle chaque route d'administration qui modifie, sur chaque type de compte, supprime et recrée les titulaires, puis essaie le mot de passe imposé sur tous les comptes ; aucun jeton obtenu n'est accepté au niveau `Services`. Cette garde ne prouve rien pour une route qui n'existe pas encore : une route nouvelle est arrêtée par le classement `CLASSES`, verdict écrit à la main et relu en revue, pas par ce balayage.

Détection d'une écriture ajoutée plus tard, de deux façons indépendantes : (1) test de sources, en texte brut sans lecture des guillemets : seuls les fichiers d'une liste fermée nomment la colonne, aucune instruction `UPDATE`, `INSERT`, `REPLACE`, `ALTER`, `DO UPDATE` ne la nomme hors de l'unique écrivain, aucune migration ne la nomme sauf `0010` pour la créer avec le défaut « aucun », et la seule requête vérifiée non `SELECT` (`.sqlx/`) qui la nomme est celle de l'écrivain ; (2) test d'exécution : des déclencheurs SQLite posés avant le balayage voient toute requête qui nomme la colonne dans sa liste `SET` (même à l'identique) et tout compte inséré avec une valeur autre que `none`.

## Application (code)
- `crates/hearth-agent/src/infrastructure/sqlite/account_repo.rs::SqliteUnitOfWork::insert (la colonne n'y est pas nommée)`
- `crates/hearth-agent/src/entrypoint/http/accounts.rs::{create, change_role} (aucun champ de droit lu)`
- `crates/hearth-agent/src/infrastructure/sqlite/services_right_repo.rs` (l'unique écrivain, appelé par la seule ligne de commande : BR-GRANT-008)
- `crates/hearth-agent/src/domain/accounts/rights.rs::Authority::may_change_login_means_of` (qui peut modifier les moyens de connexion d'un titulaire)
- `crates/hearth-agent/src/application/accounts.rs::AccountService::set_password` (la règle, dans la transaction ; refus `ServicesHolderProtected`)

## Vérification
- `crates/hearth-agent/tests/services_right_never_written.rs::no_code_of_the_agent_writes_the_services_right_and_few_files_name_it`
- `crates/hearth-agent/tests/services_right_never_written.rs::no_migration_but_the_creation_names_the_services_right`
- `crates/hearth-agent/tests/services_right_never_written.rs::the_checked_queries_only_read_the_services_right_but_for_the_one_allowed`
- `crates/hearth-agent/tests/services_right_never_written.rs::the_detector_recognises_writes_whatever_the_quoting_and_the_case`
- `crates/hearth-agent/tests/services_right_never_written.rs::only_the_command_line_and_the_installation_can_reach_the_write`
- `crates/hearth-agent/tests/http_api.rs::the_write_detector_sees_every_kind_of_write_of_the_services_right`
- `crates/hearth-agent/tests/http_api.rs::every_modifying_route_leaves_exactly_one_success_entry`
- `crates/hearth-agent/tests/http_api.rs::every_services_route_refuses_an_account_without_the_right_even_an_administrator`
- `crates/hearth-agent/tests/http_api.rs::a_read_only_account_with_the_right_reaches_nothing_but_the_services`
- `crates/hearth-agent/tests/http_api.rs::an_administrator_without_the_right_cannot_define_the_password_of_a_holder`
- `crates/hearth-agent/tests/http_api.rs::the_protection_follows_the_right_read_at_the_time_and_the_server_always_passes`
- `crates/hearth-agent/tests/http_api.rs::the_acts_declared_harmless_do_not_hand_a_holder_to_an_administrator_without_the_right`
- `crates/hearth-agent/tests/http_api.rs::no_chain_of_calls_from_an_administrator_without_the_right_reaches_a_services_request`
- `crates/hearth-agent/tests/services_grant_cli.rs::a_right_written_for_a_vanished_account_is_an_error_and_journals_nothing`
- `crates/hearth-agent/src/entrypoint/http/mod.rs::tests::every_route_is_classified_and_every_service_route_is_at_the_services_level`
- `crates/hearth-agent/src/domain/accounts/rights.rs::tests::the_means_of_a_holder_change_only_from_the_server_the_holder_or_another_holder`

## Cas limites
- Le détecteur d'exécution ne voit que les routes que le balayage appelle avec un scénario de réussite : une route modifiante sans scénario fait déjà échouer `every_modifying_route_leaves_exactly_one_success_entry` (« route modifiante sans scénario de réussite »).
- Le test de sources lit le texte brut : une requête dont le NOM de la colonne serait lui-même recomposé (`format!("services_{}", …)`) échapperait à (1), pas à (2).
- La règle des moyens de connexion suit le droit au moment de l'acte : si le serveur accorde le droit à un compte pendant qu'un administrateur sans le droit définit son mot de passe, l'ordre de la transaction décide (l'acte passe s'il précède l'octroi). Fenêtre de quelques millisecondes, sans élargissement du droit.
- Un administrateur sans le droit peut toujours fermer les sessions d'un titulaire et supprimer son compte (déni de service possible, jamais prise du compte) ; le titulaire se reconnecte avec son mot de passe, que personne d'autre n'a pu changer.
- Le client affichera pour ce refus « rôle insuffisant » tant que la liaison ne transporte pas `details.missing` (HRT-78, hors de ce ticket).

## Règles liées
- BR-GRANT-002, BR-GRANT-004, BR-GRANT-008, BR-ACCT-008

## Historique
- 2026-10-13 — création (HRT-76, ADR-0041 : le droit de gérer les services, moitié agent).
- 2026-10-13 — HRT-77 : l'unique écrivain autorisé est la requête de la commande du serveur ; second test sur les appelants.
- 2026-10-13 — revue locale : le droit ne se prend pas par le réseau (mot de passe d'un titulaire), inventaire des actes, garde de sources sur le texte brut, les migrations et `.sqlx/`, écriture contrôlée sur une ligne.
- 2026-10-13 — côté client (HRT-78) : une seule commande de la coquille touche au droit, `read_service_rights`, en lecture ; le pont simulé n'a que `setServiceRightOnServer` (le serveur, pour les tests). Gardé par `tests/rights_wire.rs::the_shell_has_one_command_for_the_right_and_it_only_reads`.
