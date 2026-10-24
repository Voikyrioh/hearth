//! Fiches de test : une fiche MINIMALE valide (les fiches piégées en sont dérivées par une modification d'une
//! ligne) et une fiche RICHE qui exerce tout le format (variantes, contrat, secret généré, console distante,
//! ports qui annoncent leur numéro, deux formats de fichier, arguments, liste d'identifiants...).
//! La fiche Terraria réelle est testée à part (elle est embarquée).

use super::loading::error::FicheRefusee;
use super::loading::loader::{Origin, load};
use super::loading::model::Fiche;

pub const BASE_FICHE: &str = r##"format = 2
id = "demo"
version = 2
family = "demo"
name = "Demo"
summary = "Une fiche de démonstration."
status = "preview"
platform = "linux"

[image]
repository = "example/demo"
tag = "1.2.3"
game_updates = "never"

[owner]
uid = 1000
gid = 1000

[[folder]]
id = "world"
role = "world"
target = "/data/world"
read_only = false
backup = true

[[folder]]
id = "config"
role = "config"
target = "/data/config"
read_only = false
backup = true

[[port]]
id = "game"
container = 7777
protocol = "tcp"
role = "game"
publish = "always"
default_host = 7777
mapping = "nat"

[[channel]]
id = "config"
kind = "file"
format = "kv_lines"
folder = "config"
path = "server.cfg"
mode = "0640"
written_by_game = "never"
raw_edit = true

[channel.kv]
separator = "="
comment = "#"
quote = "none"

[[channel]]
id = "args"
kind = "args"

[first_start]
ready_timeout_s = 600
world_generation = false

[stop]
grace_s = 30
signal = "SIGTERM"
world_saved = "by_image_on_signal"

[resources]
cpu = { evidence = "unmeasured" }
memory = { evidence = "unmeasured" }
disk = { evidence = "unmeasured" }
"##;

pub const BASE_SETTINGS: &str = r##"[[theme]]
id = "game"
label = "Partie"
order = 1

[[setting]]
id = "name"
key = "name"
channel = "config"
theme = "game"
level = "essential"
label = "Nom"
help = "Le nom du serveur."
type = "text"
charset = "line"
max_len = 50
proposed = "Demo"
change = "anytime"
source = "verified"

[[setting]]
id = "tick"
key = "-tick"
channel = "args"
arg = { flag = "-tick", form = "flag_value" }
theme = "game"
level = "common"
label = "Fréquence"
help = "La fréquence de calcul."
type = "int"
min = 1
max = 128
proposed = "20"
change = "anytime"
source = "partial"
"##;

pub const RICH_FICHE: &str = include_str!("../../../tests/fixtures/rich/fiche.toml");

pub const RICH_SETTINGS: &str = include_str!("../../../tests/fixtures/rich/settings.toml");

pub fn load_texts(fiche: &str, settings: &str) -> Result<Fiche, FicheRefusee> {
    load(Origin::Embedded, fiche.as_bytes(), settings.as_bytes())
}

pub fn base() -> Fiche {
    load_texts(BASE_FICHE, BASE_SETTINGS).expect("la fiche de base est valide")
}

pub fn rich() -> Fiche {
    load_texts(RICH_FICHE, RICH_SETTINGS).expect("la fiche riche est valide")
}

/// Remplace la première occurrence de `from` : le test échoue si le texte a changé et ne contient plus `from`.
pub fn replace(text: &str, from: &str, to: &str) -> String {
    assert!(
        text.contains(from),
        "le texte à piéger ne contient plus {from:?}"
    );
    text.replacen(from, to, 1)
}
