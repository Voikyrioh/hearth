---
id: ADR-0022
titre: Verrouillage de connexion : ralentissement par identifiant, adresses connues d'un compte, origine IPv6 en /64, places réservées
type: securite
statut: acceptée
date: 2026-10-06
portee: projet
remplace: —
liens: [ADR-0004, ADR-0006, BR-CONN-006, BR-CONN-007, BR-CONN-013, BR-CONN-018, BR-CONN-019, BR-CONN-020, BR-AUDIT-006, BR-AUDIT-007, conception technique 2026-10-04 section 7, HRT-20]
---

# ADR-0022 : Verrouillage de connexion résistant au changement d'adresse

## Contexte

Le verrouillage (BR-CONN-006 et 007) compte les échecs par couple (identifiant, adresse) et par adresse seule. Sur un réseau local, un appareil qui change d'adresse (alias IPv4, adresses IPv6 temporaires d'un même préfixe) regagne cinq essais à chaque fois : le verrouillage ne protège plus rien. Verrouiller par **compte** fermerait la brèche mais donnerait à n'importe quel appareil du réseau (invité, objet connecté) le pouvoir d'enfermer l'administrateur hors de son propre serveur : c'est la pire issue possible d'un durcissement, et elle est exclue.

Le seul mur entre un appareil du réseau et un accès administrateur à un agent qui tourne en root est ce verrouillage. Il doit donc ralentir l'attaque **sans jamais** pouvoir être retourné contre l'utilisateur légitime.

## Décision

Trois mécanismes, tous des fonctions pures du domaine (horloge injectée, aucune E/S) : `domain/login_policy.rs` (la décision), `domain/identifier_slowdown.rs` (la courbe), `domain/known_address.rs` (les adresses connues), `domain/login_origin.rs` (adresse, origine).

### 1. Adresse connue d'un compte

- **Acquisition** : une adresse devient connue d'un compte **uniquement** par une connexion **réussie** de ce compte depuis cette adresse (mot de passe vérifié, dans la transaction qui ouvre la session). Aucune autre voie : ni une tentative échouée, ni la lecture d'un journal, ni une session d'un autre compte.
- **Forme** : l'adresse **exacte** de la connexion TCP, en forme canonique (une IPv4 reçue sur une socket double pile est ramenée à IPv4). En IPv6 c'est l'adresse complète (/128), **jamais** le préfixe : sinon tout appareil du réseau domestique, qui partage le /64, serait « connu » de tous les comptes, et la brèche serait rouverte en IPv6.
- **Stockage** : table SQLite `known_addresses (account_id, address, last_success_at)`, clé primaire (compte, adresse), suppression en cascade avec le compte. Elle survit au redémarrage (sans elle, redémarrer l'agent effacerait la liste, et l'administrateur serait ralenti pendant une attaque juste après une mise à jour de l'agent).
- **Durée** : 30 jours après la dernière connexion réussie depuis cette adresse (la durée de vie glissante d'une session, BR-RESIL-012). Chaque connexion réussie la rafraîchit. Purgée par la tâche de maintenance.
- **Nombre** : 8 par compte. Au-delà, la moins récemment réussie est oubliée (`known_address::learn`, fonction pure).
- **Oubli** : session fermée par l'administration (BR-ACCT-011), mot de passe changé (par l'administrateur ou par le titulaire, BR-ACCT-008 et 009) : **toutes** les adresses connues du compte sont oubliées dans la même transaction. Compte supprimé : suppression en cascade. La déconnexion simple (BR-CONN) ne les oublie pas. Après un changement de mot de passe, la prochaine connexion réussie rapprend l'adresse.
- **Droit donné** : **aucun**. Une adresse connue n'ouvre aucune session et ne dispense jamais du mot de passe. Elle évite seulement d'être ralenti par **les échecs des autres** (points 2 et 4). Les échecs faits depuis une adresse connue ne nourrissent ni le ralentissement par identifiant ni le compteur par origine : ils ne comptent que dans le compteur du couple (identifiant, adresse), qui reste en place pour cette adresse (5 échecs, attente doublée, plafond 15 minutes).
- **Migration** : à l'installation de la migration, les adresses des sessions encore valides sont reprises comme adresses connues (au plus 8 par compte), pour que l'administrateur ne perde pas son poste habituel à la mise à jour de l'agent.

### 2. Ralentissement par identifiant (jamais un blocage)

Un compteur par **identifiant saisi** (empreinte SHA-256 de l'identifiant normalisé, comme la clé du couple : jamais l'identifiant en clair), qu'il existe ou non. Il compte les échecs venus d'une adresse **non connue de ce compte**.

- 10 échecs « gratuits » (le compteur du couple verrouille déjà au 5e depuis une même adresse : une personne qui se trompe ne le voit jamais).
- À partir du 11e échec, attente avant la tentative suivante : 2 s, 4 s, 8 s, 16 s, 32 s, 64 s, puis **120 s, plafond explicite** (`identifier_slowdown::MAX_DELAY`). L'attente n'est **jamais** supérieure à 2 minutes et ne devient jamais un blocage : même contre une attaque continue, une tentative est possible toutes les 2 minutes.
- Le compteur repart à zéro après 30 minutes sans échec. Un succès ne le remet pas à zéro (un attaquant qui possède un compte intercalerait sinon une connexion valide ; un succès n'est de toute façon pas un échec).
- Ce que fait l'attaquant qui change d'adresse : il garde ses essais par adresse, mais l'identifiant visé le ralentit quelle que soit l'adresse. Débit maximal contre un identifiant : environ 60 essais par heure (10 essais gratuits, puis une tentative toutes les 2 minutes ; chaque fenêtre de 30 minutes sans échec en rend 10).
- Ce que voit l'attaquant : `429 TOO_MANY_ATTEMPTS` avec `details.retry_after_s`, **la même réponse** que pour les autres attentes (couple, adresse). Rien n'indique quel compteur a joué.
- Ce que voit l'utilisateur légitime : sur une adresse connue, rien (le compteur ne lui est jamais appliqué). Sur une nouvelle adresse pendant une attaque, la même attente, au plus 2 minutes.
- Limite assumée : un utilisateur légitime sur une adresse **nouvelle**, pendant une attaque soutenue sur son identifiant, peut perdre la course contre l'attaquant au moment où l'attente expire. Il n'est pas bloqué définitivement, mais il peut attendre longtemps. Parade : se connecter d'abord depuis un poste connu (qui ne subit jamais le compteur) ou, sur le serveur, la ligne de commande (`hearth-agent account`).

### 3. Origine : l'IPv6 compte par préfixe /64

Le compteur par adresse seule (BR-CONN-007, 20 échecs en 10 minutes) compte par **origine** : l'adresse IPv4, ou le préfixe /64 d'une adresse IPv6. Un appareil qui change d'adresse IPv6 dans son /64 (adresses temporaires, SLAAC) reste la même origine. Le compteur du couple (identifiant, adresse) et la file d'attente restent par adresse exacte.

Une origine partagée est un risque : sur un réseau domestique tous les appareils IPv6 partagent le /64, un attaquant ferait donc bloquer **tous** les appareils (administrateur compris). C'est pourquoi une connexion faite depuis une adresse connue **du compte visé** ne passe pas par le compteur par origine : l'administrateur sur son poste habituel n'est jamais bloqué par le compteur d'origine, ni par ses voisins de préfixe, ni par un attaquant.

### 4. Plafonds et places réservées

- **Connexions en cours** (en attente de leur tour ou en vérification) : une par adresse en cours de traitement et huit en attente au plus (existant), et **32 au total**, dont **8 réservées aux adresses connues du compte visé** (les inconnues ne dépassent pas 24). Au-delà : refus immédiat `429 TOO_MANY_ATTEMPTS`, `retry_after_s = 1`, mot de passe non gardé. 32 reste inférieur à la capacité du hacheur (4 calculs et 32 en attente, ADR-0009) : une connexion admise ne reçoit jamais `503 BUSY` du hacheur, et un attaquant ne peut pas saturer le hacheur au point d'en priver l'administrateur.
- **Places d'attente du flux temps réel** (suivi de la review de HRT-06) : 16 au total, 2 par adresse (existant), dont **4 réservées** aux adresses déjà connues (qui ont une session valide ou une connexion réussie dans les 30 jours). Un inconnu ne dépasse pas 12 places en attente. C'est la même notion d'adresse connue : le flux n'a pas de compte avant son premier message `auth`, la connaissance est donc par adresse seule, sans effet autre que la place.
- **Tables bornées** : `identifier_slowdowns` 10 000 lignes, `login_attempts` 50 000 lignes. Au dépassement, on oublie d'abord les lignes **sans attente en cours**, de moins d'échecs, les plus anciennes (`identifier_slowdown::excess`, tri dans l'adaptateur) : un flot d'identifiants inventés (une ligne chacun, un échec) évince ses propres lignes, pas celle de l'identifiant réellement attaqué, qui en compte beaucoup. `known_addresses` : 8 par compte. En mémoire, les tables du tour par adresse et du flux ne dépassent jamais les plafonds ci-dessus.

## Menaces traitées nommément

| Menace | Traitement |
|---|---|
| **Usurpation d'une adresse connue** (réseau local) | Une adresse IP n'est pas une identité : la connaître ne donne aucun droit. Le mot de passe reste exigé ; le compteur du couple (identifiant, adresse) reste en place pour cette adresse (au plus 5 essais avant 1 minute d'attente, doublée jusqu'à 15 minutes) ; l'usurpateur ne peut apprendre l'adresse à aucun compte (acquisition par succès seulement) ; ses échecs ne nourrissent ni le ralentissement ni le compteur d'origine (ils ne gênent donc personne d'autre). Usurper une adresse en TCP exige d'être en position d'interception : le pire résultat est de consommer le compteur du couple de la victime pendant au plus 15 minutes (limite existante). |
| **Énumération d'identifiants par le temps de réponse** (BR-CONN-013) | Un identifiant inconnu suit **le même chemin** qu'un identifiant connu : même lecture des adresses connues (une jointure sur l'identifiant, qui rend zéro ligne pour un inconnu), même vérification contre un haché factice, mêmes écritures (ralentissement, couple, origine) dans la même transaction, même réponse. Le ralentissement par identifiant joue **de la même façon** pour un identifiant qui n'existe pas : l'attente annoncée ne révèle pas son existence. Test : mêmes compteurs et mêmes réponses pour un identifiant existant et inexistant, mêmes entrées de journal (BR-AUDIT-006). |
| **Épuisement de mémoire** (identifiants ou adresses en grand nombre) | Tables SQLite bornées avec éviction (point 4), tours par adresse et places du flux bornés, 8 adresses connues par compte, clés de longueur bornée (64 caractères d'identifiant, empreinte de 16 octets). |
| **Saturation de la file de hachage Argon2** | Plafond global de 32 connexions en cours avec 8 places réservées aux adresses connues (inférieur à la capacité du hacheur) ; le refus d'une attente (couple, origine, identifiant) se fait **avant** le calcul Argon2, donc une attaque ralentie ne coûte plus de calcul. |
| **IPv6** | Origine = /64 pour le compteur par adresse (point 3) ; adresse connue = adresse exacte (point 1) ; le ralentissement par identifiant ne dépend d'aucune adresse. |
| **Horloge** | L'horloge est injectée (`Clock`). Les attentes sont des dates persistées (une attente doit survivre à un redémarrage : une horloge monotone ne se persiste pas). Garde-fous : une attente restante est toujours ramenée au plafond (`MAX_DELAY`), donc une horloge reculée ne fabrique jamais un blocage long ; un dernier échec « dans le futur » est traité comme maintenant ; une horloge avancée met fin aux attentes (aucun blocage). Les tests n'utilisent aucune durée réelle ni `sleep`. |

## Conséquences

- Nouvelle migration `0004_known_addresses_identifier_slowdowns.sql` (additive : deux tables et une reprise des sessions valides), compatible avec une base déjà installée. `.sqlx/` régénéré.
- BR-CONN-007 reçoit une section de compléments **à valider** (origine /64, adresse connue exemptée du compteur par origine, troisième compteur) ; les énoncés de BR-CONN-006, 013 et BR-AUDIT-007 sont inchangés (sections ajoutées). Nouvelles fiches : BR-CONN-018 (ralentissement par identifiant), BR-CONN-019 (adresses connues d'un compte), BR-CONN-020 (plafond des connexions et places réservées).
- Aucun nouveau code d'erreur. Journal : le 11e échec écrit « Blocage temporaire » (comme tout déclenchement d'attente), et **chaque tentative refusée à cause du ralentissement reste comptée** (une entrée puis une synthèse au compte exact, regroupement existant, raison « trop de tentatives, attente de N s », aucun champ libre) : le journal ne devient pas muet pendant une attaque. Les refus dus au seul couple ou à la seule origine restent hors journal (BR-AUDIT-007 inchangée).
- Limites : voir « Limite assumée » (point 2) ; un attaquant qui possède déjà le mot de passe n'est pas concerné ; derrière un mandataire toutes les connexions partagent une adresse (inchangé, l'agent n'est pas prévu pour cela).

## Alternatives rejetées

- **Verrou par compte** : un appareil du réseau enfermerait l'administrateur dehors.
- **Adresse connue par préfixe /64** : tout appareil du foyer deviendrait connu.
- **Adresse connue sans durée ni limite** : liste qui grossit, et adresses obsolètes qui gardent un droit d'exemption.
- **Blocage au-delà d'un seuil par identifiant** : retourné contre l'utilisateur légitime ; le plafond de 2 minutes garde toujours une porte ouverte.
