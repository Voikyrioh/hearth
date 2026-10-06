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

## Réalisation (HRT-17)

Les écarts avec l'esquisse ci-dessus, décidés à la réalisation (voir ADR-0014) : le superviseur est une copie de l'ancien binaire (`hearth-agent update-supervise`), lancée par `systemd-run` hors du groupe de contrôle du service ; les fichiers sont dans `<données>/update/` (pas `/tmp`, pas `/var/run`) ; la demande porte aussi la somme (`sha256`) ; le fichier est téléchargé en mémoire et vérifié (somme puis signature) avant toute écriture ; le résultat est `update/last.json` (pas `last_update.json`) et se lit par `GET /agent/update` et `GET /agent/update/last` ; le contrôle exige la nouvelle version **et** le même certificat. Contrat : `docs/open-api/agent-update.md`.

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

## Amendement du 2026-10-05 : amendée par ADR-0017 (HRT-16, mise à jour du client)

L'histoire ci-dessus reste écrite telle quelle ; voici ce que HRT-16 a changé et ce qui reste vrai.

**Ce qui change pour le CLIENT** (ADR-0017) :
- le flux n'est pas « hébergé configurablement » : l'adresse est une CONSTANTE de la compilation, `https://github.com/Voikyrioh/hearth/releases/latest/download/latest.json` (GitHub Releases du dépôt public) ;
- le format du manifeste est celui du greffon de mise à jour de Tauri (`version`, `notes`, `pub_date`, `platforms.windows-x86_64.{signature,url}`), pas l'objet `{client, agent}` de l'esquisse ci-dessus, que le greffon ne lit pas ;
- l'installateur est téléchargé en mémoire, sa signature (clé publique du fichier `apps/desktop/src-tauri/update-key.pub`, embarquée) est vérifiée avant toute écriture, et la version annoncée doit être celle du commentaire signé (`requireSignedVersion`) ;
- le client lit lui-même le flux ; l'agent n'a pas à le faire.

**Ce qui reste vrai pour l'AGENT** (ADR-0014) : la clé publique de l'agent est embarquée dans l'agent (`crates/hearth-agent/update-key.pub`, distincte de celle du client), il vérifie somme et signature avant d'écrire, le client lui transmet `{ version, url, signature, sha256 }`, superviseur et retour arrière en 60 s. La section « Flux de versions » ci-dessus décrit une cible, non un fichier existant pour l'agent.

**Où l'agent trouvera sa cible** : tranché le 2026-10-06 par l'ADR-0021 (section `agent` du `latest.json` du client, lue par la même requête, jamais choisie par l'interface). Ce qui suit est l'état des lieux qui a conduit à cette décision. Contraintes établies par HRT-16 : le greffon du client ne rend le manifeste que lorsqu'une version plus récente du CLIENT existe (donc une section `agent` du même `latest.json` n'est pas lisible quand le client est à jour) ; `releases/latest` est unique, une release de l'agent seule ne doit pas retirer l'entrée `windows-x86_64` ; `client-manifest` réécrit le fichier entier. Pistes : un fichier `agent.json` dans la même release, lu par une requête supplémentaire du client (à rapprocher de la règle d'une requête par jour), ou une section `agent` fusionnée par `client-manifest`. Voir ADR-0017, décision 8.
