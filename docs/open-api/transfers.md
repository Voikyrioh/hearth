# Transferts reprenables et zone d'attente (HRT-95)

Capacité `service-transfers` annoncée par [`GET /hello`](./hello.md). Quatre routes. Tout est au niveau « gérer les services » (`Access::Services`, quel que soit le rôle, `BR-GRANT-003`) ; le socket d'envoi s'authentifie par son premier message et revérifie le droit pendant l'envoi. Règles `BR-FILES-005`, `013`, `019`, `023`, `BR-MOD-015` ; décision ADR-0048 (transferts), ADR-0045 (opérations), ADR-0055 (lecture d'archive). Types : `hearth_proto::api::transfers`.

**Cette tranche livre l'ENVOI vers la zone d'attente de l'agent.** L'envoi vers un dossier de service et le téléchargement (`BR-BACKUP-026`) viennent ensuite, sur le même socket et la même trame.

## La zone d'attente, et ce que le consommateur en fait

La zone d'attente est le dossier `<données>/transfers` de l'agent (`0700`, hors de tout dossier de service et de tout montage). Un fichier y arrive sous le nom `<id>.part`, devient `<id>.bin` d'un seul geste quand il est complet et vérifié, et n'est **jamais** visible sous ce nom tant qu'il est incomplet. L'identifiant est un ULID en majuscules (26 caractères). L'agent ne garde **aucun nom de fichier** du client.

Un autre lot de l'agent (pack de mods, archive de mods, archive d'un monde) consomme un dépôt complet par le port `DepositZone` (`crates/hearth-agent/src/application/transfers.rs`), jamais par un chemin :

```rust
#[async_trait]
pub trait DepositZone: Send + Sync {
    /// Le dépôt COMPLET de ce compte, en lecture seule et déplaçable (`Read + Seek`).
    /// `NotFound` : inconnu, d'un autre compte, pas encore complet, ou plus le fichier que Hearth a posé.
    /// `Expired` : plus rien (24 heures sans activité).
    async fn take(&self, id: &TransferId, account: &str) -> Result<CompleteDeposit, TransferError>;
    /// Le consommateur a fini (rangé ou refusé) : retire le fichier et sa ligne. Sans erreur si déjà parti.
    async fn release(&self, id: &TransferId) -> Result<(), TransferError>;
}

pub struct CompleteDeposit {
    pub id: TransferId,
    pub size: u64,
    pub tree_digest: [u8; 32],          // SHA-256 de la suite des empreintes de morceaux
    pub reader: Box<dyn ReadSeek>,      // `Read + Seek + Send` : `domain::archive::inspect` juge un zip par son index de fin
}
```

- `account` est l'identifiant technique du compte qui fait la demande (`session.account.id.as_str()`) : un compte ne consomme jamais le dépôt d'un autre.
- `AppState.transfers: Arc<Transfers>` implémente `DepositZone` ; la racine de composition (`app.rs`) le construit avant `Deployments` : un lot qui consomme prend `Arc<dyn DepositZone>` (ou `transfers.clone()`).
- Un consommateur qui plante avant `release` : le dépôt part de lui-même 24 heures après sa dernière activité.
- Le client donne l'identifiant du dépôt (`transfer_id`) au consommateur dans SA requête (par exemple l'installation d'un serveur Minecraft) ; le consommateur appelle `take(id, compte de la requête)`.

## `POST /api/v1/deposits` (acte `FileWrite` `0x16`)

Ouvre un dépôt. Acte d'administration confirmé, **couvert** par l'élévation de 5 minutes. Cible signée : `deposit`. Paramètres signés : le geste `upload`, le dernier nom du fichier et sa taille en octets (`AdminAct::file_deposit`, fonction partagée avec la liaison cliente) ; jamais un chemin ni un octet de contenu.

- **Corps** (`DepositRequest`, avec `reauth` à côté) : `name` (le dernier nom du fichier, 1 à 255 octets, ni `/` ni `\` ni octet nul ni caractère de contrôle, ni `.` ni `..`), `size` (octets, au moins 1).
- **Réponse `201`** (`DepositOpened`) : `transfer_id`, `chunk_bytes` (4 194 304), `window` (8), `expires_at` (RFC 3339, UTC : 24 heures après la dernière activité).
- **Erreurs** : `403` sans le droit ; `422 VALIDATION_ERROR` (`details.field` : `name` ou `size`, `details.reason: not_allowed`, le nom n'est jamais répété) ; `507 NO_SPACE` (`details.missing_bytes`, `details.free_after_bytes` : il restera moins que le plus grand de 1 Gio et de 5 % du disque après l'écriture de TOUTE la taille, `BR-FILES-013` ; rien n'est créé) ; `429 TRANSFER_LIMIT` (`details.limit: open` : 8 dépôts non terminés pour ce compte) ; `422 PATH_REFUSED` (`details.reason: transfers_root_unavailable`).
- Rien n'est envoyé ici : le fichier passe par le socket.

## `GET /api/v1/transfers/{id}` (WebSocket)

Un socket par envoi, jamais le flux commun. Même mise à niveau que `/stream` et que les journaux (`X-Hearth-Api`, quota des connexions en attente d'authentification). Une requête qui n'est pas une mise à niveau reçoit `422` ; un identifiant qui n'est pas un ULID est refusé avant d'accepter la mise à niveau (`404`).

**Bornes** : un message de texte (JSON) fait 4 096 octets au plus ; une trame binaire fait un morceau de données plus 64 octets au plus (4 194 368) ; une trame plus grande ferme le socket. 60 secondes de silence ferment le socket ; l'envoi reste reprenable 24 heures.

**Déroulement**

1. Client : `{"type":"auth","token":"…"}` (comme `/stream`, la preuve de poste facultative comme ailleurs).
2. Client : `{"type":"open"}`.
3. Agent : `{"type":"ready","offset":N,"window":8}` : reprendre à l'octet `N` (toujours un multiple de 4 194 304, sauf la fin). `N` est l'offset DURABLE, relu sur le disque.
4. Client : une trame binaire par morceau, dans l'ordre : `[index u64 big-endian][SHA-256 du morceau : 32 octets][données]`. Le morceau fait exactement 4 194 304 octets, sauf le dernier (le reste du fichier). Au plus 8 morceaux non accusés en vol.
5. Agent : `{"type":"ack","offset":M,"window":8}` après avoir **vérifié ET écrit ET rendu durable** le morceau : `M` octets sont durables, la fenêtre se rouvre.
6. Client : `{"type":"finish","tree_digest":"<64 chiffres hexadécimaux>"}` : le SHA-256 de la suite des empreintes de morceaux, concaténées dans l'ordre (`hearth_proto::api::transfers::chunk_digest`).
7. Agent : `{"type":"done","size":S,"verified":true,"tree_digest":"…"}` : le fichier est dans la zone d'attente, sous son nom définitif. Le socket se ferme (code 1000).

Un dépôt déjà complet qu'on rouvre répond `done` tout de suite. `{"type":"ping","n":1}` répond `{"type":"pong","n":1}`.

**Erreurs** (`{"type":"error","code":"…"}`, puis fermeture 1000, sauf mention) :

| Code | Sens |
|---|---|
| `UNAUTHENTICATED` | Jeton refusé, ou premier message qui n'est pas `auth` (fermeture 1008). |
| `BAD_REQUEST` | Message illisible, trame trop grande ou trop courte, `open` attendu. |
| `TRANSFER_NOT_FOUND` | Inconnu, d'un autre compte, abandonné, repris par un autre socket, ou fichier disparu. |
| `TRANSFER_EXPIRED` | 24 heures sans activité : la partie reçue est supprimée. |
| `TRANSFER_LIMIT` | 4 transferts en cours pour l'agent, ou 2 pour ce compte. |
| `MISSING_RIGHT` | Le compte n'a plus le droit de gérer les services. |
| `NO_SPACE` | Le disque n'a plus la place de la suite (`missing_bytes`, `free_after_bytes`) ; le morceau n'est pas écrit. |
| `CHUNK_CORRUPT` | L'empreinte annoncée n'est pas celle des octets (`expected_index`) : rien n'est écrit ; reprendre renvoie ce morceau. |
| `CHUNK_SEQUENCE` | Hors d'ordre ou de mauvaise taille (`expected_index`). |
| `TREE_MISMATCH` | `finish` avant la fin, ou l'empreinte d'ensemble ne correspond pas : rien n'est publié. |
| `WRITE_FAILED` | L'écriture a échoué sur le serveur (jamais un texte du système). |

**Reprise.** Après une coupure, le client rouvre un socket (`auth`, `open`) : il lit `ready.offset` et repart à ce morceau. **Rien ne repart seul** : la reprise est un geste du client (bouton « Reprendre l'envoi »). Un nouveau socket prend la place de l'ancien (l'ancien reçoit `TRANSFER_NOT_FOUND` à son prochain morceau).

## `GET /api/v1/deposits/{id}`

L'état du dépôt de CE compte, pour le retrouver après une coupure. Une consultation du droit ; seul un refus faute de droit est consigné. `DepositView` : `transfer_id`, `state` (`receiving`, `complete`), `size`, `received_bytes`, `expires_at`, `tree_digest` (hexadécimal, seulement pour un dépôt complet). Erreurs : `404` (inconnu, d'un autre compte, expiré ou abandonné, sans distinction), `410 TRANSFER_EXPIRED` quand la ligne est trouvée juste après ses 24 heures.

## `DELETE /api/v1/transfers/{id}`

Abandonne le dépôt de CE compte et retire la partie reçue. Exception nommée de la liste des actes (`NOT_AN_ACT` : n'efface que la partie reçue du compte appelant, dans la zone d'attente de Hearth) : le droit suffit, aucun mot de passe. Un émetteur encore branché ne peut plus rien écrire. Une partie retirée est consignée sous `service.file.write` (cible `/transfers/{id}/abandon`). `TransferAbandoned` : `{ removed }` (`false` : il n'y avait plus rien, ou le dépôt n'est pas à ce compte).

## Journal d'activité

L'ouverture d'un dépôt (cible `/deposits`) puis la FIN d'un envoi (cible `/transfers/{id}`, quand le dernier morceau est arrivé et vérifié) sont consignées sous `service.file.write` (compte, origine), jamais un nom de fichier, une taille ni un chemin (`BR-FILES-019`). Un refus faute de droit est consigné par la couche d'accès sous la même action.

## Reste à venir (hors tranche)

Envoi vers un dossier de service (« Remplacer », « Garder les deux »), téléchargement avec reprise et « fichier modifié pendant le téléchargement », téléchargement d'une sauvegarde (`BR-BACKUP-026`).
