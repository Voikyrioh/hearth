---
id: ADR-0057
titre: Supprimer un service ne perd jamais une donnée sans un choix explicite : conteneur sans volumes ni force, données après le conteneur, dossier rejugé avant d'être effacé, rien repris après un redémarrage
type: securite
statut: acceptée
date: 2026-10-24
portee: projet
remplace: — (complète l'ADR-0039 pour la suppression, l'ADR-0045 pour le plan, l'ADR-0046 pour l'effacement)
liens: [ADR-0039, ADR-0045, ADR-0046, HRT-85, HRT-179, HRT-197, BR-DEL-001 à BR-DEL-031, SECURITY.md]
---

# ADR-0057 : Supprimer un service ne perd rien sans un choix explicite

## Contexte

La suppression est le geste irréversible du lot : l'agent est root et efface des fichiers qu'un programme tiers a écrits, parfois des dizaines de Go. Voiky a demandé que le déploiement d'applications soit complet et sûr : « la suppression ne perd jamais de données sans un choix explicite, elle est reprise proprement après un redémarrage, elle n'efface jamais hors du dossier géré ».

## Décision

1. **Deux gestes de suppression de conteneur, deux garanties.** `ContainerEngine::remove` (volumes anonymes compris, forcé) ne sert qu'à nettoyer un déploiement échoué. La suppression d'un service utilise `remove_keeping_volumes` : `v=false` (les volumes, même anonymes, sont des données du service, BR-DEL-006) et `force=false` (un conteneur qui tourne est refusé, jamais tué en silence). L'arrêt est fait avant, avec le délai de grâce du service, et une coupure forcée est dite au journal.
2. **Données : jamais sans case, jamais avant le conteneur.** `erase_data` et `erase_backups` sont décochées quand elles sont absentes ; le plan de l'opération n'a une étape `erase` que si des données doivent partir ; le nom du service retapé est exigé et vérifié par l'agent. L'effacement vient après la suppression du conteneur et de la description : un échec laisse du superflu, jamais un service à moitié vidé.
3. **Ce qui peut être effacé est décidé trois fois, sur des chemins canoniques et des identités.** Seuls les dossiers gérés par Hearth (un composant sous `<données>/services/<clé>/`) sont candidats ; un dossier du serveur, un nom non sûr, un dossier qu'un autre conteneur monte (même chemin, parent ou enfant, en marche ou arrêté, adopté ou non) ou dont un autre conteneur a des montages non lus sont **conservés** (« un doute vaut conservé ») ; une liste de conteneurs absente ou pas prête est un doute, jamais un feu vert ; les chemins sont comparés sous leur forme canonique des deux côtés (dossier de données canonique lu au démarrage) et par identité (périphérique, inode). Le sort est jugé à la fenêtre, à l'acceptation et juste avant d'effacer chaque dossier.
4. **Une suppression ne se reprend jamais.** Les trois étapes qui détruisent (`remove_container`, `forget`, `erase`) écrivent leur `intent` avant le geste ; un redémarrage de l'agent au milieu laisse une opération `needs_attention` (nom réservé, seule décision « j'ai vérifié »), jamais rejouée : le dossier a pu être réutilisé. Le nom est réservé de l'acceptation à la fin de l'opération.
5. **L'effacement est un geste de fond.** L'appelant lance l'effacement par `spawn_blocking` (il occupe donc un fil du groupe de tâches bloquantes pendant toute sa durée) ; le geste lui-même tourne sur un fil créé pour lui, dont le « nice » est 19 : une priorité PROCESSEUR, pas d'entrée-sortie. Noms lus par lots de 1 024, noms coincés bornés au même lot, suivi dans l'opération (`items_done`) qui peut demander l'arrêt quand l'agent s'arrête (l'opération reste `running`, « à vérifier » au démarrage suivant), ce qui reste est compté et dit (`data_not_erased`), jamais l'arrêt à la première erreur (BR-DEL-030, HRT-179). Un montage « bind » du même système de fichiers est vu par `STATX_MNT_ID`.
6. **Le journal d'activité ne porte que des nombres** : `Target::ServiceDelete` n'a aucun champ pour un chemin, un nom de fichier, une variable ou une commande (BR-DEL-029).
7. **Refus plutôt qu'oubli** : la case « sauvegardes » coche quelque chose qui n'existe pas encore (HRT-97) : elle est refusée (`422`), jamais ignorée en silence.

## Alternatives rejetées

- **`remove` avec volumes pour tout** : perd en silence les données d'une image qui déclare `VOLUME`.
- **Effacer d'abord, supprimer le conteneur ensuite** : un échec du moteur laisserait un service vivant sans ses données.
- **Reprendre un effacement interrompu au redémarrage** : le dossier a pu être réutilisé par un nouveau service du même nom.
- **Juger le partage une seule fois** : un conteneur créé entre la fenêtre et l'effacement perdrait ses données.
- **`ioprio_set` classe `idle`** : non exposé par `rustix`, `libc` est interdit avec `unsafe` ; un « nice » de 19 sur un fil créé pour le geste en tient lieu pour le processeur seulement : l'entrée-sortie n'est pas abaissée (suivi : HRT-212).

## Conséquences

- Le contrat du moteur gagne `remove_keeping_volumes` (faux moteur, faux démon et suite de contrat comprise).
- Les volumes d'un service supprimé restent sur le serveur, listés par la fenêtre (`DeletePreview::volumes`) : les retirer est un geste du serveur, pas de Hearth.
- Capacité `service-delete` ; cause d'opération `data_not_erased`.
- À l'accord du client (HRT-86) : la liste des éléments restés et les chemins « des fichiers peuvent rester dans… » se bâtissent depuis la fenêtre ; l'opération ne les garde pas.
