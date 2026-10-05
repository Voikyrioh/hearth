---
id: FIX-01M460G9AE6AFPSTRF4ZD872QB
titre: Le retour en arrière d'une installation ne remettait pas l'activation au démarrage, et une erreur de lecture valait « pas activé »
date_découverte: 2026-10-05
date_correction: 2026-10-05
---

# FIX-01M460G9AE6AFPSTRF4ZD872QB : Le retour en arrière d'une installation ne remettait pas l'activation au démarrage, et une erreur de lecture valait « pas activé »

## Symptôme
Une installation qui échoue sur un service arrêté et désactivé laissait le service activé au démarrage ; et si l'état d'activation était illisible, il était lu comme « désactivé » et le retour en arrière désactivait un service qui l'était.

## Cause root
`Installer::rollback` ne rétablissait que l'unité et l'état actif/arrêté, jamais `enable`/`disable` ; `is_enabled().unwrap_or(false)` masquait l'erreur (même défaut que FIX-01M45V0PZB5TRE3A7KQHAHJNXD).

## Impacté
Installation et désinstallation depuis HRT-15.

## Workaround
Aucun.

## Correction
Port `ServiceManager::{is_enabled, enable}` ; l'activation d'avant est lue avant toute écriture (une erreur arrête l'installation sans rien modifier) et rétablie au retour en arrière. Tests `a_failure_puts_the_start_at_boot_setting_back_as_it_was` et `systemd::tests::enabled_follows_the_unit_and_enable_does_not_start`.

## Références
- Ticket : HRT-17 (suivi de la review de HRT-15, PR #11)
- BR : BR-INSTALL-008 ; code : `crates/hearth-agent/src/application/install.rs` (marqueur `FIX:01M460G9AE6AFPSTRF4ZD872QB`)
