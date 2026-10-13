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
Un seul modèle, celui qui existait : `#[serde(other)] Unknown`. Ajouté à `UpdateStep`, `UpdateOutcome`, `ErrorCode` (statut 500, jamais émis), `UpgradeTarget`, `SessionNotice`, `ServerMessage` (type de message inconnu), `RoleName`, `OriginKindName`, `OutcomeName`, `HistoryWindow`, `OperationStatus` (HTTP), `ReauthMode`, `AttackModeState`, `AttackModeEnd`, `SessionDevice`, `ChallengePurpose`, `DeviceStatus`. `PasswordRule`, `UsernameProblem` et `Level` n'étaient lues d'aucun réseau : leur `Deserialize` est retiré (ils ne peuvent plus l'être par erreur).

Traitement de la valeur inconnue (jamais prise pour une valeur connue ni pour un succès) :
- étape inconnue : événement transmis tel quel, l'attente du redémarrage ne change pas ; issue inconnue : ni réussite ni échec ; état d'opération inconnu : « résultat inconnu », jamais rejoué ; motif de fin de session inconnu : ni expirée ni révoquée, la reconnexion apprend le vrai motif ; code d'erreur inconnu : message et détails conservés ; état du mode attaque inconnu : ne change pas ce que le client savait (`SecurityWatch`) ; cible de mise à jour inconnue : « le client » (un agent qui nomme une cible inconnue est plus récent que lui) ;
- `ServerMessage::Unknown` accepte tout `type` : `decode_frame` lit donc `security` et `services` d'abord ;
- DTO Tauri : chaque miroir porte `Unknown`, d'où `"unknown"` dans `bindings.ts` (régénéré) ; l'interface reçoit la valeur au lieu d'une erreur (types miroirs, libellés « Inconnu » pour l'audit et l'issue, étape inconnue dite comme « contrôle », issue inconnue en ton d'avertissement).

Côté AGENT, les commandes restent strictes (BR-COMPAT-001) : un rôle, un réglage ou un usage de défi inconnu est un refus `422` typé (`Parsed::read` avant la confirmation, `role_from_wire`, `set_reauth`, `issue_challenge`), jamais une action par défaut. `covered_by_elevation` ne couvre plus qu'un rôle lecture seule : un rôle inconnu demande le mot de passe comme un administrateur.

`API_VERSION` ne change pas : aucune route, aucun champ, aucune valeur existante ne change ; on tolère seulement ce qui arrivera.

Le garde (`crates/hearth-proto/tests/tolerance.rs`) : il lit le code source de `hearth-proto`, relève toute `pub enum` qui dérive `Deserialize` et exige qu'elle figure dans la liste tenue à la main (`enums()`), classée tolérante (doit lire `"__valeur_du_futur__"`, ou un `type` inventé, comme `Unknown`), stricte (commande lue par l'agent : doit la refuser) ou enveloppe du flux ; toute entrée sans énumération correspondante échoue aussi. Il scanne `hearth-proto`, `hearth-link` et le client pour `deny_unknown_fields` et `Deserialize` écrit à la main. Une énumération ajoutée sans repli fait échouer la suite.

## Règles
- BR-COMPAT-001 (créée).
- BR-UPDATE-013 (le flux de progression), BR-GRANT-016 (même principe pour `rights`).
