---
id: ADR-0029
titre: Alerte et mode attaque dans le client : état tenu côté Rust et rejoué, poste lu par l'agent, confirmation avec mot de passe, notifications à priorité et réglage à part, pas de repli sans clé
type: architecture
statut: acceptée
date: 2026-10-07
portee: projet
remplace: —
liens: [ADR-0013, ADR-0016, ADR-0023, ADR-0024, ADR-0025, BR-TRUST-009, BR-TRUST-010, BR-TRUST-029, BR-TRUST-033, BR-RESIL-015, HRT-26]
---

# ADR-0029 : Alerte et mode attaque dans le client

## Contexte

L'agent livre l'alerte (HRT-24) et le mode attaque (HRT-25) : message `security` du flux, `GET /security`, `PUT /security/attack-mode` (mot de passe ET preuve de clé, contrat commun des actes d'usage `0x05` depuis ADR-0033). Le client doit les montrer partout (bandeaux, marque du serveur), laisser un administrateur activer et désactiver, notifier Windows, et dire clairement quand ce PC ne peut pas agir. Contraintes : la WebView ne voit jamais la clé (ADR-0023), un événement n'est qu'un signal (ADR-0013 point 3), une commande = une action typée (ADR-0016).

## Décision

1. **L'état de sécurité est tenu côté Rust** (`security/book.rs::SecurityBook`) : dernier état de chaque serveur, numéro de séquence qui croît strictement, rejoué à l'abonnement (`list_security_states`) ; l'interface écarte tout état dont `seq` n'est pas supérieur au dernier connu. `link://security` est un signal portant l'état, jamais une source unique.
2. **« Ce poste est reconnu » se lit chez l'agent.** Le message du flux ne dit rien de la session de ce poste : `GET /security` rend `device` (`proven` ou `none`), lu au retour du lien par la commande `get_security` ; l'événement porte `device: unknown` tant qu'aucune lecture n'a eu lieu (l'interface garde la dernière lecture). Le client ajoute seulement un booléen, `keyAtHand` (ce PC garde une clé au coffre), jamais la clé. Sans clé au coffre, `set_attack_mode` rend `NotRecognized` sans aucun appel (pas même un défi) ; cette règle est la même pour activer et désactiver.
3. **Confirmation par une fenêtre à mot de passe** (`FormDialog`, pas `ConfirmDialog`) : le ticket demandait `ConfirmDialog`, mais Q16 exige le mot de passe actuel dans la confirmation ; le champ est vidé après chaque envoi. La fenêtre est portée par le gabarit du serveur (un état de store, `security.dialog`) pour que le bandeau d'alerte et la page Sécurité l'ouvrent au même endroit.
4. **Les bandeaux sont posés par `ServerLayout`, hors de `StaleSurface`** : une alerte ne s'estompe pas quand le lien tombe (« Dernier état connu à {heure} »). Trois raisons d'indisponibilité, une affichée, dans l'ordre : Lecture seule, agent trop ancien, poste sans clé ; l'interface ne les arbitre pas, elle évite d'envoyer ce que l'agent refuserait.
5. **Notifications Windows** : un épisode d'alerte notifie une fois ; l'arrêt automatique du mode, seulement après l'avoir vu actif ; même fenêtre d'une minute que le lien (BR-RESIL-015) avec **priorité** à la sécurité, jamais remplacée par un changement du lien ; réglage « Alertes de sécurité » séparé, activé par défaut, qui ne touche jamais les bandeaux ni les épisodes suivis.
6. **Pas de repli automatique sans clé après N échecs de défi.** Quand ce PC a une clé mais que l'agent ne donne pas de défi (coupure du seul défi, réponse illisible, délai), la connexion échoue avec un message juste (« Le serveur ne donne pas, pour l'instant, de quoi reconnaître ce PC. Rien n'a été envoyé. Réessaie dans un instant. », `LinkError::DeviceChallengeUnavailable`) et non « serveur injoignable » ; elle ne se présente jamais comme un poste inconnu (un poste reconnu y perdrait son statut). Un repli demandé par l'utilisateur (réglage) est à étudier avec Q15 ; il n'est pas fait.
7. **Case « Garder ce poste reconnu » décochée par défaut** (Q15) : `keep_address` n'est envoyé que cochée ; désactivée avec sa raison quand l'agent est trop ancien pour connaître la sécurité.

## Alternatives écartées

- **Relire `GET /security` toutes les 5 s dans le client** : l'agent pousse déjà le message à chaque changement ; la relecture n'a lieu qu'au retour du lien et après une issue d'opération.
- **Faire porter `device` par le message du flux** : changement de l'agent hors périmètre, la lecture suffit.
- **`ConfirmDialog` sans mot de passe** : contredit Q16.

## Conséquences

- Une alerte déjà en cours quand le client se lance notifie une fois par exécution et par épisode.
- Un clic sur la notification ne ramène pas à la page Sécurité (greffon de notifications, ADR-0016) ; le bandeau est au premier plan à l'ouverture de la fenêtre.
- Le toast unique « Ce PC est maintenant un poste de confiance de ce serveur. » (design, écran E, « à confirmer ») n'est pas fait : il faut un signal de la liaison (inscription réussie) qui n'existe pas.

## Quand ne PAS l'appliquer

- Pour un acte d'administration autre que le mode attaque (HRT-28, Q17) : le même schéma s'appliquera, mais cette ADR ne le décide pas.
