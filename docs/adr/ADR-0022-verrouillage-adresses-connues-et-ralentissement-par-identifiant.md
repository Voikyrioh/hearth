---
id: ADR-0022
titre: Verrouillage de connexion : ralentissement par identifiant et adresses connues d'un compte (modèle PROVISOIRE)
type: securite
statut: acceptée
date: 2026-10-06
portee: projet
remplace: —
liens: [ADR-0004, ADR-0006, BR-CONN-006, BR-CONN-007, BR-CONN-013, BR-CONN-018, BR-CONN-019, BR-CONN-020, BR-AUDIT-006, BR-AUDIT-007, conception technique 2026-10-04 section 7, HRT-20]
---

# ADR-0022 : Verrouillage de connexion résistant au changement d'adresse

> **Points 1 et 4 remplacés par l'[ADR-0024](./ADR-0024-etats-de-securite-et-regle-deux-sur-trois.md)** (HRT-24 : l'« adresse connue » devient un critère de la règle « 2 critères sur 3 »). Le texte ci-dessous est conservé tel qu'il était.
>
> **PROVISOIRE.** Le mécanisme « adresse connue » décrit ici (points 1 et 4) **sera remplacé par l'identité d'appareil par clé** (échange de clé à la connexion, pour garantir que c'est bien le client d'origine qui opère ; conception à venir, ticket à part). Il est livré tel quel, avec ses limites connues (voir « Limites connues »). Le ralentissement par identifiant (point 2) ne dépend pas de ce choix.

## Contexte

Le verrouillage (BR-CONN-006 et 007) compte les échecs par couple (identifiant, adresse) et par adresse seule. Sur un réseau local, un appareil qui change d'adresse (alias IPv4, adresses IPv6 temporaires) regagne cinq essais à chaque fois : le verrouillage ne protège plus rien. Verrouiller par **compte** fermerait la brèche mais donnerait à n'importe quel appareil du réseau (invité, objet connecté) le pouvoir d'enfermer l'administrateur hors de son propre serveur : c'est la pire issue possible d'un durcissement, et elle est exclue.

## Décision

Fonctions pures du domaine (horloge injectée, aucune E/S) : `domain/login_policy.rs` (la décision), `domain/identifier_slowdown.rs` (la courbe), `domain/known_address.rs` (les adresses connues), `domain/eviction.rs` (l'ordre d'éviction des tables bornées).

### 1. Adresse connue d'un compte (provisoire)

- **Acquisition** : une adresse devient connue d'un compte **uniquement** par une connexion **réussie** de ce compte depuis cette adresse (mot de passe vérifié, dans la transaction qui ouvre la session).
- **Forme** : l'adresse **exacte** de la connexion TCP, canonique (en IPv6 l'adresse complète, **jamais un préfixe**). **Aucun regroupement des adresses IPv6 en /64** (décision du détenteur) : le compteur du couple et le compteur par adresse restent par adresse exacte, comme avant HRT-20.
- **Stockage** : table SQLite `known_addresses`, suppression en cascade avec le compte ; survit au redémarrage. **Durée** : 30 jours après la dernière connexion réussie. **Nombre** : 8 par compte (la moins récente est oubliée). **Oubli** : sessions fermées par l'administration, mot de passe changé (par l'administrateur ou par le titulaire), compte supprimé.
- **Droit donné** : **aucun**. Une adresse connue ne dispense jamais du mot de passe. Son seul effet : le titulaire du mot de passe, depuis une adresse connue de son compte, **passe malgré l'attente** du ralentissement par identifiant. Les compteurs du couple et de l'adresse s'appliquent à elle comme à toute autre.
- **Reprise** à la migration : les adresses des sessions encore valides.

### 2. Ralentissement par identifiant (jamais un blocage)

Un compteur par **identifiant saisi** (empreinte SHA-256 de l'identifiant normalisé, jamais l'identifiant en clair ; qu'il existe ou non), tous clients confondus. Il compte les échecs venus d'adresses inconnues de **tous** les comptes.

- 10 échecs gratuits, puis attente avant la tentative suivante : 2 s, 4 s, 8 s, 16 s, 32 s, 64 s, puis **120 s, plafond explicite**. Remise à zéro après 30 minutes sans échec. Un succès ne le remet pas à zéro.
- **Le refus se fait APRÈS la vérification du mot de passe** (contre un haché factice si l'identifiant n'existe pas), pour que le chemin, la durée et la réponse soient les mêmes pour un identifiant existant ou non, depuis n'importe quelle adresse (BR-CONN-013). Contrepartie : une tentative ralentie coûte un calcul Argon2, que borne le plafond des connexions en cours (point 4).
- Pendant l'attente : `429 TOO_MANY_ATTEMPTS` avec `details.retry_after_s`, **la même réponse** que pour toute autre attente. Un mot de passe faux compte dans le compteur du couple pour tout le monde ; un mot de passe juste depuis une adresse inconnue du compte est refusé sans rien compter ; un mot de passe juste depuis une adresse connue du compte passe.
- **Horloge** : une attente qui se termine à plus de 120 s devant `maintenant` ne peut venir que d'une horloge reculée ; elle est ramenée à `maintenant + 120 s` (jamais plus que le plafond, jamais zéro) et la correction est écrite. Une horloge avancée met fin aux attentes. Les dates sont murales et persistées (une attente doit survivre à un redémarrage, ce qu'une horloge monotone ne fait pas).

### 3. Adresses exactes, jamais un préfixe

Le compteur du couple (identifiant, adresse), celui de l'adresse seule et la file d'attente sont par **adresse exacte**. Un appareil qui change d'adresse IPv6 regagne des essais par adresse, mais le ralentissement par identifiant (point 2) ne dépend d'aucune adresse : c'est lui qui ferme la brèche. Regrouper en /64 aurait permis à un appareil du foyer de bloquer tous les postes non encore connus de son préfixe.

### 4. Plafonds et places réservées

- **Connexions en cours** : une traitée et huit en attente par adresse (existant), et **32 au total**, dont **8 réservées aux adresses déjà connues** (d'un compte quelconque : une connexion réussie récente, ou une session valide) : une adresse inconnue n'en prend jamais plus de 24. La décision ne dépend **jamais de l'identifiant saisi** (sinon la réservation serait un oracle d'existence). Comme pour le flux, la lecture en base n'a lieu qu'en saturation. 32 reste inférieur à la capacité du hacheur (4 calculs et 32 en attente, ADR-0009). Au-delà : refus immédiat `429`, `retry_after_s = 1`.
- **Places d'attente du flux temps réel** : 16 au total, 2 par adresse, dont **4 réservées** aux adresses connues. Suivi de la review de HRT-06 rattaché à ce ticket ; même notion d'adresse connue, aucun effet autre que la place.
- **Tables bornées** : `identifier_slowdowns` 10 000 lignes, `login_attempts` 50 000, `known_addresses` 8 par compte. Au dépassement, **un seul ordre d'éviction pour les deux premières** (`domain::eviction::rank`) : d'abord les lignes **sans attente en cours**, puis **moins d'échecs**, puis **les plus anciennes**. La borne est vérifiée seulement quand une ligne a été créée.

## Menaces traitées nommément

| Menace | Traitement |
|---|---|
| **Usurpation d'une adresse connue** | Une adresse IP n'est pas une identité : la connaître ne donne aucun droit. Le mot de passe reste exigé ; les compteurs du couple et de l'adresse s'appliquent ; pendant l'attente de l'identifiant, chaque mot de passe faux compte dans le couple (5 essais puis 1 minute doublée, plafond 15 minutes). Usurper en TCP exige une position d'interception. |
| **Énumération d'identifiants** (BR-CONN-013) | Aucune exemption des compteurs du couple et de l'adresse. Le ralentissement par identifiant refuse après vérification, pour tous. Il ne compte pas les échecs d'une adresse connue d'un compte **quelconque**, que l'identifiant existe ou non, et un mot de passe faux compte dans le couple pour tout le monde pendant l'attente : la progression des compteurs ne dit pas si une adresse est connue du compte visé. La réservation de places dépend de l'adresse seule. Tests : `tests/login_review.rs` (la suite de 21 requêtes du reviewer, rouge avant correction ; le scénario d'attente), `tests/login_lockout.rs::an_existing_and_a_missing_identifier_...`. Reste observable, par construction : le titulaire du mot de passe, depuis une adresse connue de son compte, passe pendant l'attente. |
| **Épuisement de mémoire** | Tables SQLite bornées avec un ordre d'éviction unique ; tours par adresse et places du flux bornés ; 8 adresses connues par compte ; clés de longueur bornée. |
| **Saturation de la file de hachage Argon2** | Plafond de 32 connexions en cours (8 réservées) inférieur à la capacité du hacheur. Une tentative ralentie coûte un calcul : le plafond le borne ; l'administrateur sur une adresse connue garde ses places. |
| **IPv6** | Pas de regroupement par préfixe. Le changement d'adresse est fermé par le ralentissement par identifiant, qui ne dépend d'aucune adresse. |
| **Horloge** | Horloge injectée ; attente ramenée au plafond en cas de recul, correction persistée ; fin des attentes en cas d'avance brutale. Tests : recul d'une heure, d'un an, avance d'un an. |
| **Journal** | Chaque tentative refusée à cause du ralentissement est comptée (une entrée puis une synthèse au compte exact). La clé de regroupement ne contient **aucune donnée variable** (`Reason::group_text` : la durée d'attente n'en fait pas partie, la synthèse garde celle de la dernière occurrence) : trois heures d'attaque, une tentative toutes les 15 secondes depuis des adresses toujours neuves, laissent 541 entrées exactement et comptent les 720 tentatives (environ 1 260 entrées pour sept heures, sur 50 000). |

## Essais offerts à un attaquant contre un identifiant (par le calcul, vérifié par les tests de la courbe)

- **Avant HRT-20** : 5 par adresse puis 6 de plus dans l'heure, sans plafond par identifiant ; en IPv6, borné par le seul hacheur (quelques centaines de milliers par heure, non mesuré).
- **Après, en changeant d'adresse (IPv4 ou IPv6)** : **45 la première heure** (10 gratuits, 6 pendant la montée de 126 s, 29 au plafond de 120 s), puis **30 par heure** en continu (31 à 32 par heure en alternant pauses de 30 minutes et salves).
- **Après, avec usurpation des 8 adresses connues du compte** (pire cas) : par adresse usurpée, 11 la première heure puis 4 par heure (le compteur du couple seul : « jamais un droit » est tenu). Total : 45 + 88 = **133 la première heure, puis 30 + 32 = 62 par heure**.

## Limites connues (modèle provisoire, à reprendre dans la conception de l'identité d'appareil)

- **Session de plus de 30 jours** : l'adresse n'est rafraîchie que par une connexion, jamais par l'usage de la session (qui glisse 30 jours à chaque requête). Un poste qui n'a pas rouvert de connexion depuis plus de 30 jours n'est plus connu. La reprise de la migration prend la date de création de la session, pas sa dernière activité.
- **Adresse IPv6 temporaire qui change** : la nouvelle adresse n'est connue qu'à la prochaine connexion réussie, pas à l'usage de la session ouverte.
- **Changement de son propre mot de passe** : les adresses connues du compte sont oubliées, y compris celle de la session que le titulaire garde ; la prochaine connexion réussie la rapprend.
- **Poste jamais vu** : un poste que le compte ne connaît pas, pendant une attaque continue contre son identifiant, est ralenti, voire privé d'essais tant que l'attaque dure (il peut perdre la course contre l'attaquant à chaque fin d'attente). Deux voies de secours : un poste déjà connu du compte, ou la commande `hearth-agent account` sur le serveur (`docs/runbooks/recuperer-acces-administrateur.md`).
- Qui usurpe l'adresse connue épuise le compteur du couple et bloque ce poste jusqu'à 15 minutes, renouvelable (limite existante avant HRT-20).

## Conséquences

- Migration `0004_known_addresses_identifier_slowdowns.sql` (additive : deux tables et une reprise des sessions valides). `.sqlx/` régénéré.
- BR-CONN-007 reçoit une section de compléments **à valider** (troisième compteur, plafond global) ; les énoncés de BR-CONN-006, 013 et BR-AUDIT-007 sont inchangés, sauf le changement de règle signalé dans BR-AUDIT-007 (refus ralentis comptés), **à valider**. Nouvelles fiches : BR-CONN-018, 019, 020.
- Journal : le 11e échec écrit « Blocage temporaire » ; aucune nouvelle raison, aucun champ libre.

## Alternatives rejetées

- **Verrou par compte** : un appareil du réseau enfermerait l'administrateur dehors.
- **Origine IPv6 en /64** : un appareil du foyer bloquerait tous les postes non encore connus du préfixe ; le ralentissement par identifiant suffit.
- **Exemption des adresses connues des compteurs par adresse et par identifiant** : un oracle d'existence d'un identifiant.
- **Blocage au-delà d'un seuil par identifiant** : retourné contre l'utilisateur légitime ; le plafond de 2 minutes garde toujours une porte ouverte.
