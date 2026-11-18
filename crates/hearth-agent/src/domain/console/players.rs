//! Le lecteur de la réponse à la commande `list` (HRT-267, BR-CONSOLE-011, ADR-0067).
//!
//! La réponse vient du jeu, donc d'un conteneur hostile : elle n'est jamais crue au-delà de ce que ce lecteur sait en
//! tirer. Deux formes de Minecraft :
//! - récente : `There are 2 of a max of 20 players online: Bram, Lia` ;
//! - ancienne : `There are 2/20 players online:` suivi des noms (sur la même ligne ou la suivante).
//!
//! Le nombre de joueurs et les places sont bornés (0 à 100 000) ; au plus 100 noms sont gardés ; un nom qui n'a pas la
//! forme d'un pseudo (`\.?[A-Za-z0-9_]{1,16}`, le point étant celui des joueurs Bedrock) est compté sans être rendu.
//! Une réponse inconnue est « illisible » : jamais un chiffre deviné.

/// Au plus ce nombre de joueurs ou de places.
pub const MAX_COUNT: u32 = 100_000;
/// Au plus ce nombre de noms gardés.
pub const MAX_NAMES: usize = 100;

/// Les joueurs connectés, tels que la réponse les dit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Players {
    pub online: u32,
    pub max: u32,
    /// Les pseudos de la forme permise, au plus [`MAX_NAMES`].
    pub names: Vec<String>,
    /// Des noms comptés mais pas rendus : hors de la forme d'un pseudo, ou au-delà de [`MAX_NAMES`].
    pub hidden_names: u32,
}

/// Les lecteurs de la liste fermée de la fiche.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reader {
    MinecraftList,
}

impl Reader {
    pub fn parse(text: &str) -> Option<Self> {
        (text == "minecraft_list").then_some(Self::MinecraftList)
    }
}

/// Lit la réponse. `None` : illisible.
pub fn read(reader: Reader, reply: &str) -> Option<Players> {
    match reader {
        Reader::MinecraftList => minecraft_list(reply),
    }
}

fn nom_ok(name: &str) -> bool {
    let body = name.strip_prefix('.').unwrap_or(name);
    (1..=16).contains(&body.len()) && body.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Un entier de 0 à [`MAX_COUNT`], chiffres seulement.
fn count(text: &str) -> Option<u32> {
    if text.is_empty() || text.len() > 9 || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse::<u32>().ok().filter(|n| *n <= MAX_COUNT)
}

fn minecraft_list(reply: &str) -> Option<Players> {
    let reply = reply.trim();
    let rest = reply.strip_prefix("There are ")?;
    // « N of a max of M players online » (récente) ou « N/M players online » (ancienne).
    let (online, max, after) = if let Some((n, tail)) = rest.split_once(" of a max of ") {
        let (m, tail) = tail.split_once(' ')?;
        (count(n)?, count(m)?, tail)
    } else {
        let (pair, tail) = rest.split_once(' ')?;
        let (n, m) = pair.split_once('/')?;
        (count(n)?, count(m)?, tail)
    };
    let after = after
        .strip_prefix("players online")
        .or_else(|| after.strip_prefix("player online"))?;
    // Le texte qui suit : `:` puis les noms (ou rien).
    let names_text = match after.strip_prefix(':') {
        Some(names) => names,
        None if after.trim().is_empty() => "",
        None => return None,
    };
    let mut names = Vec::new();
    let mut hidden = 0u32;
    for raw in names_text
        .split(',')
        .map(str::trim)
        .filter(|n| !n.is_empty())
    {
        if nom_ok(raw) && names.len() < MAX_NAMES {
            names.push(raw.to_owned());
        } else {
            hidden = hidden.saturating_add(1);
        }
    }
    Some(Players {
        online,
        max,
        names,
        hidden_names: hidden,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_list(reply: &str) -> Option<Players> {
        read(Reader::MinecraftList, reply)
    }

    #[test]
    fn the_recent_and_the_old_forms_are_read() {
        let recent = read_list("There are 2 of a max of 20 players online: Bram, Lia").unwrap();
        assert_eq!((recent.online, recent.max), (2, 20));
        assert_eq!(recent.names, ["Bram", "Lia"]);
        assert_eq!(recent.hidden_names, 0);
        let old = read_list("There are 1/20 players online:\nBram")
            .unwrap_or_else(|| read_list("There are 1/20 players online: Bram").unwrap());
        assert_eq!((old.online, old.max), (1, 20));
        let same_line = read_list("There are 1/20 players online: Bram").unwrap();
        assert_eq!(same_line.names, ["Bram"]);
    }

    #[test]
    fn nobody_online_is_zero_players_and_no_names() {
        let none = read_list("There are 0 of a max of 20 players online: ").unwrap();
        assert_eq!((none.online, none.max), (0, 20));
        assert!(none.names.is_empty());
        let bare = read_list("There are 0 of a max of 20 players online:").unwrap();
        assert_eq!(bare.online, 0);
    }

    #[test]
    fn a_bedrock_name_with_a_dot_is_a_name_and_a_hostile_one_is_counted_but_not_returned() {
        let reply = "There are 3 of a max of 20 players online: .Bedrock_Joe, Bram, §cx\u{202e}evil, Name With Space, Waytoolongplayernamethatisbad";
        let players = read_list(reply).unwrap();
        assert_eq!(players.names, [".Bedrock_Joe", "Bram"]);
        assert_eq!(players.hidden_names, 3);
        assert_eq!(players.online, 3);
    }

    #[test]
    fn huge_numbers_and_unknown_replies_are_unreadable() {
        for reply in [
            "",
            "Unknown command",
            "There are 999999999999 of a max of 20 players online:",
            "There are 100001 of a max of 20 players online:",
            "There are 2 of a max of 100001 players online:",
            "There are -1 of a max of 20 players online:",
            "There are 2 of a max of 20 gens online:",
            "There are x/20 players online:",
            "There are 2 of a max of 20 players online ceci n'est pas une liste",
        ] {
            assert_eq!(read_list(reply), None, "{reply:?}");
        }
        assert!(read_list("There are 100000 of a max of 100000 players online:").is_some());
    }

    #[test]
    fn at_most_a_hundred_names_are_kept_and_the_rest_is_counted() {
        let names: Vec<String> = (0..150).map(|n| format!("p{n}")).collect();
        let reply = format!(
            "There are 150 of a max of 200 players online: {}",
            names.join(", ")
        );
        let players = read_list(&reply).unwrap();
        assert_eq!(players.names.len(), 100);
        assert_eq!(players.hidden_names, 50);
    }

    #[test]
    fn the_reader_list_is_closed() {
        assert_eq!(Reader::parse("minecraft_list"), Some(Reader::MinecraftList));
        assert_eq!(Reader::parse("autre"), None);
    }
}
