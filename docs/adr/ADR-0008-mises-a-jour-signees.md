---
id: ADR-0008
titre: Mises à jour agent signées (minisign), superviseur et retour arrière
type: securite
statut: acceptée
date: 2026-10-04
portee: projet
remplace: —
liens: [conception technique 2026-10-04, sections 2 et 7]
---

# ADR-0008 — Mises à jour agent signées (minisign), superviseur et retour arrière

## Contexte

Agent s'auto-met-à-jour depuis un flux de versions (manifeste + binaire). Attaque : binaire malveillant injecté. Solution : signature cryptographique (minisign, Ed25519), vérifiée avant échange des binaires. Supervisor (copie ancien binaire) orchestre swap et rollback en 60 s si nouveau ne démarre pas.

## Décision

Mises à jour signées avec minisign (Ed25519). Clé publique compilée dans client et agent. Flux de versions (JSON statique) contient : version agent/client, notes, adresses téléchargement, signatures minisign, plage API support. Superviseur : ancien binaire lancé détaché après mise à jour, arrête l'ancien, échange les binaires, redémarre le nouveau, vérification 60 s, rollback en cas d'erreur.

## Comment l'appliquer

### Flux de versions

Manifeste `latest.json` (hébergé configurablement, statique) :
```json
{
  "client": {
    "version": "1.0.0",
    "notes": "bugfix réseau",
    "url": "https://example.com/hearth-1.0.0.msi",
    "signature": "<minisign base64>"
  },
  "agent": {
    "version": "1.0.0",
    "notes": "amélioration mesures GPU",
    "url": "https://example.com/hearth-agent-1.0.0",
    "signature": "<minisign base64>",
    "api": { "min": 1, "max": 1 }
  }
}
```

### Processus agent

Client envoie au serveur : `POST /api/v1/agent/update { version, url, signature }`.

Serveur :
1. Vérifie signature (clé publique compilée) : `minisign -Vm <binaire> -P <pubkey>` ← signature.
2. Télécharge `url`.
3. Vérifie à nouveau la signature du fichier téléchargé.
4. Lance superviseur détaché : `hearth-supervisor --old-bin /usr/local/bin/hearth-agent --new-bin /tmp/hearth-agent-new --lock /var/run/hearth.lock`.
5. Agent courant répond `202 Accepted` avec opération en cours.

### Superviseur

Script shell ou Rust minimal `deploy/supervisor` :
1. Attend verrou `/var/run/hearth.lock` libre (agent a arrêté le service).
2. `systemctl stop hearth-agent`.
3. Copie `/tmp/hearth-agent-new` → `/usr/local/bin/hearth-agent.new`.
4. `systemctl start hearth-agent` (pointe vers nouveau).
5. Boucle : ping `GET /hello` pendant 60 s, sinon `rollback` (restaure ancien, restart, écrit log).
6. Écrit résultat dans `/var/lib/hearth/last_update.json`.

### Client consulte statut

`GET /api/v1/agent/update` → `{ current, managed, in_progress, last: { version, outcome, at } }`.

## Quand NE PAS l'appliquer / limites

- Offline update : si agent jamais accède Internet (machine air-gap), mise à jour manuelle ou flux interne.
- Rollback implicite (60 s non responsif) : peut masquer vrai problème lent à démarrer. User doit relancer manuellement après 60 s si nécessaire.
- Signature leak (clé publique dans client) : non critique (public key pour vérification seulement), clé privée gardée sûre.

## Alternatives rejetées

- **Pas de vérification signature** : vulnérable injection malveillant.
- **Vérification client seulement** : agent reçoit binaire non-vérifié, risque.
- **OAuth revocation** : complexité infra, nécessite serveur stateful.
- **SCP manuel supervisé** : erreur humaine, frictions.

## Conséquences

- Déploiement : clés minisign générées, clé publique compilée au build.
- Hébergement versions : manifestez JSON + binaires signés, adresse configurable au déploiement.
- Révision : tout changement de signature = nouvelle version.

## Références

- minisign : https://jedisct1.github.io/minisign/
- Ed25519 : https://ed25519.cr.yp.to/
- Tauri updater (même pattern) : https://docs.rs/tauri-plugin-updater/latest/tauri_plugin_updater/
- systemd service : https://www.freedesktop.org/software/systemd/man/systemd.service.html
- ADR-0008 globale (sécurité) : `orga-global/docs/adr/ADR-0008-*.md`
