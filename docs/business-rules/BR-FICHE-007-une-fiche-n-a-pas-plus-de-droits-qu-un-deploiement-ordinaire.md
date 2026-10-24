---
id: BR-FICHE-007
domaine: FICHE
titre: Une fiche n'a pas plus de droits qu'un déploiement ordinaire : le format est fermé et neuf invariants refusent le reste avec un motif typé
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-007), contexts/hearth/conceptions/2026-10-13-technique-fiches-de-jeu.md, HRT-113
maj: 2026-10-13
---

# BR-FICHE-007 — Une fiche n'a pas plus de droits qu'un déploiement ordinaire : le format est fermé et neuf invariants refusent le reste avec un motif typé

## Règle
Le format est FERMÉ : tout champ inconnu, à n'importe quel niveau, est refusé (`ChampInconnu`). Il n'a aucun champ pour un mode privilégié, une capacité, le réseau ou les processus de l'hôte, un périphérique, une option de sécurité, le socket du moteur, un chemin du serveur, une commande ou un argument saisi, un script, une expression régulière ou un identifiant de compte. Ce qui reste exprimable est refusé par un invariant NOMMÉ : I1 image sans version précise, dépôt invalide, port sous 1024, `published` sans empreinte ; I2 cible ou chemin de dossier invalide (alphabet fermé `[A-Za-z0-9._-]` par composant : jamais `:`, `,`, `=` ni espace, rien qui ait un sens dans une description de montage), dossier inconnu ; I3 argument non littéral, valeur hors de l'alphabet fermé (le refus cite le caractère, jamais la valeur), secret en argument, valeur dérivée de plus d'un réglage ou dont le préfixe, le suffixe ou la cible sortent de l'alphabet du réglage cible (la cible est un texte, jamais un entier), ordre envoyé au jeu de plus de 64 caractères ou non ASCII ; I4 aucun genre d'étape qui exécute une commande ; I5 adresse non `https` ; I6 repère de journal ou de version qui n'est pas un littéral borné ; I7 console distante publiée ou au mot de passe non généré ; I8 contrat accepté d'avance, ou lié à autre chose qu'une variable d'environnement (un contrat lié à un fichier serait accepté puis jamais écrit) ; I9 bornes (256 Kio, 400 réglages, 16 ports, 8 dossiers, 6 canaux, 6 étapes d'arrêt, 20 minutes, libellés bornés).

## Application (code)
- `crates/hearth-agent/src/domain/catalog/loading/invariants.rs::{check_bounds, check_fiche_file, check_cross}`
- `crates/hearth-agent/src/domain/catalog/loading/error.rs::Motif (un motif par invariant, `invariant()` le nomme)`
- `crates/hearth-agent/src/domain/catalog/loading/schema/ (`deny_unknown_fields` à tous les niveaux)`
- `crates/hearth-agent/src/domain/catalog/loading/loader.rs::load`

## Vérification
- `crates/hearth-agent/src/domain/catalog/tests_fiches.rs::format_closed_an_unknown_field_is_refused_at_every_level`
- `crates/hearth-agent/src/domain/catalog/tests_fiches.rs::i1_one_way_closed_plan_refuses_image_and_port_traps`
- `crates/hearth-agent/src/domain/catalog/tests_fiches.rs::i2_every_mount_is_a_managed_folder_no_host_path_exists`
- `crates/hearth-agent/src/domain/catalog/tests_fiches.rs::i3_no_text_reaches_an_interpreter_arguments_orders_and_derived_values`
- `crates/hearth-agent/src/domain/catalog/tests_fiches.rs::i4_nothing_is_executed_no_step_kind_runs_a_command`
- `crates/hearth-agent/src/domain/catalog/tests_fiches.rs::i5_no_address_is_contacted_https_only`
- `crates/hearth-agent/src/domain/catalog/tests_fiches.rs::i6_neither_regular_expression_nor_template_signs_and_versions_are_literals`
- `crates/hearth-agent/src/domain/catalog/tests_fiches.rs::i7_a_remote_console_is_never_published_and_its_password_is_generated`
- `crates/hearth-agent/src/domain/catalog/tests_fiches.rs::i8_a_contract_is_accepted_only_by_a_gesture`
- `crates/hearth-agent/src/domain/catalog/tests_fiches.rs::i9_a_fiche_cannot_exhaust_the_agent_or_the_screen_bounds`
- `crates/hearth-agent/src/domain/catalog/architecture_guard.rs::i4_i5_the_catalog_domain_has_no_io_no_process_no_network_no_async`
- `crates/hearth-agent/src/domain/catalog/tests_translate.rs::property_every_accepted_fiche_and_valid_answers_give_a_closed_description`

## Cas limites
- Le format borne ce que Hearth DEMANDE au moteur, pas ce que l'image fait une fois lancée ni ce qu'elle télécharge (BR-FICHE-036).
- Le validateur du déploiement ordinaire est branché (HRT-101) : toute traduction est acceptée par `OrdinaryDeployment` (`tests_translate.rs::property_every_accepted_fiche_and_valid_answers_give_a_closed_description`, 700 cas par fiche, graine fixe) ; ses refus gardent le même code que le formulaire (`the_ordinary_deployment_validator_refuses_what_a_hand_made_deployment_refuses`).

## Règles liées
- BR-FICHE-005, BR-FICHE-011, BR-JEU-040

## Historique
- 2026-10-13 — création (HRT-113, format de fiche version 2, domaine pur).
