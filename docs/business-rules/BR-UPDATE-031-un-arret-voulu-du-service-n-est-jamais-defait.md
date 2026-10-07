---
id: BR-UPDATE-031
domaine: UPDATE
titre: Un arrêt voulu du service n'est jamais défait par la mise à jour
statut: active
invariant: true
source: HRT-27, ADR-0014 (amendement du 2026-10-07)
maj: 2026-10-07
---

# BR-UPDATE-031 : Un arrêt voulu du service n'est jamais défait par la mise à jour

## Règle
Rien ne relance le service à part le superviseur, et le superviseur ne relance que ce qu'il a lui-même arrêté. Hors d'une mise à jour, aucun mécanisme n'existe : un `systemctl stop hearth-agent` est définitif. Pendant le contrôle du nouvel agent (BR-UPDATE-015), le superviseur lit l'état du service (`systemctl show --property=ActiveState`, port `ServiceManager::state`) à chaque tour où le nouvel agent ne s'est pas encore annoncé : un service **arrêté** (`inactive`) est un arrêt demandé par quelqu'un d'autre que lui (il n'arrête jamais le service pendant le contrôle, et `Restart=always` de l'unité du service ne laisse jamais un échec à l'état arrêté : un binaire qui plante est `activating (auto-restart)`). Dans ce cas, **rien n'est défait** : ni retour arrière, ni redémarrage. Le superviseur journalise « service arrêté à la main », sort en 0 sans rien conclure, et garde le travail, l'ancien binaire, la copie de la base et son marqueur (dont le compteur de reprises est remis à zéro : un arrêt voulu n'est pas une panne du superviseur, deux arrêts voulus de suite n'épuisent pas ses reprises) ; au prochain démarrage de l'agent, BR-UPDATE-028 reprend (un superviseur de reprise contrôle le binaire en place, ou remet l'ancien).

La distinction est donc : **arrêté** = voulu ; **en échec, en train de se relancer, ou muet** = panne. Un état que systemd ne dit pas (`unknown`) n'est jamais pris pour un arrêt voulu.

## Application (code)
- `crates/hearth-agent/src/application/update_supervisor.rs::Supervisor::{check, pause}`.
- `crates/hearth-agent/src/infrastructure/service/systemd.rs::{Systemd::state, parse_active_state}` ; port `application/ports/service_manager.rs::ServiceState`.

## Vérification
- `tests/update_supervisor_resume.rs::a_service_stopped_by_hand_during_the_check_is_never_started_again_nor_rolled_back`.
- `infrastructure::service::systemd::tests::the_service_state_tells_a_stop_from_a_service_that_keeps_failing`, `the_state_is_read_with_systemctl_show_and_a_silent_system_is_unknown`.
- Sur un vrai systemd : `deploy/e2e/double-failure-systemd.sh` (lancé à la main sous systemd réel, dernière passe le 2026-10-07 sous WSL2 systemd 255 ; pas en CI) (`systemctl stop hearth-agent` pendant le contrôle). Non couvert par la CI : seuls les tests avec service simulé le sont.

## Cas limites
- Un arrêt demandé **après** la conclusion, ou sans mise à jour en cours : aucun superviseur, rien à défaire.
- Un arrêt demandé entre deux tours du contrôle, suivi d'un redémarrage voulu avant le tour suivant : indiscernable d'un service qui tourne ; sans effet.
- Un nouveau binaire qui s'arrête de lui-même avec le code 0 : avec `Restart=always` il est relancé, jamais `inactive` : traité comme muet (retour arrière).
- Le superviseur tué pendant que l'administrateur a arrêté le service, puis relancé : la reprise reprend à `Checking`, voit le service arrêté au premier tour et se met en pause de même.

## Règles liées
- BR-UPDATE-015, BR-UPDATE-028, BR-UPDATE-030, ADR-0014

## Historique
- 2026-10-07 : création (HRT-27).
