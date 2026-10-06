---
id: FIX-01M46G7Y2DW32G1D190GE4MA7A
titre: La troncature des adresses MAC pouvait écarter la carte physique
date_découverte: 2026-10-05
date_correction: 2026-10-05
---

# FIX-01M46G7Y2DW32G1D190GE4MA7A : La troncature des adresses MAC pouvait écarter la carte physique

## Symptôme
Un serveur qui annonce plus de 16 interfaces (hôte Docker) pouvait être enregistré sans l'adresse de sa carte réseau physique : le futur réveil réseau n'aurait eu aucune adresse utilisable.

## Cause root
`crates/hearth-link/src/domain/book.rs::check_mac_addresses` gardait les 16 premières adresses valides dans l'ordre annoncé ; l'agent les annonce triées, et les `02:42:…` de Docker passent devant les cartes réelles.

## Impacté
Carnet des serveurs depuis HRT-10, pour toute machine qui annonce plus de 16 interfaces.

## Workaround
Aucun.

## Correction
Les adresses « universelles » (bit « administrée localement » à zéro : cartes réelles) passent avant les adresses administrées localement (conteneurs, ponts, machines virtuelles) ; chaque groupe garde l'ordre annoncé ; doublons écartés ; au plus 16. Test `domain::book::tests::the_physical_card_survives_the_truncation_behind_forty_virtual_ones`.

## Références
- Ticket : HRT-12 (suivi de la review de HRT-10, PR #12)
- BR : BR-CONN-008
