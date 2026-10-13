---
id: BR-GRANT-007
domaine: GRANT
titre: Sans terminal ou avec --yes la réponse vaut non, sauf --services ou HEARTH_SERVICES ; --no-services force non ; install.sh transmet les options
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-droit-de-gerer-les-services.md (BR-GRANT-007), contexts/hearth/conceptions/2026-10-13-adr-droit-de-gerer-les-services-brouillon.md, HRT-77
maj: 2026-10-13
---

# BR-GRANT-007 — Sans terminal ou avec --yes la réponse vaut non, sauf --services ou HEARTH_SERVICES ; --no-services force non ; install.sh transmet les options

## Règle
Sans terminal interactif, ou avec `--yes` (l'option qui valide toutes les questions), la réponse vaut NON. Seule l'option explicite `--services` (ou la variable `HEARTH_SERVICES=manage`) accorde le droit sans question ; `--no-services` (ou `HEARTH_SERVICES=none`) force non. Priorité : `--no-services`, puis `--services`, puis `HEARTH_SERVICES`, puis la question (interactif) ou non. `--services` et `--no-services` ensemble sont refusés à l'analyse de la ligne. `HEARTH_SERVICES` n'accepte QUE `manage` et `none`, exactement (ni `1`, `true`, `yes`, `on`, ni une autre casse, ni un espace : une variable booléenne héritée d'un environnement ne doit jamais accorder un droit qui vaut `root`) ; une variable vide est absente. Toute autre valeur est refusée avant toute écriture, qu'un moteur soit détecté ou non, qu'un compte existe déjà ou non et même quand une option tranche : « HEARTH_SERVICES doit valoir « manage » ou « none ». ». Une option explicite vaut même en interactif (aucune question). Sans moteur détecté, l'option n'accorde rien (BR-GRANT-006). L'option n'est appliquée qu'au premier compte créé ; avec des comptes existants, l'installation le dit et donne la commande.

`deploy/install.sh` transmet `--services` et `--no-services` comme toute option qu'il ne connaît pas : une par ligne, sans développement de motifs ni interprétation (POSIX sh : dash, BusyBox). L'installation gérée (`install --managed`, NixOS) passe par le même flux et pose la même question de la même façon.

## Application (code)
- `crates/hearth-agent/src/entrypoint/cli.rs::InstallArgs::{services, no_services}`
- `crates/hearth-agent/src/entrypoint/install.rs::{services_requested, decide_services}`
- `deploy/install.sh` (transmission, aide)

## Vérification
- `crates/hearth-agent/tests/install_flow.rs::without_a_terminal_or_with_yes_the_answer_is_no_and_nothing_is_asked`
- `crates/hearth-agent/tests/install_flow.rs::the_explicit_option_or_variable_grants_without_asking`
- `crates/hearth-agent/tests/install_flow.rs::no_services_forces_no_over_the_variable_and_over_the_terminal`
- `crates/hearth-agent/tests/install_flow.rs::an_unknown_value_of_the_variable_is_refused_before_any_write`
- `crates/hearth-agent/tests/install_flow.rs::a_managed_installation_asks_the_same_question_the_same_way`
- `crates/hearth-agent/src/entrypoint/cli.rs::tests::install_options_are_parsed`
- `deploy/e2e/scenario.sh` (section « Droit de gérer les services » : `--services`, `--no-services`, `HEARTH_SERVICES`, `install.sh --services`, terminal simulé, installation gérée ; CI seulement)

## Cas limites
- `--yes` valide aussi la question : un octroi par `--yes` seul n'existe pas.
- `sudo` retire l'environnement : `HEARTH_SERVICES` doit être listée dans `--preserve-env` comme `HEARTH_ADMIN_USER`.

## Règles liées
- BR-GRANT-006, BR-GRANT-008

## Historique
- 2026-10-13 — création (HRT-77, ADR-0041).
