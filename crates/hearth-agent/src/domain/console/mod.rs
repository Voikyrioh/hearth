//! La console d'un serveur de jeu (HRT-267, ADR-0067) : du domaine pur, aucune E/S. La forme d'une ligne et sa classe
//! sont dans `hearth_proto::console` (partagées avec le client) ; ici, la lecture de la réponse à la commande des
//! joueurs.

pub mod bucket;
pub mod players;
