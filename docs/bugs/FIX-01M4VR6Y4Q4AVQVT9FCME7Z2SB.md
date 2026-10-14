---
id: FIX-01M4VR6Y4Q4AVQVT9FCME7Z2SB
titre: Une étape, une issue ou un autre code inconnu d'un agent plus récent faisait rejeter ou perdre le message entier
date_découverte: 2026-10-14
date_correction: 2026-10-14
---

# FIX-01M4VR6Y4Q4AVQVT9FCME7Z2SB : valeurs inconnues du protocole (BR-COMPAT-001)

Sévérité : élevée (avant la première version publique : après elle, toute valeur ajoutée casserait les clients installés). Zones : `crates/hearth-proto` (énumérations), `crates/hearth-link` (lecture des trames, `task.rs`, `attempt.rs`, `pending_ops.rs`), `apps/desktop/src-tauri` (DTO), agent (commandes), interface (types miroirs).

## Symptôme
Un agent plus récent que le client (un utilisateur qui n'a pas mis son client à jour, pendant la mise à jour de l'agent elle-même) envoie une étape (`UpdateStep`) ou une issue (`UpdateOutcome`) inconnue : la trame du flux est rejetée au décodage, la liaison la prend pour une « trame sans intérêt » et l'ignore. Le client ne voit plus la progression, jamais la fin, et ne sait pas que le redémarrage de l'agent est annoncé. Même sort pour toute autre énumération sans repli : un code d'erreur (`ErrorCode`) inconnu faisait perdre le message d'erreur du flux et traiter le corps d'une réponse d'erreur comme « pas un agent » ; un motif de fin de session inconnu faisait ignorer la fin de la session ; un rôle, un état du mode attaque, un état d'opération (`OperationStatus`) ou une cible de mise à jour (`UpgradeTarget`) inconnus faisaient rejeter la réponse entière.

## Reproduction
`crates/hearth-link/tests/unknown_agent_values.rs` (agent « du futur » : TLS 1.3 + WebSocket qui joue des trames brutes contre le vrai `HttpTransport`) et `crates/hearth-proto/tests/tolerance.rs`. Avant la correction : `"step":"etape_du_futur"` ⇒ `Frame::Other`, la progression est perdue.

## Cause root
Seules les énumérations ajoutées depuis HRT-61 portaient une variante `#[serde(other)] Unknown` (services, journaux, opérations, `UpdateReason`). Les autres, plus anciennes (mise à jour de l'agent, comptes, sécurité, audit, sessions, erreurs), étaient fermées : `serde` rejette alors la valeur et le message entier avec elle. Rien n'empêchait d'en ajouter une nouvelle fermée, et la règle (« chaque côté tolère ») n'était écrite que dans les commentaires d'un module.

## Impacté
Inventaire complet des énumérations de `hearth-proto` : 50 dont 47 lues du fil (40 lues du côté client ou des deux côtés, 4 commandes lues par l'agent, 3 enveloppes du flux) et 3 qui ne sortent d'aucun réseau ; 17 de celles lues du fil rejetaient la valeur inconnue (voir « Correction »), 23 avaient déjà leur repli. Aucune structure ne refusait un champ en trop (aucun `deny_unknown_fields` ni `Deserialize` à la main hors des fiches de jeu de l'agent, qui sont des fichiers locaux). `GET /agent/update`, `GET /agent/update/last`, le sujet `update` du flux, `GET /security`, `GET /accounts`, `GET /audit`, `GET /operations/{id}`, les réponses d'erreur.

## Workaround
Aucun (mettre le client à jour ; impossible pendant la mise à jour de l'agent).

## Correction
Un seul modèle, celui qui existait : `#[serde(other)] Unknown`. Ajouté à `UpdateStep`, `UpdateOutcome`, `ErrorCode` (statut 500 si on le demande, jamais émis), `UpgradeTarget`, `SessionNotice`, `ServerMessage` (type de message inconnu), `RoleName`, `OriginKindName`, `OutcomeName`, `HistoryWindow`, `OperationStatus` (HTTP), `ReauthMode`, `AttackModeState`, `AttackModeEnd`, `SessionDevice`, `ChallengePurpose`, `DeviceStatus`. `PasswordRule`, `UsernameProblem` et `Level` n'étaient lues d'aucun réseau : leur `Deserialize` est retiré (ils ne peuvent plus l'être par erreur).

Traitement de la valeur inconnue (jamais prise pour une valeur connue ni pour un succès) :
- étape inconnue : événement transmis tel quel, l'attente du redémarrage ne change pas ; issue inconnue : ni réussite ni échec ; état d'opération inconnu : « résultat inconnu », jamais rejoué ; motif de fin de session inconnu : ni expirée ni révoquée, la reconnexion apprend le vrai motif ; code d'erreur inconnu : message et détails conservés ; état du mode attaque inconnu : ne change pas ce que le client savait (`SecurityWatch`) ; cible de mise à jour inconnue : « le client » (un agent qui nomme une cible inconnue est plus récent que lui) ;
- `ServerMessage::Unknown` accepte tout `type` : `decode_frame` lit donc `security` et `services` d'abord ;
- DTO Tauri : chaque miroir porte `Unknown`, d'où `"unknown"` dans `bindings.ts` (régénéré) ; l'interface reçoit la valeur au lieu d'une erreur (types miroirs, libellés « Inconnu » pour l'audit et l'issue, aucune étape marquée pour une étape inconnue (l'écran dit « en cours »), issue inconnue en ton d'avertissement).

Côté AGENT, les commandes restent strictes (BR-COMPAT-001) : un rôle, un réglage ou un usage de défi inconnu est un refus `422` typé (`Parsed::read` avant la confirmation, `role_from_wire`, `set_reauth`, `issue_challenge`), jamais une action par défaut. `covered_by_elevation` ne couvre plus qu'un rôle lecture seule : un rôle inconnu demande le mot de passe comme un administrateur.

`API_VERSION` ne change pas : aucune route, aucun champ, aucune valeur existante ne change ; on tolère seulement ce qui arrivera.

Le garde (`crates/hearth-proto/tests/tolerance.rs`) : il lit le code source de `hearth-proto`, relève toute `pub enum` qui dérive `Deserialize` et exige qu'elle figure dans la liste tenue à la main (`enums()`), classée tolérante (doit lire `"__valeur_du_futur__"`, ou un `type` inventé, comme `Unknown`), stricte (commande lue par l'agent : doit la refuser) ou enveloppe du flux ; toute entrée sans énumération correspondante échoue aussi. Il scanne `hearth-proto`, `hearth-link` et le client pour `deny_unknown_fields` et `Deserialize` écrit à la main. Une énumération ajoutée sans repli fait échouer la suite.

## Corrections de la revue de la PR 92 (2026-10-14)
- **B1, interface** : un mode attaque `unknown` n'est plus affiché « Actif », n'ouvre plus de bandeau permanent, ne propose plus « Désactiver » : libellé « Inconnu », phrase neutre, aucun bouton ; l'alerte reste visible sans « Activer » ; une fin automatique n'est annoncée que d'un mode su en vigueur. Même contrôle écran par écran pour les autres `"unknown"` de `bindings.ts` : rôle (« Rôle inconnu de ce client », plus « lecture seule », aucun changement de rôle proposé, bloc `role_unknown` du mode attaque), réglage de confirmation (ni coché ni modifiable), fin de session (`Reason::Ended`), tables de libellés exhaustives (test `unknownLabels.test.ts`).
- **B2, agent** : `UpdateStep` et `UpdateOutcome` sont aussi le format disque de l'agent. Corrigé : lecture disque par `Kept<T>` (valeur étrangère gardée et réécrite à l'identique, dans `last.json`, `state.json` et `phase.json`), issue étrangère consignée par une entrée dédiée (ni réussite ni échec de la mise à jour), émise aux clients comme `unknown`. Dire que l'agent « n'émet jamais Unknown » était FAUX pour ces deux types ; la vérité : il ne l'émet que pour une valeur qu'il ne sait pas lire (BR-COMPAT-001 point 7).
- **Avant la première version publique** : repli par statut HTTP pour un code d'erreur inconnu (cinq interprètes + connexion, point 8) ; fin de session au motif inconnu = déconnexion neutre (point 9) ; règle unique pour les listes et les nombres (`hearth_proto::lenient`, point 10) ; garde renforcé (dérivations repliées, visibilités, indentation, `cfg_attr`, dossier absent, lectures personnalisées, `impl` génériques, devenir d'`Unknown`) ; contrôle côté interface des tables de libellés.
- **Garde éprouvé par mutation** (défaut introduit puis retiré, le garde a échoué à chaque fois) : dérivation sur plusieurs lignes, `pub(crate)`, enum privée, enum indentée dans un module en ligne, `cfg_attr(derive(Deserialize))`, `serde(other)` retiré, `deserialize_with` non déclaré, `try_from` non déclaré, `impl<'a, T> Deserialize<'a>` écrit à la main, `deny_unknown_fields`, bras `Unknown => connue`, motif `Connue | Unknown`, liste qui rejette tout, nombre borné en silence, mesure nulle lue zéro ; côté interface : entrée `unknown` retirée d'une table, union `"unknown"` non déclarée, `modeOn` revenu à `!== "off"`.
- Reste à traiter (tickets) : `UpdateReason` relue puis réécrite par un ancien agent ; `Phase` du marqueur de reprise reste fermée ; affichage du nombre d'éléments illisibles ; détection de trou sur `rev` des services ; tracer un message connu mais illisible ; agent « du futur » contre le gestionnaire complet ; message d'incompatibilité quand la cible est inconnue et que le client est à jour ; refus local d'un rôle, réglage ou issue `unknown` avant l'envoi ; garde sur les `f32` non finis côté agent.

## Règles
- BR-COMPAT-001 (créée).
- BR-UPDATE-013 (le flux de progression), BR-GRANT-016 (même principe pour `rights`).
