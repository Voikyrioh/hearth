---
id: BR-DASH-015
domaine: DASH
titre: Seules les interfaces réseau physiques comptent dans le débit de la machine
statut: active
invariant: true
source: revue Stephen HRT-06 round 1, contexts/hearth/conceptions/2026-10-04-fonctionnelle-tableau-de-bord-machine.md (BR-DASH-001, BR-DASH-014), HRT-06
maj: 2026-10-04
---

# BR-DASH-015 — Interfaces réseau comptées dans le débit

## Règle
Le débit réseau d'un échantillon (`net`) additionne les interfaces **physiques** : le trafic des conteneurs, ponts, tunnels, VPN (tun, tap, wg, tailscale) et agrégats (bond) repasse par une interface physique, le compter doublerait le débit. Le bouclage local ne compte jamais.

- Sous Linux, une interface est physique si elle a un périphérique derrière elle : `/sys/class/net/<interface>/device` existe (carte réseau, adaptateur Wi-Fi).
- Si aucune interface physique n'est trouvée, ou hors Linux (mode dev Windows), on se rabat sur toutes les interfaces sauf le bouclage.
- La décision est une fonction pure ; l'observation (nom, présence du périphérique) est faite par la sonde.

## Application (code)
- `crates/hearth-agent/src/domain/machine.rs::{throughput_interfaces, InterfaceKind}` : la décision.
- `crates/hearth-agent/src/infrastructure/system/sysinfo_probe.rs::observe_interface` : l'observation.

## Vérification
- `domain::machine::tests` (physiques seules, repli, bouclage jamais compté) ; `infrastructure::system::sysinfo_probe::tests::the_loopback_is_recognised_by_its_name`.

## Cas limites
- Un nom ne suffit pas à décider (une interface peut s'appeler comme on veut) : la présence du périphérique, pas le nom.
- Machine virtuelle : l'interface virtio a un périphérique, elle compte.

## Règles liées
- BR-DASH-001, BR-DASH-014.

## Historique
- 2026-10-04 — création (HRT-06, revue Stephen round 1 : la règle par préfixes de noms laissait compter tailscale, wg, tun, bond).
