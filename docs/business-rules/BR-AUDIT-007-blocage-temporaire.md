---
id: BR-AUDIT-007
domaine: AUDIT
titre: Le blocage temporaire d'un compte est journalisé
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-journal-activite.md (BR-AUDIT-007), HRT-05 ; Q14 (points 9, 10, 11) ; contexts/hearth/tickets/hrt/HRT-24.md ; ADR-0024
maj: 2026-10-07
---

# BR-AUDIT-007 — Le blocage temporaire d'un compte est journalisé

## Règle
La tentative refusée qui déclenche une attente (BR-CONN-006 : 5 échecs par couple identifiant + adresse ; BR-CONN-007 : 20 échecs en 10 minutes par adresse) écrit deux événements : la connexion refusée, puis l'action « Blocage temporaire » (le premier de chaque groupe est écrit tout de suite, les suivants sont regroupés, voir plus bas), tous deux avec le compte visé **si l'identifiant saisi correspond à un compte existant** (sinon sans compte, BR-AUDIT-005 et 006) : (résultat refusé, raison « trop de tentatives, attente de N s »). Les seuils « à définir en conception technique » sont ceux de BR-CONN-006 et 007.

## Regroupement des refus et échecs répétés
Les refus faute de droits, les échecs écrits par la couche d'accès et **les connexions refusées** (tous par `AuditSink`, hors transaction) sont regroupés. **La clé** : le compte authentifié (ou « anonyme »), l'**adresse de la connexion** (**MODIFIÉE par HRT-24, décision du détenteur Q14 point 9 : « on regroupe par adresse »** ; avant, ni le nom du poste ni l'adresse n'y entraient), l'action, le résultat, la cible (un compte existant ou le motif statique de la route) et la raison (énumération). **Ni le nom du poste ni l'identifiant saisi** : un client les fait varier à volonté. L'adresse est celle de la connexion TCP, jamais un en-tête.

**Fenêtre qui s'allonge (HRT-24)** : le premier événement d'un groupe est écrit tout de suite ; les suivants sont comptés ; **une** entrée de synthèse est écrite à la fin de la fenêtre, au compte exact (tâche toutes les 15 s, et à l'arrêt de l'agent pour tout ce qui est en attente) : elle reprend l'origine de la **dernière** occurrence, avec `repeat_count` et la raison « … (N autres fois en 1 min) ». Tant que le groupe revient, la fenêtre suivante **double** : 1, 2, 4, 8 puis **15 minutes** (plafond). Un groupe sans occurrence depuis **30 minutes** est oublié : la fenêtre repart à 1 minute (un groupe jamais répété s'oublie à la fin de sa première fenêtre). Les événements d'une fenêtre qui s'allonge sont comptés, jamais écrits un par un.

**Plafond par famille (Q14, point 10, volume borné)** : la famille est la clé SANS adresse (compte ou anonyme, action, résultat, cible, raison). Au plus **8 adresses distinctes** ont leur groupe par fenêtre de famille ; les tentatives des adresses suivantes ne sont **pas écrites une par une** : elles sont comptées et **une** entrée de synthèse « (N tentatives depuis M adresses) » est écrite en fin de fenêtre de la famille, sans adresse ni liste (origine sans adresse), au compte exact des tentatives ; les adresses distinctes au-delà de 8 sont comptées jusqu'à 256 (M plafonné, la mémoire ne grandit pas). La fenêtre de la famille s'allonge (1, 2, 4, 8, 15 minutes), et dès qu'elle a vu 4 adresses distinctes. Aucune tentative n'est perdue dans les comptes.

Conséquences : une attaque d'une tentative toutes les 15 secondes **depuis une même adresse** (3 refus par tentative) laisse 48 entrées en 3 heures au lieu de plusieurs centaines (`domain::audit::repeat::tests`) ; un compte lecture seule qui boucle sur `GET /audit` avec des noms de poste différents depuis une même adresse écrit deux entrées (5 000 requêtes, 2 entrées) ; le même compte qui vise le mot de passe de marie, de paul puis de carl dans la minute laisse une trace pour chacun (la cible est dans la clé) ; deux refus du même compte vers deux routes différentes (actions différentes) restent distincts. Une attaque qui change d'adresse à CHAQUE tentative laisse 387 entrées en 3 heures (une tentative par 15 s) et 405 à 50 tentatives par seconde (contre 541 avant HRT-24 et 814 sans le plafond par famille) ; elle est bornée par le temps, pas par le nombre de tentatives. Reste borné par le **nombre de comptes existants visés** : dix comptes à la fois, 3 870 entrées en 3 heures, 50 000 lignes en 39 h (ADR-0024, « Limites » : toutes les mesures).

Garde-fou : au plus 1 024 groupes et 1 024 familles suivis. Au débordement, les groupes neufs sont comptés dans un groupe de débordement dont la fenêtre s'allonge de la même façon. Au débordement, les événements nouveaux ne sont plus écrits un par un : ils sont comptés dans **un groupe de débordement unique**, résumé par une entrée « activité trop variée, N événements regroupés », **sans compte ni cible** (les N événements viennent de comptes différents) ; elle garde l'action et l'origine de la dernière occurrence.

## Application (code)
- `crates/hearth-agent/src/domain/audit/repeat.rs::RepeatFilter` ; `application/audit.rs::AuditRecorder::{record, flush}` ; `entrypoint/tasks.rs::spawn_audit_flush`.
- `crates/hearth-agent/src/application/sessions.rs::SessionService::login_in_turn`.
- `crates/hearth-agent/src/domain/audit/action.rs::AuditAction::LoginLocked`.
- `crates/hearth-agent/src/domain/audit/event.rs::Reason::TooManyAttempts`.

## Vérification
- `domain::audit::repeat::tests` (dont `the_window_grows_one_two_four_eight_then_fifteen_minutes_with_the_exact_count`, `a_group_quiet_for_thirty_minutes_starts_again_at_one_minute`, `many_addresses_of_one_family_write_eight_entries_and_one_summary_with_the_exact_counts`, `the_distinct_addresses_counted_per_family_never_grow_past_their_bound`, `the_family_window_grows_when_the_ceiling_is_reached_even_without_overflow`, `measure_1_…`, `measure_2_…`, `measure_3_…`, `measure_4_one_address_does_not_regress`) ; `crates/hearth-agent/tests/audit_use_cases.rs::a_thousand_identical_refusals_make_two_entries`, `::refusals_of_different_accounts_are_not_grouped_and_a_later_one_brings_the_summary`, `::five_thousand_refusals_of_one_account_with_changing_hosts_and_addresses_make_nine_entries`, `::five_thousand_refusals_of_one_account_from_one_address_with_changing_hosts_make_two_entries`, `::refused_logins_from_many_addresses_leave_eight_entries_and_one_summary`, `::targeting_three_accounts_in_a_minute_leaves_a_trace_for_each`, `::anonymous_events_of_one_action_group_whatever_the_number_of_addresses`, `::stopping_writes_every_pending_summary_before_the_windows_end`.
- `crates/hearth-agent/tests/audit_use_cases.rs::the_attempt_that_locks_adds_a_lock_entry_and_attempts_during_the_wait_add_nothing`.

## Cas limites
- Les tentatives refusées **pendant** l'attente ne sont pas journalisées : une rafale contre un compte bloqué ne doit pas faire tourner les 50 000 entrées du journal et en chasser l'historique utile. Le blocage lui-même est écrit une fois.
- Une connexion refusée parce que la file de l'adresse est pleine (`429 TOO_MANY_ATTEMPTS`, `details.retry_after_s = 1`) n'est pas journalisée non plus (elle est tracée en `warn` : adresse et raison), pour la même raison.

## Règles liées
- BR-CONN-006, BR-CONN-007, BR-AUDIT-003.

## Complément HRT-20 : CHANGEMENT de la règle de la ligne « Les tentatives refusées pendant l'attente ne sont pas journalisées » (À VALIDER)
L'énoncé ci-dessus est conservé tel quel. **Ce complément y déroge** pour un seul cas : les tentatives refusées à cause du **ralentissement par identifiant** (BR-CONN-018) sont **comptées** au journal, au lieu d'être ignorées. Raison : une attaque depuis de nombreuses adresses est précisément ce que le journal doit montrer. Le journal reste borné : la clé de regroupement ne contient **aucune donnée variable** (la durée d'attente n'en fait pas partie, `Reason::group_text`), on obtient une entrée « Connexion refusée, raison « trop de tentatives, attente de N s » » puis une synthèse au compte exact par fenêtre de 60 secondes, sans identifiant saisi, avec le compte visé seulement s'il existe. Trois heures d'attaque (une tentative toutes les 15 secondes depuis des adresses toujours neuves) laissaient 541 entrées avant HRT-24 ; avec l'adresse dans la clé et le plafond par famille (ci-dessus), 329 dans le test d'intégration (`login_review`) : chaque tentative compte, aucune n'est perdue. Le 11e échec écrit aussi « Blocage temporaire ». Les refus dus au seul compteur du couple ou de l'adresse restent hors journal.

**Option si le détenteur refuse ce changement** : ne pas journaliser les refus ralentis (comme les autres attentes) ; le journal ne montre alors que les 10 premiers refus, le blocage et les échecs admis à chaque fin d'attente.

Tests : `tests/login_review.rs::an_attack_of_hours_leaves_a_bounded_journal_with_the_exact_count_of_attempts` (horloge qui avance), `tests/audit_use_cases.rs::{refused_logins_from_many_addresses_leave_one_entry_and_one_summary, refused_logins_from_many_addresses_are_all_counted_in_bounded_entries}`, `domain::audit::event::tests::the_group_text_of_a_lock_does_not_carry_the_wait`.

## Historique
- 2026-10-04 — création (HRT-05, session 2026-10-04-hearth-creation).
- 2026-10-06 — complément HRT-20 (section ajoutée, énoncé inchangé).
- 2026-10-06 — complément HRT-20 : refus ralentis comptés, durée hors de la clé de regroupement (changement à valider).
- 2026-10-07 — HRT-24 (T33), revue r1 : plafond par famille de 8 adresses par fenêtre et synthèse « N tentatives depuis M adresses » (Q14 point 10).
- 2026-10-07 — HRT-24 (T33) : adresse dans la clé de regroupement (Q14 point 9), fenêtre qui s'allonge 1, 2, 4, 8, 15 minutes, oubli après 30 minutes sans occurrence ; seuil de rafale à l'affichage confirmé : 5 refus en 2 minutes (Q14 point 11, côté client).
