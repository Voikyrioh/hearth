---
id: FIX-01M7KAB43G8ZFZ2RN8YEQ0QC30
titre: Une recréation refusée laissait les anciens scellés perdus, le journal « réussi » et un enregistrement mixte écrit à moitié (HRT-266)
date_découverte: 2026-11-17
date_correction: 2026-11-17
---

# FIX-01M7KAB43G8ZFZ2RN8YEQ0QC30 : Enregistrement de réglages passés au conteneur, refus de la recréation (HRT-266)

Sévérité : majeur (état et journal faux).

## Symptôme
Quand la recréation d'un service était refusée après l'enregistrement des réglages, l'état revenait mais pas les anciens scellés des réglages secrets passés au conteneur ; le journal disait « réussi » pour des réglages non appliqués ; un échec de remise en place était ignoré en silence ; une demande mixte (fichier + conteneur) écrivait le fichier avant de savoir si le service pouvait être recréé ; la base de la recréation était relue après coup, jamais la révision lue avant d'écrire.

## Reproduction
Enregistrer, pendant qu'une autre recréation tient le service, une demande mêlant un réglage de fichier, un réglage de conteneur et un mot de passe.

## Cause root
`GameSettings::save` écrivait puis demandait la recréation sans la vérifier d'avance ; le retour arrière ne rendait que l'état ; `record` ne connaissait pas le résultat.

## Impacté
`application/game_settings.rs`, `application/service_update.rs` (`precheck`, `update_game(base_rev)`), `entrypoint/http/game_settings.rs`.

## Workaround
Aucun.

## Correction
`ServiceUpdates::precheck` refuse toute la demande avant la moindre écriture ; la révision de la description lue avant d'écrire sert de base à la recréation ; le retour arrière remet état ET anciens scellés ; un échec de remise se trace ; le journal ne compte que ce qui est enregistré (`Outcome::Failed`) ; `RECREATE_NOT_AVAILABLE` dit « Les réglages du fichier sont enregistrés ; ceux passés au conteneur ne le sont pas. » quand la demande était mixte. `Unset` d'un réglage qui a une valeur proposée est refusé (`action`).

## Références
- HRT-266, BR-JEU-035, BR-JEU-036, BR-FICHE-053
