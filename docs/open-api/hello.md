# GET /api/v1/hello

Identité publique de l'agent. Sert de sonde de vie et de premier contact : le client la lit avant toute authentification, pendant le contact TLS non épinglé (ADR-0005), puis affiche l'empreinte du certificat à confirmer.

- **Authentification** : aucune.
- **Rôle requis** : aucun.
- **Idempotence** : lecture, sans effet de bord.
- **Code** : `crates/hearth-agent/src/entrypoint/http/hello.rs` (cas d'usage `application/hello.rs`).
- **Types** : `hearth-proto::api::hello::HelloResponse`.

## Réponse `200`

Corps JSON direct (pas d'enveloppe `data`).

```json
{
  "product": "hearth",
  "agent_version": "0.1.0",
  "api": { "min": 1, "max": 1 },
  "machine_name": "forge",
  "install_id": "0123456789abcdef0123456789abcdef",
  "managed": false,
  "mac_addresses": ["AA:BB:CC:DD:EE:FF"]
}
```

| Champ | Type | Description |
|---|---|---|
| `product` | texte | Toujours `"hearth"` : permet de reconnaître un agent Hearth (BR-CONN-012). |
| `agent_version` | texte | Version du binaire agent. |
| `api` | `{ min, max }` | Plage de versions d'interface acceptées (`X-Hearth-Api`, ADR-0004). |
| `machine_name` | texte | Nom d'hôte de la machine. |
| `install_id` | texte | 32 caractères hexadécimaux minuscules ; généré à la première exécution, jamais modifié. |
| `managed` | booléen | Installation gérée de l'extérieur : pas de mise à jour automatique. |
| `mac_addresses` | tableau de texte | Adresses MAC des interfaces (`AA:BB:CC:DD:EE:FF`), triées, sans doublon, sans adresse nulle. |

L'empreinte du certificat n'est pas dans le corps : le client la calcule sur le certificat présenté pendant la poignée de main TLS (SHA-256 du DER).

## Erreurs

Toute route inconnue sous `/api/v1` (ou ailleurs) répond `404` au format commun :

```json
{ "error": { "code": "NOT_FOUND", "message": "route inconnue", "details": {} } }
```

## Transport

HTTPS, TLS 1.3 seul, certificat auto-signé (ADR-0005). Port par défaut `7341` (`HEARTH_PORT`, `port` dans `agent.toml`). Une connexion TLS 1.2 est refusée à la poignée de main.
