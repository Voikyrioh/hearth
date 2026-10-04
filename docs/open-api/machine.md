# GET /api/v1/machine

Identité de la machine : ce que l'agent sait d'elle et qui change rarement. Même contenu que le `machine` du `snapshot` du flux ([stream](./stream.md)).

- **Authentification** : jeton. **Rôle requis** : tous (administrateur et lecture seule voient la même chose, BR-DASH-013).
- **Idempotence** : lecture, sans effet de bord. Version d'interface contrôlée (`X-Hearth-Api`).
- **Code** : `crates/hearth-agent/src/entrypoint/http/metrics.rs` (route), `application/metrics.rs::MetricsService::identity` (cas d'usage), `entrypoint/metrics_wire.rs` (conversion vers le fil).
- **Types** : `hearth-proto::api::machine::MachineResponse`.

## Réponse `200`

```json
{
  "name": "forge",
  "os": { "name": "NixOS", "version": "25.05", "kernel": "6.12.1", "arch": "x86_64" },
  "cpu": { "model": "AMD Ryzen 9 7950X", "physical_cores": 16, "logical_cores": 32, "frequency_mhz": 4500 },
  "memory_total_bytes": 68719476736,
  "disks": [
    { "name": "/dev/nvme0n1p2", "mount": "/", "fs": "ext4", "total_bytes": 1000204886016, "removable": false }
  ],
  "gpus": [ { "name": "NVIDIA GeForce RTX 4090", "memory_total_bytes": 25769803776 } ],
  "capabilities": { "gpu": true, "temps": true }
}
```

| Champ | Description |
|---|---|
| `name` | Nom d'hôte. |
| `os`, `cpu` | Système (version et noyau absents si le système ne les donne pas) et processeur (`physical_cores` et `frequency_mhz` absents si illisibles). |
| `memory_total_bytes` | Mémoire vive totale, en octets. |
| `disks` | Disques montés (identité **en cache, relue au plus toutes les 30 s** ; la liste à la seconde est celle de chaque échantillon, BR-DASH-012) : pseudo-systèmes de fichiers et doublons exclus. |
| `gpus` | Cartes graphiques mesurables, **liste vide** sans carte (BR-DASH-005). Une carte vue une fois reste listée (et `capabilities.gpu` vrai) même pendant une relance de `nvidia-smi`, seules ses mesures deviennent absentes. `memory_total_bytes` absent si la carte ne l'expose pas. |
| `capabilities` | `gpu` : au moins une carte mesurable ; `temps` : au moins une sonde de température exposée par le système (BR-DASH-006). |

Une machine sans carte graphique ou sans sonde n'est jamais une erreur : capacité fausse, liste vide. Si la sonde ne répond pas (2 s), l'identité du cache est servie, périmée au besoin ; `500` seulement si elle n'a jamais pu être lue.

## Erreurs

- Authentification, version : voir [sessions](./sessions.md) et les conventions de l'[index](./INDEX.md).
