//! Trouver les mondes d'un dossier ouvert, puis mesurer et planifier la copie d'un seul (HRT-105,
//! `BR-MONDE-003`, `005`, `006`, `012`). Lecture seule : ne parle qu'à un [`WorldSource`].

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

use crate::application::ports::{WorldEntryKind, WorldFsError, WorldSource};
use crate::domain::files::RelPath;
use crate::domain::worlds::{
    MAX_DEPTH, MAX_LISTED, MAX_WORLD_ITEMS, Shape, WorldRules, looks_like_backup,
};

/// Un monde trouvé, sans rien d'ouvert.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoundWorld {
    pub rel: RelPath,
    pub name: String,
    pub files: Option<u64>,
    pub bytes: Option<u64>,
    pub mtime_ns: Option<i128>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Found {
    pub worlds: Vec<FoundWorld>,
    /// La recherche s'est arrêtée à la borne : des mondes peuvent manquer (`BR-MONDE-003`).
    pub truncated: bool,
}

/// Cherche les mondes sous la racine : trois niveaux et `max_entries` éléments au plus (`BR-MONDE-003`). Un
/// élément dont le nom évoque une sauvegarde n'est ni un monde ni un dossier où descendre (`BR-MONDE-005`).
pub fn find_worlds(
    source: &dyn WorldSource,
    rules: &WorldRules,
    max_entries: usize,
) -> Result<Found, WorldFsError> {
    let mut found = Found::default();
    let mut seen = 0usize;
    let mut queue: VecDeque<(RelPath, usize)> = VecDeque::new();
    queue.push_back((RelPath::root(), 1));
    while let Some((dir, depth)) = queue.pop_front() {
        let remaining = max_entries.saturating_sub(seen);
        if remaining == 0 {
            found.truncated = true;
            break;
        }
        let listing = match source.list(&dir, remaining) {
            Ok(listing) => listing,
            // Un sous-dossier disparu, remplacé par un lien ou illisible : ignoré. La racine illisible, non.
            Err(_) if !dir.is_root() => continue,
            Err(error) => return Err(error),
        };
        found.truncated |= listing.truncated;
        seen += listing.entries.len();
        let by_name: HashMap<&str, &crate::application::ports::WorldDirEntry> = listing
            .entries
            .iter()
            .map(|entry| (entry.name.as_str(), entry))
            .collect();
        for entry in &listing.entries {
            let is_dir = match entry.kind {
                WorldEntryKind::File => false,
                WorldEntryKind::Dir => true,
                WorldEntryKind::Link | WorldEntryKind::Other => continue,
            };
            if looks_like_backup(&entry.name) {
                continue;
            }
            if rules.is_world(depth, is_dir, &entry.name) {
                let (Some(name), Ok(rel)) = (rules.world_name(&entry.name), dir.child(&entry.name))
                else {
                    continue;
                };
                let mut world = FoundWorld {
                    rel,
                    name: name.to_owned(),
                    files: None,
                    bytes: None,
                    mtime_ns: Some(entry.mtime_ns),
                };
                if !is_dir {
                    let (mut files, mut bytes, mut mtime) = (1u64, entry.size, entry.mtime_ns);
                    for companion in rules.companion_names(&entry.name) {
                        if looks_like_backup(&companion) {
                            continue;
                        }
                        if let Some(c) = by_name
                            .get(companion.as_str())
                            .filter(|c| c.kind == WorldEntryKind::File)
                        {
                            files += 1;
                            bytes = bytes.saturating_add(c.size);
                            mtime = mtime.max(c.mtime_ns);
                        }
                    }
                    world.files = Some(files);
                    world.bytes = Some(bytes);
                    world.mtime_ns = Some(mtime);
                }
                found.worlds.push(world);
            } else if is_dir
                && depth < wanted_depth(rules)
                && let Ok(child) = dir.child(&entry.name)
            {
                queue.push_back((child, depth + 1));
            }
        }
    }
    found.worlds.sort_by_key(|world| world.rel.to_string());
    Ok(found)
}

/// Jusqu'où descendre : trois niveaux pour un monde fichier, la profondeur de la fiche pour un monde dossier
/// (un monde dossier trouvé n'est pas descendu).
fn wanted_depth(rules: &WorldRules) -> usize {
    match &rules.shape {
        Shape::File { .. } => MAX_DEPTH,
        Shape::Dir { depth } => *depth,
    }
}

/// Un fichier à copier : d'où il vient, où il va dans le dossier d'import, ce qu'on en savait.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanFile {
    pub src: RelPath,
    pub dst: RelPath,
    pub size: u64,
    pub mtime_ns: i128,
}

/// Ce que la copie d'un monde ferait et laisserait (`BR-MONDE-006`, `009`, `012`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Plan {
    pub files: Vec<PlanFile>,
    /// Dossiers à créer dans le dossier d'import, parents avant enfants.
    pub dirs: Vec<RelPath>,
    pub bytes: u64,
    pub not_copied: u64,
    pub not_copied_names: Vec<String>,
    pub skipped_links: u64,
    pub skipped_link_names: Vec<String>,
    /// La mesure s'est arrêtée à son budget : les chiffres sont « au moins ».
    pub partial: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanError {
    /// Ce chemin n'est pas un monde de cette fiche.
    NotAWorld,
    /// Plus d'éléments que la borne de mémoire de la copie.
    TooBig,
    Fs(WorldFsError),
}

impl From<WorldFsError> for PlanError {
    fn from(error: WorldFsError) -> Self {
        Self::Fs(error)
    }
}

impl Plan {
    fn skip_name(&mut self, link: bool, name: &str) {
        let (count, names) = if link {
            (&mut self.skipped_links, &mut self.skipped_link_names)
        } else {
            (&mut self.not_copied, &mut self.not_copied_names)
        };
        *count += 1;
        if names.len() < MAX_LISTED {
            names.push(name.to_owned());
        }
    }

    fn add_file(&mut self, src: RelPath, dst: RelPath, size: u64, mtime_ns: i128) {
        self.bytes = self.bytes.saturating_add(size);
        self.files.push(PlanFile {
            src,
            dst,
            size,
            mtime_ns,
        });
    }
}

/// Planifie la copie du monde `world` (un chemin relatif à la racine ouverte). Les fichiers d'un monde fichier
/// sont mis à plat dans le dossier d'import ; un monde dossier garde son arborescence. Ne lit que des noms.
pub fn plan_world(
    source: &dyn WorldSource,
    rules: &WorldRules,
    world: &RelPath,
    budget: Duration,
) -> Result<Plan, PlanError> {
    let started = Instant::now();
    let (leaf, _) = world.split_last().ok_or(PlanError::NotAWorld)?;
    // Un monde rangé dans un dossier de sauvegardes n'est jamais une source (`BR-MONDE-005`).
    if world.components().any(looks_like_backup) {
        return Err(PlanError::NotAWorld);
    }
    let depth = world.depth();
    let mut plan = Plan::default();
    match &rules.shape {
        Shape::File { .. } => {
            let parent = world.parent().ok_or(PlanError::NotAWorld)?;
            // Un lien ou un fichier spécial n'est pas un monde (`NotRegular`) ; rien n'est listé pour le trouver.
            let stat = match source.stat(world) {
                Ok(stat) => stat,
                Err(WorldFsError::NotRegular) => return Err(PlanError::NotAWorld),
                Err(error) => return Err(PlanError::Fs(error)),
            };
            if !rules.is_world(depth, false, leaf) {
                return Err(PlanError::NotAWorld);
            }
            let flat = |name: &str| {
                RelPath::root()
                    .child(name)
                    .map_err(|_| PlanError::NotAWorld)
            };
            plan.add_file(world.clone(), flat(leaf)?, stat.size, stat.mtime_ns);
            for companion in rules.companion_names(leaf) {
                let src = parent.child(&companion).map_err(|_| PlanError::NotAWorld)?;
                // Les compagnons sont cherchés par leur nom exact, jamais en listant le dossier.
                let found = match source.stat(&src) {
                    Ok(found) => found,
                    Err(WorldFsError::NotFound) => continue,
                    // Un compagnon qui est un lien ou un fichier spécial n'est pas copié et est dit (`BR-MONDE-006`).
                    Err(WorldFsError::NotRegular) => {
                        plan.skip_name(true, &companion);
                        continue;
                    }
                    Err(error) => return Err(PlanError::Fs(error)),
                };
                if looks_like_backup(&companion) {
                    plan.skip_name(false, &companion);
                    continue;
                }
                plan.add_file(src, flat(&companion)?, found.size, found.mtime_ns);
            }
        }
        Shape::Dir { .. } => {
            let parent = world.parent().ok_or(PlanError::NotAWorld)?;
            let listing = source.list(&parent, MAX_WORLD_ITEMS)?;
            let entry = listing
                .entries
                .iter()
                .find(|entry| entry.name == leaf)
                .ok_or(PlanError::Fs(WorldFsError::NotFound))?;
            if entry.kind != WorldEntryKind::Dir || !rules.is_world(depth, true, leaf) {
                return Err(PlanError::NotAWorld);
            }
            let mut stack = vec![(world.clone(), RelPath::root())];
            let mut items = 0usize;
            while let Some((src_dir, dst_dir)) = stack.pop() {
                if started.elapsed() >= budget {
                    plan.partial = true;
                    break;
                }
                let listing = source.list(&src_dir, MAX_WORLD_ITEMS.saturating_sub(items) + 1)?;
                items += listing.entries.len();
                if items > MAX_WORLD_ITEMS || listing.truncated {
                    return Err(PlanError::TooBig);
                }
                for entry in &listing.entries {
                    let (Ok(src), Ok(dst)) =
                        (src_dir.child(&entry.name), dst_dir.child(&entry.name))
                    else {
                        // Un nom ou une profondeur que Hearth ne recrée pas : jamais copié, dit comme tel.
                        plan.skip_name(true, &entry.name);
                        continue;
                    };
                    match entry.kind {
                        WorldEntryKind::File if looks_like_backup(&entry.name) => {
                            plan.skip_name(false, &entry.name);
                        }
                        WorldEntryKind::File => plan.add_file(src, dst, entry.size, entry.mtime_ns),
                        WorldEntryKind::Dir if looks_like_backup(&entry.name) => {
                            plan.skip_name(false, &entry.name);
                        }
                        WorldEntryKind::Dir => {
                            plan.dirs.push(dst.clone());
                            stack.push((src, dst));
                        }
                        WorldEntryKind::Link | WorldEntryKind::Other => {
                            plan.skip_name(true, &entry.name);
                        }
                    }
                }
            }
        }
    }
    Ok(plan)
}

#[cfg(test)]
pub(crate) mod fake {
    //! Une source et une destination en mémoire, pour les tests des cas d'usage.

    use std::collections::BTreeMap;
    use std::io::{self, Cursor, Read, Write};
    use std::sync::{Arc, Mutex};

    use crate::application::ports::{
        SinkFile, WorldDirEntry, WorldDirListing, WorldEntryKind, WorldFileStat, WorldFsError,
        WorldSink, WorldSource,
    };
    use crate::domain::files::RelPath;

    #[derive(Clone, Debug)]
    pub enum Node {
        File { bytes: Vec<u8>, mtime_ns: i128 },
        Dir,
        Link,
    }

    /// Un arbre `chemin -> noeud` ; la racine est implicite.
    #[derive(Clone, Default)]
    pub struct Tree(pub Arc<Mutex<BTreeMap<String, Node>>>);

    impl Tree {
        pub fn put_file(&self, path: &str, bytes: &[u8], mtime_ns: i128) {
            self.0.lock().expect("lock").insert(
                path.to_owned(),
                Node::File {
                    bytes: bytes.to_vec(),
                    mtime_ns,
                },
            );
        }

        pub fn put(&self, path: &str, node: Node) {
            self.0.lock().expect("lock").insert(path.to_owned(), node);
        }

        pub fn snapshot(&self) -> BTreeMap<String, String> {
            self.0
                .lock()
                .expect("lock")
                .iter()
                .map(|(k, v)| (k.clone(), format!("{v:?}")))
                .collect()
        }

        fn children(&self, dir: &RelPath) -> Vec<(String, Node)> {
            let prefix = if dir.is_root() {
                String::new()
            } else {
                format!("{dir}/")
            };
            self.0
                .lock()
                .expect("lock")
                .iter()
                .filter_map(|(path, node)| {
                    let rest = path.strip_prefix(&prefix)?;
                    (!rest.contains('/') && !rest.is_empty())
                        .then(|| (rest.to_owned(), node.clone()))
                })
                .collect()
        }
    }

    impl WorldSource for Tree {
        fn list(&self, dir: &RelPath, limit: usize) -> Result<WorldDirListing, WorldFsError> {
            let mut entries: Vec<WorldDirEntry> = self
                .children(dir)
                .into_iter()
                .map(|(name, node)| match node {
                    Node::File { bytes, mtime_ns } => WorldDirEntry {
                        name,
                        kind: WorldEntryKind::File,
                        size: bytes.len() as u64,
                        mtime_ns,
                    },
                    Node::Dir => WorldDirEntry {
                        name,
                        kind: WorldEntryKind::Dir,
                        size: 0,
                        mtime_ns: 0,
                    },
                    Node::Link => WorldDirEntry {
                        name,
                        kind: WorldEntryKind::Link,
                        size: 0,
                        mtime_ns: 0,
                    },
                })
                .collect();
            let truncated = entries.len() > limit;
            entries.truncate(limit);
            Ok(WorldDirListing { entries, truncated })
        }

        fn stat(&self, file: &RelPath) -> Result<WorldFileStat, WorldFsError> {
            match self.0.lock().expect("lock").get(&file.to_string()) {
                Some(Node::File { bytes, mtime_ns }) => Ok(WorldFileStat {
                    size: bytes.len() as u64,
                    mtime_ns: *mtime_ns,
                }),
                Some(_) => Err(WorldFsError::NotRegular),
                None => Err(WorldFsError::NotFound),
            }
        }

        fn open(&self, file: &RelPath) -> Result<Box<dyn Read + Send>, WorldFsError> {
            match self.0.lock().expect("lock").get(&file.to_string()) {
                Some(Node::File { bytes, .. }) => Ok(Box::new(Cursor::new(bytes.clone()))),
                Some(_) => Err(WorldFsError::NotRegular),
                None => Err(WorldFsError::NotFound),
            }
        }
    }

    struct Writer {
        tree: Tree,
        path: String,
        buf: Vec<u8>,
        /// Corrompt l'octet 0 à l'écriture : la relecture doit le voir.
        corrupt: bool,
    }

    impl Write for Writer {
        fn write(&mut self, data: &[u8]) -> io::Result<usize> {
            self.buf.extend_from_slice(data);
            Ok(data.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl SinkFile for Writer {
        fn finish(mut self: Box<Self>) -> io::Result<()> {
            if self.corrupt
                && let Some(first) = self.buf.first_mut()
            {
                *first ^= 0xFF;
            }
            self.tree.put_file(&self.path, &self.buf, 0);
            Ok(())
        }
    }

    /// Une destination en mémoire ; `corrupt` : toute écriture est altérée.
    #[derive(Clone, Default)]
    pub struct Sink {
        pub tree: Tree,
        pub corrupt: bool,
    }

    impl WorldSink for Sink {
        fn mkdir(&self, dir: &RelPath) -> Result<(), WorldFsError> {
            self.tree.put(&dir.to_string(), Node::Dir);
            Ok(())
        }

        fn create(&self, file: &RelPath) -> Result<Box<dyn SinkFile>, WorldFsError> {
            Ok(Box::new(Writer {
                tree: self.tree.clone(),
                path: file.to_string(),
                buf: Vec::new(),
                corrupt: self.corrupt,
            }))
        }

        fn open(&self, file: &RelPath) -> Result<Box<dyn Read + Send>, WorldFsError> {
            WorldSource::open(&self.tree, file)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fake::{Node, Tree};
    use super::*;
    use crate::domain::worlds::MAX_ENTRIES;

    fn rules(companions: &[&str]) -> WorldRules {
        WorldRules {
            shape: Shape::File {
                suffix: ".wld".into(),
                companions: companions.iter().map(|c| (*c).to_owned()).collect(),
            },
            names_setting: "world_name".into(),
            carried: vec![],
            folder_source: "w".into(),
        }
    }

    fn tree() -> Tree {
        let tree = Tree::default();
        tree.put("Worlds", Node::Dir);
        tree.put_file("Worlds/Alpha.wld", b"alpha", 10);
        tree.put_file("Worlds/Alpha.wld.meta", b"meta", 20);
        tree.put_file("Worlds/Alpha.wld.bak", b"old", 30);
        tree.put_file("Worlds/notes.txt", b"x", 1);
        tree.put("Worlds/Deep", Node::Dir);
        tree.put("Worlds/Deep/Er", Node::Dir);
        tree.put("Worlds/Deep/Er/Est", Node::Dir);
        tree.put_file("Worlds/Deep/Er/Est/TooDeep.wld", b"z", 5);
        tree.put("Worlds/Backups", Node::Dir);
        tree.put_file("Worlds/Backups/Old.wld", b"o", 5);
        tree.put("Worlds/link.wld", Node::Link);
        tree.put_file("Beta.wld", b"beta!", 40);
        tree
    }

    #[test]
    fn the_search_lists_worlds_within_three_levels_and_skips_backups_and_links() {
        let found = find_worlds(&tree(), &rules(&[".meta"]), MAX_ENTRIES).expect("trouvés");
        let names: Vec<&str> = found.worlds.iter().map(|w| w.name.as_str()).collect();
        assert_eq!(names, vec!["Beta", "Alpha"], "{found:?}");
        assert!(!found.truncated);
        let alpha = &found.worlds[1];
        assert_eq!(alpha.rel.to_string(), "Worlds/Alpha.wld");
        assert_eq!(alpha.files, Some(2), "le monde et son compagnon");
        assert_eq!(alpha.bytes, Some(9));
        assert_eq!(alpha.mtime_ns, Some(20));
    }

    #[test]
    fn the_search_stops_at_the_entry_bound_and_says_so() {
        let found = find_worlds(&tree(), &rules(&[]), 3).expect("trouvés");
        assert!(found.truncated);
    }

    #[test]
    fn the_plan_copies_the_world_and_its_companions_flat_and_lists_what_it_leaves() {
        let plan = plan_world(
            &tree(),
            &rules(&[".meta", ".bak"]),
            &RelPath::parse("Worlds/Alpha.wld").expect("rel"),
            Duration::from_secs(60),
        )
        .expect("plan");
        let dst: Vec<String> = plan.files.iter().map(|f| f.dst.to_string()).collect();
        assert_eq!(dst, vec!["Alpha.wld", "Alpha.wld.meta"]);
        assert_eq!(plan.bytes, 9);
        assert_eq!(plan.not_copied, 1);
        assert_eq!(plan.not_copied_names, vec!["Alpha.wld.bak".to_owned()]);
        assert_eq!(plan.skipped_links, 0);
    }

    #[test]
    fn a_companion_that_is_a_link_is_listed_not_copied() {
        let tree = tree();
        tree.put("Worlds/Alpha.wld.lnk", Node::Link);
        let plan = plan_world(
            &tree,
            &rules(&[".lnk"]),
            &RelPath::parse("Worlds/Alpha.wld").expect("rel"),
            Duration::from_secs(60),
        )
        .expect("plan");
        assert_eq!(plan.files.len(), 1);
        assert_eq!(plan.skipped_links, 1);
        assert_eq!(plan.skipped_link_names, vec!["Alpha.wld.lnk".to_owned()]);
    }

    #[test]
    fn a_path_that_is_not_a_world_of_the_fiche_is_refused() {
        for rel in [
            "Worlds/notes.txt",
            "Worlds",
            "Worlds/Backups/Old.wld",
            "Worlds/link.wld",
        ] {
            let outcome = plan_world(
                &tree(),
                &rules(&[]),
                &RelPath::parse(rel).expect("rel"),
                Duration::from_secs(60),
            );
            assert!(outcome.is_err(), "{rel}: {outcome:?}");
        }
    }

    #[test]
    fn a_dir_world_keeps_its_tree_and_lists_backups_and_links_apart() {
        let tree = Tree::default();
        tree.put("Saves", Node::Dir);
        tree.put("Saves/Game1", Node::Dir);
        tree.put_file("Saves/Game1/level.dat", b"12345", 1);
        tree.put("Saves/Game1/sub", Node::Dir);
        tree.put_file("Saves/Game1/sub/a.dat", b"1", 1);
        tree.put("Saves/Game1/backup-1", Node::Dir);
        tree.put("Saves/Game1/lnk", Node::Link);
        let rules = WorldRules {
            shape: Shape::Dir { depth: 2 },
            ..rules(&[])
        };
        let plan = plan_world(
            &tree,
            &rules,
            &RelPath::parse("Saves/Game1").expect("rel"),
            Duration::from_secs(60),
        )
        .expect("plan");
        let dst: Vec<String> = plan.files.iter().map(|f| f.dst.to_string()).collect();
        assert_eq!(dst, vec!["level.dat", "sub/a.dat"]);
        assert_eq!(
            plan.dirs
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            vec!["sub"]
        );
        assert_eq!((plan.not_copied, plan.skipped_links), (1, 1));
    }
}
