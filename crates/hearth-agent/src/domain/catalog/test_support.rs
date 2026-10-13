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

pub const RICH_FICHE: &str = r##"format = 2
id = "rich"
version = 3
family = "rich"
name = "Rich"
summary = "Fiche de test qui exerce tout le format."
status = "preview"
platform = "linux"

[image]
repository = "example/rich"
tag = "2.0"
game_updates = "at_install"

[owner]
uid = 1001
gid = 1002

[[variant]]
id = "java"
label = "Java"
help = "La version Java."
default = true

[[variant]]
id = "bedrock"
label = "Bedrock"
help = "La version Bedrock."
default = false
image_tag = "bedrock-2.0"

[[variant.env]]
name = "EDITION"
value = "bedrock"

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

[[folder]]
id = "mods"
role = "mods"
target = "/data/mods"
read_only = false
backup = false
drop_hint = "Dépose ici tes fichiers de mods."

[[port]]
id = "game"
container = 25565
protocol = "tcp"
role = "game"
publish = "always"
default_host = 25565
group = "base"
mapping = "same_number"
game_setting = "server_port"

[[port]]
id = "query"
container = 25566
protocol = "udp"
role = "query"
publish = "optional"
default_host = 25566
group = "base"
mapping = "nat"

[[port]]
id = "rcon"
container = 25575
protocol = "tcp"
role = "remote_console"
publish = "never"
mapping = "nat"

[[channel]]
id = "env"
kind = "env"

[[channel]]
id = "props"
kind = "file"
format = "kv_lines"
folder = "config"
path = "server.properties"
mode = "0640"
written_by_game = "at_stop"
raw_edit = false

[channel.kv]
separator = "="
comment = "#"
quote = "none"

[[channel]]
id = "xml"
kind = "file"
format = "xml_properties"
folder = "config"
path = "settings.xml"
mode = "0644"
written_by_game = "unverified"
raw_edit = true

[channel.xml]
root = "ServerSettings"
element = "property"
key_attr = "name"
value_attr = "value"

[[channel]]
id = "args"
kind = "args"

[[env]]
name = "PUID"
owner = "uid"

[[env]]
name = "PGID"
owner = "gid"

[[env]]
name = "RCON_PASSWORD"
secret = "rcon"

[[secret]]
id = "rcon"
origin = "generated"
length = 32
alphabet = "alnum"

[remote_console]
protocol = "source_rcon"
port = "rcon"
password = "rcon"

[[agreement]]
id = "eula"
title = "Contrat de licence"
url = "https://example.org/eula"
sentence = "J'ai lu le contrat et je l'accepte."

[agreement.binds]
channel = "env"
key = "EULA"
accepted_value = "TRUE"

[first_start]
ready_timeout_s = 3600
world_generation = true

[[first_start.download]]
what = "Fichiers du serveur"
source_label = "Steam"
bytes = 1073741824
evidence = { publisher = "https://example.org/size" }

[[first_start.stage]]
label = "Téléchargement"
log_contains = "Downloading"

[ready]
log_contains = "Done"
status = "tested"

[game_version]
after = "version "
until_any = [" "]
scheme = "dotted_numeric"
status = "tested"

[stop]
grace_s = 60
signal = "SIGTERM"
world_saved = "by_sequence"

[stop.sequence]
status = "tested"

[[stop.sequence.step]]
kind = "stdin_line"
line = "save"
quits = false

[[stop.sequence.step]]
kind = "wait_log"
contains = "Saved"
timeout_s = 30
required = true

[[stop.sequence.step]]
kind = "stdin_line"
line = "stop"
quits = true

[save]
status = "tested"

[[save.step]]
kind = "stdin_line"
line = "save"
quits = false

[[save.step]]
kind = "wait_log"
contains = "Saved"
timeout_s = 30
required = false

[resources]
cpu = { min_millis = 1000, recommended_millis = 2000, evidence = "measured" }
memory = { min_bytes = 1073741824, recommended_bytes = 2147483648, evidence = "measured_idle" }
disk = { install_bytes = 1073741824, evidence = { publisher = "https://example.org/size" } }

[worlds]
folder = "world"
unit = "dir"
depth = 1
names_setting = "level_name"
carried_by_world = ["difficulty"]
loaded_contains = "Preparing level"

[[not_exposed]]
key = "ops"
channel = "props"
why = "Liste des opérateurs : elle se gère depuis le jeu."

[[check]]
if = { setting = "hardcore", equals = "TRUE" }
then = { setting = "difficulty", must_equal = "hard" }
message = "Le mode hardcore impose la difficulté difficile."

[[migration]]
from_version = 1
ops = [{ rename = ["old-key", "new-key"] }, { retire = "old_mode" }]
"##;

pub const RICH_SETTINGS: &str = r##"[[theme]]
id = "game"
label = "Partie"
order = 1

[[theme]]
id = "network"
label = "Réseau"
order = 2

[[setting]]
id = "level_name"
key = "level-name"
channel = "props"
theme = "game"
level = "essential"
label = "Nom du monde"
help = "Le nom du monde."
type = "text"
charset = "file_stem"
max_len = 32
proposed = "world"
change = "world_creation"
source = "verified"

[[setting]]
id = "motd"
key = "MOTD"
channel = "env"
theme = "game"
level = "essential"
label = "Message d'accueil"
help = "Le message montré aux joueurs."
type = "text"
charset = "line"
max_len = 64
proposed = "Bienvenue"
change = "anytime"
source = "verified"

[[setting]]
id = "max_players"
key = "MAX_PLAYERS"
channel = "env"
theme = "game"
level = "essential"
label = "Joueurs"
help = "Le nombre de joueurs."
type = "int"
min = 1
max = 100
proposed = "10"
change = "anytime"
source = "partial"

[[setting]]
id = "difficulty"
key = "Difficulty"
channel = "xml"
theme = "game"
level = "essential"
label = "Difficulté"
help = "La difficulté du monde."
type = "choice"
choices = [
  { value = "easy", label = "Facile" },
  { value = "normal", label = "Normale" },
  { value = "hard", label = "Difficile" },
]
proposed = "normal"
change = "world_creation"
source = "verified"

[[setting]]
id = "hardcore"
key = "HARDCORE"
channel = "env"
theme = "game"
level = "common"
label = "Hardcore"
help = "Une seule vie."
type = "bool"
on = "TRUE"
off = "FALSE"
proposed = "FALSE"
change = "anytime"
source = "partial"

[[setting]]
id = "server_port"
key = "server-port"
channel = "props"
theme = "network"
level = "advanced"
label = "Port du jeu"
help = "Le port annoncé par le jeu."
type = "int"
min = 1024
max = 65535
change = "anytime"
imposed = { port = "game", why = "Le jeu annonce son port : il suit celui du service." }
source = "verified"

[[setting]]
id = "rcon_enabled"
key = "enable-rcon"
channel = "props"
theme = "network"
level = "advanced"
label = "Console distante"
help = "Active la console distante."
type = "bool"
on = "true"
off = "false"
change = "anytime"
imposed = { value = "true", why = "L'arrêt propre du jeu en dépend." }
source = "verified"

[[setting]]
id = "rcon_port"
key = "rcon.port"
channel = "props"
theme = "network"
level = "advanced"
label = "Port de la console"
help = "Le port de la console distante."
type = "int"
min = 1024
max = 65535
change = "anytime"
imposed = { port = "rcon", why = "Le port de la console n'est jamais publié." }
source = "verified"

[[setting]]
id = "rcon_password"
key = "rcon.password"
channel = "props"
theme = "network"
level = "advanced"
label = "Mot de passe de la console"
help = "Généré par Hearth."
type = "secret"
secret_kind = "admin_password"
max_len = 64
change = "anytime"
imposed = { secret = "rcon", why = "Généré par Hearth, jamais affiché." }
source = "verified"

[[setting]]
id = "admin_password"
key = "admin-password"
channel = "props"
theme = "game"
level = "common"
label = "Mot de passe d'administration"
help = "Le mot de passe des opérateurs."
type = "secret"
secret_kind = "admin_password"
min_len = 8
max_len = 64
change = "anytime"
source = "verified"

[[setting]]
id = "mods"
key = "MODS"
channel = "env"
theme = "game"
level = "advanced"
label = "Mods"
help = "Les identifiants de mods que le programme télécharge lui-même."
type = "id_list"
item_charset = "digits"
separator = ","
max_items = 20
change = "anytime"
source = "partial"

[[setting]]
id = "view"
key = "-view"
channel = "args"
arg = { flag = "-view", form = "flag_value" }
theme = "game"
level = "common"
label = "Distance de vue"
help = "La distance de vue."
type = "int"
min = 2
max = 32
proposed = "10"
change = "anytime"
source = "partial"

[[setting]]
id = "bedrock_only"
key = "BEDROCK_FLAG"
channel = "env"
theme = "game"
level = "advanced"
label = "Option Bedrock"
help = "Une option de la version Bedrock."
type = "bool"
on = "1"
off = "0"
only_variants = ["bedrock"]
proposed = "0"
change = "anytime"
source = "partial"

[[retired]]
id = "old_mode"
key = "old-mode"
channel = "props"
label = "Ancien mode"
until = "2.0"
replaced_by = "difficulty"
why = "Le jeu a remplacé ce réglage par la difficulté."
"##;

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
