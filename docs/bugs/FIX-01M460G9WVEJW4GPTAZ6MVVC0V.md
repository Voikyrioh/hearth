---
id: FIX-01M460G9WVEJW4GPTAZ6MVVC0V
titre: La désinstallation avec purge laissait des temporaires d'écriture (clé privée comprise) et le dossier de données
date_découverte: 2026-10-05
date_correction: 2026-10-05
---

# FIX-01M460G9WVEJW4GPTAZ6MVVC0V : La désinstallation avec purge laissait des temporaires d'écriture (clé privée comprise) et le dossier de données

## Symptôme
Après `uninstall --purge`, un temporaire d'écriture de l'identité (`key.pem.<pid>.<n>.tmp`, qui peut contenir la clé privée), un `.hearth-agent.new-<pid>` ou un `hearth-agent.service.new` restaient, et le dossier de données n'était pas retiré (non vide).

## Cause root
La liste de purge ne portait que les noms définitifs ; les noms temporaires n'étaient connus que de leur écrivain.

## Impacté
Installation et désinstallation depuis HRT-15.

## Workaround
Aucun.

## Correction
Les noms sont dans `domain/install/files.rs` (source unique) ; `Installer::remove_known_data` et `remove_binary_temporaries`, `Systemd::remove` retirent les temporaires connus. Test `a_purge_also_removes_the_temporaries_and_the_update_directory_of_hearth`.

## Références
- Ticket : HRT-17 (suivi de la review de HRT-15, PR #11)
- BR : BR-INSTALL-011 ; code : `crates/hearth-agent/src/application/install.rs` (marqueur `FIX:01M460G9WVEJW4GPTAZ6MVVC0V`)
