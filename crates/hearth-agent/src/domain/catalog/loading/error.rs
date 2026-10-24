//! Refus typés d'une fiche. Un refus nomme le fichier, l'endroit, l'invariant et un motif énuméré :
//! jamais une erreur générique. Le message ne cite jamais la valeur d'un réglage, seulement des noms
//! de la fiche et, pour un alphabet fermé, le caractère fautif (BR-FICHE-005, BR-FICHE-007).

use std::fmt;

/// Fichier d'une fiche où le refus a été trouvé.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fichier {
    Fiche,
    Settings,
    IdsLock,
}

impl Fichier {
    pub fn nom(self) -> &'static str {
        match self {
            Self::Fiche => "fiche.toml",
            Self::Settings => "settings.toml",
            Self::IdsLock => "ids.lock",
        }
    }
}

/// Pourquoi une fiche est refusée. Chaque variante appartient à un invariant (`invariant()`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Motif {
    // ---- Format fermé -------------------------------------------------------------------------
    /// Champ que le schéma ne connaît pas, à n'importe quel niveau.
    ChampInconnu {
        champ: String,
    },
    /// Valeur d'énumération inconnue (statut, protocole, preuve...).
    VarianteInconnue,
    /// Genre de canal inconnu.
    GenreDeCanalInconnu,
    /// Genre d'étape inconnu : aucune étape n'exécute une commande (I4).
    GenreDEtapeInconnu,
    ChampManquant {
        champ: String,
    },
    /// Champ qui n'a pas de sens avec le genre ou le type voisin.
    ChampIncoherent {
        champ: String,
        avec: String,
    },
    Syntaxe {
        detail: String,
    },
    FormatInattendu {
        trouve: u32,
    },
    OrigineNonSupportee,
    // ---- I1 : une seule voie, plan fermé ----------------------------------------------------------
    ImageSansVersion,
    DepotInvalide,
    EmpreinteInvalide,
    PublieeSansEmpreinte,
    PortReserve {
        port: u16,
    },
    PortEnDouble {
        port: u16,
    },
    // ---- I2 : dossiers gérés seulement --------------------------------------------------------
    CheminInvalide {
        ou: String,
    },
    CibleEnDouble,
    DossierInconnu {
        id: String,
    },
    // ---- I3 : rien n'atteint un interpréteur --------------------------------------------------
    ArgumentNonLitteral,
    SecretEnArgument,
    ArgumentHorsAlphabet {
        caractere: char,
    },
    DeriveInvalide {
        raison: &'static str,
    },
    OrdreInvalide {
        raison: &'static str,
    },
    ValeurFixeInvalide {
        raison: &'static str,
    },
    // ---- I4 : aucune exécution ----------------------------------------------------------------
    SequenceIncoherente {
        raison: &'static str,
    },
    // ---- I5 : aucune adresse contactée ---------------------------------------------------------
    AdresseNonHttps,
    AdresseInvalide,
    // ---- I6 : ni expression régulière, ni gabarit ----------------------------------------------
    RepereInvalide {
        raison: &'static str,
    },
    // ---- I7 : console distante ----------------------------------------------------------------
    ConsolePubliee,
    ConsoleMotDePasseNonGenere,
    ConsoleIncoherente {
        raison: &'static str,
    },
    // ---- I8 : un contrat ne s'accepte que par un geste ----------------------------------------
    ContratAccepteDAvance {
        ou: String,
    },
    // ---- I9 : bornes ----------------------------------------------------------------------------
    BorneDepassee {
        quoi: &'static str,
        limite: usize,
        trouve: usize,
    },
    // ---- Cohérence interne ------------------------------------------------------------------------
    IdentifiantInvalide {
        ou: &'static str,
    },
    IdentifiantEnDouble {
        genre: &'static str,
        id: String,
    },
    CleEnDouble {
        canal: String,
        cle: String,
    },
    /// `only_variants = []` : « toutes » s'écrit en omettant le champ (BR-FICHE-060).
    ListeDeVariantesVide,
    ReferenceInconnue {
        genre: &'static str,
        id: String,
    },
    TropDEssentiels {
        trouve: usize,
    },
    ValeurProposeeInvalide {
        reglage: String,
        raison: &'static str,
    },
    SecretAvecValeur {
        reglage: String,
    },
    ReglageNameOnlyIncoherent {
        reglage: String,
    },
    ReglageImposeEssentiel {
        reglage: String,
    },
    RessourceNonMesureeAvecNombre {
        ressource: &'static str,
    },
    RessourceSansPreuve {
        ressource: &'static str,
    },
    ReglageIncomplet {
        reglage: String,
        champ: &'static str,
    },
    BorneInvalide {
        quoi: &'static str,
    },
    // ---- Mémoire recommandée et crans (HRT-122) -------------------------------------------------
    CranInconnu {
        id: String,
    },
    CranEnDouble {
        id: String,
    },
    /// « Instable », « Lent » et « Recommandé » se déduisent des bornes de la fiche : ils ne s'écrivent pas.
    CranNonDeclarable {
        id: String,
    },
    CranBorneInvalide {
        id: String,
        raison: &'static str,
    },
    /// Preuve « mesuré au repos » ou « non mesuré » sur un cran de joueurs.
    CranPreuveInterdite {
        id: String,
    },
    PublieeSansMemoireRecommandee {
        raison: &'static str,
    },
}

impl Motif {
    /// L'invariant (I1 à I9) ou la famille (`format`, `coherence`) auquel le motif appartient.
    pub fn invariant(&self) -> &'static str {
        use Motif::*;
        match self {
            ChampInconnu { .. }
            | VarianteInconnue
            | GenreDeCanalInconnu
            | ChampManquant { .. }
            | ChampIncoherent { .. }
            | Syntaxe { .. }
            | FormatInattendu { .. }
            | OrigineNonSupportee => "format",
            ImageSansVersion
            | DepotInvalide
            | EmpreinteInvalide
            | PublieeSansEmpreinte
            | PortReserve { .. }
            | PortEnDouble { .. } => "I1",
            CheminInvalide { .. } | CibleEnDouble | DossierInconnu { .. } => "I2",
            ArgumentNonLitteral
            | SecretEnArgument
            | ArgumentHorsAlphabet { .. }
            | DeriveInvalide { .. }
            | OrdreInvalide { .. }
            | ValeurFixeInvalide { .. } => "I3",
            GenreDEtapeInconnu | SequenceIncoherente { .. } => "I4",
            AdresseNonHttps | AdresseInvalide => "I5",
            RepereInvalide { .. } => "I6",
            ConsolePubliee | ConsoleMotDePasseNonGenere | ConsoleIncoherente { .. } => "I7",
            ContratAccepteDAvance { .. } => "I8",
            BorneDepassee { .. } => "I9",
            IdentifiantInvalide { .. }
            | IdentifiantEnDouble { .. }
            | CleEnDouble { .. }
            | ListeDeVariantesVide
            | ReferenceInconnue { .. }
            | TropDEssentiels { .. }
            | ValeurProposeeInvalide { .. }
            | SecretAvecValeur { .. }
            | ReglageNameOnlyIncoherent { .. }
            | ReglageImposeEssentiel { .. }
            | RessourceNonMesureeAvecNombre { .. }
            | RessourceSansPreuve { .. }
            | ReglageIncomplet { .. }
            | BorneInvalide { .. }
            | CranInconnu { .. }
            | CranEnDouble { .. }
            | CranNonDeclarable { .. }
            | CranBorneInvalide { .. }
            | CranPreuveInterdite { .. }
            | PublieeSansMemoireRecommandee { .. } => "coherence",
        }
    }

    /// Phrase qui dit ce qui est refusé, sans jamais citer une valeur de réglage.
    pub fn explication(&self) -> String {
        use Motif::*;
        match self {
            ChampInconnu { champ } => format!("le champ `{champ}` n'existe pas dans le format de fiche (format fermé)"),
            VarianteInconnue => "valeur inconnue pour ce champ (énumération fermée)".into(),
            GenreDeCanalInconnu => "genre de canal inconnu (env, args ou file)".into(),
            GenreDEtapeInconnu => "genre d'étape inconnu (seuls stdin_line, wait_log et wait existent ; aucune étape n'exécute une commande)".into(),
            ChampManquant { champ } => format!("le champ `{champ}` manque"),
            ChampIncoherent { champ, avec } => format!("le champ `{champ}` n'a pas de sens {avec}"),
            Syntaxe { detail } => format!("fichier illisible : {detail}"),
            FormatInattendu { trouve } => format!("`format = {trouve}` : seule la version 2 est connue"),
            OrigineNonSupportee => "origine de fiche non prise en charge (un catalogue distant exigera une signature)".into(),
            ImageSansVersion => "l'image n'a pas de version précise (`latest` ou version vide)".into(),
            DepotInvalide => "dépôt d'image invalide (minuscules, chiffres, `.`, `_`, `-` et `/` seulement)".into(),
            EmpreinteInvalide => "empreinte d'image invalide (attendu `sha256:` suivi de 64 chiffres hexadécimaux minuscules)".into(),
            PublieeSansEmpreinte => "une fiche `published` exige l'empreinte de son image".into(),
            PortReserve { port } => format!("le port {port} est sous 1024 : Hearth ne le publie ni ne le fait écouter"),
            PortEnDouble { port } => format!("le port {port} est déclaré deux fois avec le même protocole"),
            CheminInvalide { ou } => format!("chemin invalide ({ou}) : ni absolu côté serveur, ni `..`, ni composant vide"),
            CibleEnDouble => "deux dossiers montent la même cible dans le service".into(),
            DossierInconnu { id } => format!("le dossier `{id}` n'est pas déclaré"),
            ArgumentNonLitteral => "le nom d'un argument doit être un littéral de la fiche (`-nom`)".into(),
            SecretEnArgument => "un secret ne passe jamais en argument de commande (lisible par tout lecteur du moteur)".into(),
            ArgumentHorsAlphabet { caractere } => format!("caractère {caractere:?} hors de l'alphabet fermé des arguments (lettres, chiffres, `.`, `-`, `_`, `:`)"),
            DeriveInvalide { raison } => format!("valeur dérivée invalide : {raison}"),
            OrdreInvalide { raison } => format!("ordre envoyé au jeu invalide : {raison}"),
            ValeurFixeInvalide { raison } => format!("valeur fixe invalide : {raison}"),
            SequenceIncoherente { raison } => format!("séquence incohérente : {raison}"),
            AdresseNonHttps => "une adresse de fiche doit commencer par `https://` (elle est seulement affichée, jamais contactée)".into(),
            AdresseInvalide => "adresse invalide".into(),
            RepereInvalide { raison } => format!("repère de journal invalide : {raison}"),
            ConsolePubliee => "une console distante n'est jamais publiée (`publish = \"never\"` obligatoire)".into(),
            ConsoleMotDePasseNonGenere => "le mot de passe d'une console distante est un secret généré par l'agent".into(),
            ConsoleIncoherente { raison } => format!("console distante incohérente : {raison}"),
            ContratAccepteDAvance { ou } => format!("la valeur d'acceptation d'un contrat est écrite par un autre chemin que le geste de l'utilisateur ({ou})"),
            BorneDepassee { quoi, limite, trouve } => format!("borne dépassée : {quoi} ({trouve} pour une limite de {limite})"),
            IdentifiantInvalide { ou } => format!("identifiant invalide ({ou}) : minuscules, chiffres, tirets"),
            IdentifiantEnDouble { genre, id } => format!("{genre} `{id}` déclaré deux fois"),
            CleEnDouble { canal, cle } => format!("la clé `{cle}` est réglée deux fois dans le canal `{canal}`"),
            ListeDeVariantesVide => "`only_variants` est vide : pour toutes les variantes, omettez le champ".into(),
            ReferenceInconnue { genre, id } => format!("{genre} `{id}` introuvable"),
            TropDEssentiels { trouve } => format!("{trouve} réglages `essential` pour 8 au plus (BR-FICHE-038)"),
            ValeurProposeeInvalide { reglage, raison } => format!("la valeur proposée ou par défaut du réglage `{reglage}` est invalide : {raison}"),
            SecretAvecValeur { reglage } => format!("le réglage secret `{reglage}` ne porte ni valeur proposée ni défaut non vide"),
            ReglageNameOnlyIncoherent { reglage } => format!("le réglage `{reglage}` est `name_only` : libellé fixe « Réglage du jeu, sans explication » et niveau `advanced` (BR-FICHE-037)"),
            ReglageImposeEssentiel { reglage } => format!("le réglage imposé `{reglage}` ne peut pas être `essential`"),
            RessourceNonMesureeAvecNombre { ressource } => format!("la ressource `{ressource}` est `unmeasured` : aucun nombre n'est écrit"),
            RessourceSansPreuve { ressource } => format!("la ressource `{ressource}` porte un nombre sans preuve"),
            ReglageIncomplet { reglage, champ } => format!("le réglage `{reglage}` n'a pas de `{champ}`"),
            BorneInvalide { quoi } => format!("borne invalide : {quoi}"),
            CranInconnu { id } => format!("cran de mémoire `{id}` inconnu (unstable, slow, recommended, smooth, ultra, community)"),
            CranEnDouble { id } => format!("cran de mémoire `{id}` déclaré deux fois"),
            CranNonDeclarable { id } => format!("le cran `{id}` se déduit de `min_bytes` et `recommended_bytes` : seuls smooth, ultra et community se déclarent"),
            CranBorneInvalide { id, raison } => format!("cran de mémoire `{id}` : {raison}"),
            CranPreuveInterdite { id } => format!("cran de mémoire `{id}` : seule la preuve `measured` ou `{{ publisher = \"https://...\" }}` est permise (BR-RES-033)"),
            PublieeSansMemoireRecommandee { raison } => format!("une fiche `published` exige une mémoire recommandée mesurée ou publiée par l'éditeur : {raison} (BR-RES-038)"),
        }
    }
}

/// Une fiche refusée : où, et pourquoi.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FicheRefusee {
    pub fichier: Fichier,
    /// Endroit dans le fichier (`channel[1]`, `setting[world_name]`...), vide pour le fichier entier.
    pub chemin: String,
    pub ligne: Option<usize>,
    pub motif: Motif,
}

impl FicheRefusee {
    pub fn new(fichier: Fichier, chemin: impl Into<String>, motif: Motif) -> Self {
        Self {
            fichier,
            chemin: chemin.into(),
            ligne: None,
            motif,
        }
    }

    /// Message de compilation : nomme la fiche (dossier), le fichier, l'invariant et le motif.
    pub fn message_pour(&self, dossier: &str) -> String {
        let mut out = format!(
            "fiche `{dossier}` refusée ({} / invariant {}) : {}",
            self.fichier.nom(),
            self.motif.invariant(),
            self.motif.explication()
        );
        if !self.chemin.is_empty() {
            out.push_str(&format!(" [{}]", self.chemin));
        }
        if let Some(ligne) = self.ligne {
            out.push_str(&format!(" (ligne {ligne})"));
        }
        out
    }
}

impl fmt::Display for FicheRefusee {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message_pour("?"))
    }
}

impl std::error::Error for FicheRefusee {}
