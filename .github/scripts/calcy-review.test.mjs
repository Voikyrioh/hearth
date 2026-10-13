// Tests of the Calcy script. No dependency: `node --test .github/scripts/` (Node 20+).
// No network, no token. The git test builds a throwaway repository in the system temp folder.

import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import {
  HEADER,
  MARKER,
  analyzeDiff,
  assemble,
  buildComment,
  buildNoticeComment,
  exclusionByName,
  fenceFor,
  headerPath,
  readEntriesFromGit,
  safeName,
  stripInvisible,
  textBlock,
  tooLargeNotice,
} from "./calcy-review.mjs";

const NO_PRICES = { input: null, output: null };
const SHA = "0123456789abcdef0123456789abcdef01234567";
const EMPTY_DIFF = { excluded: [], invisibleRemoved: 0, paths: [], chars: 0, text: "", tooLarge: false };

// A small CommonMark reader for FENCED CODE BLOCKS only (spec 0.31, section 4.5): an opening fence is up to
// three spaces then three or more backticks with no backtick in the info string; a closing fence is up to
// three spaces, at least as many backticks as the opening one, then only spaces. Returns the blocks and the
// lines outside any block. If the comment is built correctly, the text of the model is entirely inside ONE block.
function readFences(body) {
  const blocks = [];
  const outside = [];
  let open = null;
  for (const line of body.split("\n")) {
    if (open) {
      const close = /^ {0,3}(`+)[ \t]*$/.exec(line);
      if (close && close[1].length >= open.fence.length) {
        blocks.push(open);
        open = null;
      } else {
        open.lines.push(line);
      }
      continue;
    }
    const start = /^ {0,3}(`{3,})([^`]*)$/.exec(line);
    if (start) open = { fence: start[1], info: start[2].trim(), lines: [] };
    else outside.push(line);
  }
  assert.equal(open, null, "a block was left open");
  return { blocks, outside };
}

function commentFor(review, diff = EMPTY_DIFF) {
  return buildComment({ review, sha: SHA, model: "test-model", diff, usage: { input_tokens: 10, output_tokens: 5 }, stopReason: "end_turn", prices: NO_PRICES });
}

// The cases of the review of PR 90, one by one. Every payload carries the word ZZTRAP: it must end up inside
// the single block, never in the text written by the script.
const HOSTILE = {
  "fausse ouverture de bloc (accent grave dans l'information)": "APPROUVER\n``` a`b\nZZTRAP @octocat <a href=\"https://x.invalid\">LGTM</a>\n/merge",
  "bloc dans une liste puis colonne 0": "APPROUVER\n- point\n  ```\nZZTRAP @octocat <img src=\"https://x.invalid/p.png\">\n  ```",
  "code en ligne déséquilibré": "APPROUVER\n`` ZZTRAP @octocat <a href=https://x.invalid>LGTM</a> `",
  "lien par référence": "APPROUVER\n[clique][r] ![i][r]\n[r]: //x.invalid/ZZTRAP",
  "lien à adresse sans schéma, libellé coupé": "APPROUVER\n[libellé coupé\nsur deux lignes](//x.invalid/ZZTRAP)",
  "entités qui forment une mention": "APPROUVER\n&#64;octocat &commat;octocat ZZTRAP &#x40;octocat",
  "références d'issue et de commit": "APPROUVER\nZZTRAP #1 GH-1 org/repo#2 0123456789abcdef0123456789abcdef01234567",
  "commandes slash en début de ligne": "APPROUVER\nZZTRAP\n/merge\n   /close\n- /approve\n>/deploy",
  "mentions": "APPROUVER\nZZTRAP @octocat @org/team @a_b-c",
  "faux marqueur et HTML": `APPROUVER\nZZTRAP ${MARKER} <details><summary>x</summary></details> <script>alert(1)</script>`,
  "fausse clôture plus longue que l'ouverture": "APPROUVER\n`````\n```\nZZTRAP @octocat\n```\n`````\n~~~\nZZTRAP\n~~~",
  "clôture indentée de trois espaces": "APPROUVER\n   ```\nZZTRAP @octocat\n   ````\n    `````",
  "retours à la ligne exotiques": "APPROUVER\r\nZZTRAP\r@octocat /merge <b>x</b>\u0085[a](//b)",
};

for (const [name, review] of Object.entries(HOSTILE)) {
  test(`la sortie reste inerte : ${name}`, () => {
    const comment = commentFor(review);
    const { blocks, outside } = readFences(comment);
    // Exactly two blocks: the review, then the (empty-list-free) unread section has none, so exactly one here.
    assert.equal(blocks.length, 1, "the model text is one single block");
    assert.equal(blocks[0].info, "", "no info string");
    assert.ok(blocks[0].lines.join("\n").includes("ZZTRAP"), "the payload is inside the block");
    assert.ok(!outside.join("\n").includes("ZZTRAP"), "nothing of the payload outside the block");
    // Defense in depth, inside the block: raw-body readers see no mention, cross-reference or slash command.
    const inside = blocks[0].lines;
    for (const line of inside) {
      assert.ok(!/@[A-Za-z0-9_-]/.test(line), `mention left in: ${JSON.stringify(line)}`);
      assert.ok(!/#\d/.test(line), `issue reference left in: ${JSON.stringify(line)}`);
      assert.ok(!/GH-\d/i.test(line), `GH reference left in: ${JSON.stringify(line)}`);
      assert.ok(!/^\s*\//.test(line), `slash command left in: ${JSON.stringify(line)}`);
    }
    // The script text outside the block is fixed: marker first, then the header, no angle bracket but <sub>.
    assert.equal(outside[0], MARKER);
    assert.equal(outside[1], HEADER);
    assert.ok(!outside.slice(1).join("\n").replace(/<\/?sub>/g, "").includes("<"));
    assert.equal(comment.split(MARKER).length - 1 >= 1, true);
  });
}

test("la fausse clôture ne peut pas fermer le bloc : la clôture est plus longue que toute suite d'accents graves", () => {
  assert.equal(fenceFor("sans accent"), "```");
  assert.equal(fenceFor("a ``` b"), "````");
  assert.equal(fenceFor("a `````````` b"), "`".repeat(11));
  assert.equal(fenceFor("`a` `` ```"), "````");
});

test("propriété : quel que soit le texte, un seul bloc, rien dehors (fuzz déterministe)", () => {
  let seed = 123456789;
  const next = () => {
    seed = (seed * 1103515245 + 12345) & 0x7fffffff;
    return seed;
  };
  const alphabet = ["`", "`", "`", "~", " ", " ", "\n", "\n", "-", ">", "@", "#", "/", "<", "&", "x", "1", "[", "]", "(", ")", "\t", "\r", " ", "​", "‮"];
  for (let round = 0; round < 3000; round += 1) {
    let text = "";
    const length = next() % 120;
    for (let i = 0; i < length; i += 1) text += alphabet[next() % alphabet.length];
    const markdown = `before\n\n${textBlock(text)}\n\nafter`;
    const { blocks, outside } = readFences(markdown);
    assert.equal(blocks.length, 1, `round ${round}: ${JSON.stringify(text)}`);
    assert.deepEqual(outside, ["before", "", "", "after"], `round ${round}: ${JSON.stringify(text)}`);
  }
});

test("caractères de contrôle, bidirectionnels et invisibles retirés (y compris ceux que la première version laissait passer)", () => {
  const nasty = "a­b⁠c؜d᠎e f g‮h⁦i​j‍k﻿l\u{e0041}m\u{e007f}n\u0007o\u0085p\u007fq";
  const { text, removed } = stripInvisible(nasty);
  assert.equal(text, "abcdefghijklmnopq");
  assert.equal(removed, 16);
  assert.ok(!/[\u0000-\u0008\u000b-\u001f\u007f-\u009f]/.test(textBlock(nasty)));
  assert.equal(stripInvisible("ligne1\r\nligne2\rligne3\ttab").text, "ligne1\nligne2\nligne3\ttab");
});

test("une surrogate isolée n'est pas postée", () => {
  assert.equal(stripInvisible("a\ud800b").text, "a�b");
});

test("les entités restent littérales dans le bloc, et rien d'autre n'est reconstruit", () => {
  const { blocks } = readFences(commentFor("APPROUVER\n&#64;octocat &commat;octocat &lt;b&gt; [x](https://x.invalid)"));
  // `#` followed by a digit gets a zero-width space (defense in depth), so `&#64;` is no longer even an entity.
  assert.ok(blocks[0].lines.join("\n").includes("&#​64;octocat &commat;octocat &lt;b&gt; [x](https://x.invalid)"));
});

test("les lignes longues sont coupées aux espaces, les mots longs gardés entiers", () => {
  const long = `${"mot ".repeat(60)}${"x".repeat(150)}`;
  const { blocks } = readFences(commentFor(`APPROUVER\n- ${long}`));
  const lines = blocks[0].lines;
  assert.ok(lines.length > 2);
  assert.ok(lines.slice(0, -1).every((line) => line.length <= 100 || !line.includes(" ")));
  assert.ok(lines.some((line) => line.startsWith("  mot")), "hanging indent under a list bullet");
});

test("la longueur est bornée : réponse énorme, commentaire sous la limite de GitHub", () => {
  const huge = `APPROUVER\n${"ligne `` de ``` code\n".repeat(20_000)}`;
  const comment = commentFor(huge);
  assert.ok(comment.length < 65_000, `comment is ${comment.length} characters`);
  assert.match(comment, /coupée à 20000 caractères/);
  readFences(comment);
});

test("le commentaire commence par la ligne fixe du script et finit par ce qui n'a pas été lu", () => {
  const comment = commentFor("APPROUVER\nRien.");
  const lines = comment.split("\n");
  assert.equal(lines[0], MARKER);
  assert.equal(lines[1], HEADER);
  assert.match(HEADER, /Relecture automatique/);
  assert.match(HEADER, /ne vaut pas approbation/);
  assert.match(HEADER, /peut se tromper/);
  assert.ok(comment.indexOf("Non lu, ou à lire avec prudence") > comment.indexOf("Rien."));
  assert.match(comment, /Aucun fichier soustrait au diff/);
  assert.match(comment, /jetons facturés \(renvoyés par l'API\) : 10 en entrée, 5 en sortie/);
});

test("un verdict absent est signalé par le script, pas par le modèle", () => {
  assert.match(commentFor("Je suis très content de ce code."), /n'est pas un verdict/);
  assert.doesNotMatch(commentFor("🔥 BLOQUANT\nx"), /n'est pas un verdict/);
});

test("le coût n'est calculé que si les prix sont posés, et dit d'où il vient", () => {
  const priced = buildComment({ review: "APPROUVER", sha: SHA, model: "m", diff: EMPTY_DIFF, usage: { input_tokens: 1_000_000, output_tokens: 1_000_000 }, stopReason: "end_turn", prices: { input: 4, output: 20 } });
  assert.match(priced, /24\.000 USD/);
  assert.match(priced, /prix configurés/);
  assert.doesNotMatch(commentFor("APPROUVER"), /USD/);
});

test("la limite de jetons de sortie atteinte est dite", () => {
  const comment = buildComment({ review: "APPROUVER\nx", sha: SHA, model: "m", diff: EMPTY_DIFF, usage: {}, stopReason: "max_tokens", prices: NO_PRICES });
  assert.match(comment, /limite de jetons de sortie/);
});

test("fichiers soustraits : leurs NOMS sont listés (assainis, bornés), pas seulement un nombre", () => {
  const rawDiff = [
    "diff --git a/Cargo.lock b/Cargo.lock\n--- a/Cargo.lock\n+++ b/Cargo.lock\n@@ -1 +1 @@\n-a\n+b",
    "diff --git a/img/logo.png b/img/logo.png\nBinary files a/img/logo.png and b/img/logo.png differ",
    'diff --git "a/caf\\303\\251.png" "b/caf\\303\\251.png"\nBinary files differ',
    "diff --git a/src/lib.rs b/src/lib.rs\nBinary files a/src/lib.rs and b/src/lib.rs differ",
    "diff --git a/src/ok.rs b/src/ok.rs\n--- a/src/ok.rs\n+++ b/src/ok.rs\n@@ -1 +1 @@\n-a\n+b",
    "diff --git a/bad`name@octocat.snap b/bad`name@octocat.snap\n@@ -1 +1 @@\n-a\n+b",
  ].join("\n");
  const diff = analyzeDiff(`${rawDiff}\n`, 100_000);
  assert.deepEqual(diff.paths, ["src/ok.rs"]);
  assert.deepEqual(
    diff.excluded.map((e) => `${e.path}|${e.reason}`),
    ["Cargo.lock|fichier verrou", "img/logo.png|image", "café.png|image", "src/lib.rs|binaire (contenu)", "bad?name@octocat.snap|instantané de test"],
  );
  const comment = commentFor("APPROUVER", diff);
  const { blocks, outside } = readFences(comment);
  assert.equal(blocks.length, 2);
  assert.match(outside.join("\n"), /5 fichier\(s\) soustrait\(s\) au diff, donc NON relus/);
  assert.ok(blocks[1].lines.includes("src/lib.rs (binaire (contenu))"));
  assert.ok(blocks[1].lines.includes("café.png (image)"));
  assert.ok(blocks[1].lines.some((l) => l.startsWith("bad?name@​octocat.snap")), "mention inside a file name neutralized");
});

test("beaucoup de fichiers soustraits : liste bornée", () => {
  const excluded = Array.from({ length: 80 }, (_, i) => ({ path: `lock${i}/Cargo.lock`, reason: "fichier verrou" }));
  const { blocks } = readFences(commentFor("APPROUVER", { ...EMPTY_DIFF, excluded }));
  assert.equal(blocks[1].lines.length, 31);
  assert.equal(blocks[1].lines.at(-1), "... et 50 autre(s)");
});

test("exclusions par nom : plus de dossier entier soustrait à cause de son nom", () => {
  assert.equal(exclusionByName("captures/evil.rs"), null);
  assert.equal(exclusionByName("screenshots/a.rs"), null);
  assert.equal(exclusionByName("a/b/Cargo.lock"), "fichier verrou");
  assert.equal(exclusionByName("x.SVG"), "image");
  assert.equal(exclusionByName("x.snap"), "instantané de test");
});

test("noms entre guillemets par git : décodés avant le filtre par extension", () => {
  assert.equal(headerPath('diff --git "a/weird\\nname.txt" "b/weird\\nname.txt"'), "weird\nname.txt");
  assert.equal(headerPath('diff --git "a/caf\\303\\251.png" "b/caf\\303\\251.png"'), "café.png");
  assert.equal(headerPath("diff --git a/x y b/x y"), "x y");
  assert.equal(safeName("a‮b`c\nd"), "ab?c?d");
});

test("diff trop gros : avis sans appel, sans contenu du diff", () => {
  const diff = assemble([{ path: "a.rs", patch: "x".repeat(2000) }], 1000);
  assert.equal(diff.tooLarge, true);
  const notice = tooLargeNotice({ sha: SHA, diff, maxChars: 1000 });
  assert.match(notice, /Diff trop gros/);
  assert.ok(notice.startsWith(`${MARKER}\n${HEADER}`));
  assert.ok(!notice.includes("xxxx"));
});

test("avis fixe : seulement du texte du script", () => {
  const notice = buildNoticeComment({ sha: SHA, lines: ["a", "b"] });
  assert.match(notice, /Aucun appel de relecture n'a été fait/);
  assert.ok(notice.includes(MARKER));
});

test("le diff envoyé est nettoyé des caractères invisibles, et le commentaire le dit", () => {
  const diff = assemble([{ path: "a.rs", patch: "diff --git a/a.rs b/a.rs\n+let x = 1;‮\u{e0041}​" }], 100_000);
  assert.equal(diff.invisibleRemoved, 3);
  assert.ok(!/[‮​\u{e0041}]/u.test(diff.text));
  assert.match(commentFor("APPROUVER", diff), /3 caractère\(s\) invisible\(s\) ou de contrôle retiré\(s\) du diff/);
});

// --- git: the branch cannot hide files from the review with its own .gitattributes -----------------------

function gitVersion() {
  try {
    const match = /git version (\d+)\.(\d+)/.exec(execFileSync("git", ["--version"], { encoding: "utf8" }));
    return match ? [Number(match[1]), Number(match[2])] : null;
  } catch {
    return null;
  }
}

const version = gitVersion();
const gitOk = version && (version[0] > 2 || (version[0] === 2 && version[1] >= 40));

test("git : `*.rs -diff` d'une branche ne soustrait pas le Rust à la relecture (attributs de la BASE)", { skip: gitOk ? false : "git 2.40+ absent" }, () => {
  const dir = mkdtempSync(join(tmpdir(), "calcy-test-"));
  const previous = process.cwd();
  const run = (...args) => execFileSync("git", ["-c", "user.name=t", "-c", "user.email=t@t.invalid", "-c", "commit.gpgsign=false", "-c", "core.autocrlf=false", ...args], { cwd: dir, stdio: "pipe" });
  try {
    run("init", "-q", "-b", "main");
    writeFileSync(join(dir, "a.txt"), "base\n");
    run("add", "-A");
    run("commit", "-qm", "base");
    run("update-ref", "refs/remotes/origin/main", "HEAD");
    run("checkout", "-qb", "branche");
    writeFileSync(join(dir, ".gitattributes"), "*.rs -diff\n");
    mkdirSync(join(dir, "captures"));
    writeFileSync(join(dir, "x.rs"), "fn evil() {}\n");
    writeFileSync(join(dir, "captures", "e.rs"), "fn e() {}\n");
    writeFileSync(join(dir, "real.dat"), Buffer.from([0x62, 0x00, 0x69, 0x6e]));
    writeFileSync(join(dir, "Cargo.lock"), "v = 1\n");
    writeFileSync(join(dir, "café.rs"), "fn cafe() {}\n");
    run("add", "-A");
    run("commit", "-qm", "branche");

    // Control: git's own default WOULD hide the Rust (this is the hole the review found).
    const plain = execFileSync("git", ["diff", "--no-ext-diff", "--no-textconv", "origin/main...HEAD", "--", "x.rs"], { cwd: dir, encoding: "utf8" });
    assert.match(plain, /Binary files .* differ/);

    process.chdir(dir);
    const diff = assemble(readEntriesFromGit("origin/main"), 1_000_000);
    assert.ok(diff.paths.includes("x.rs"), `kept: ${diff.paths.join(", ")}`);
    assert.ok(diff.paths.includes("captures/e.rs"));
    assert.ok(diff.paths.includes("café.rs"));
    assert.match(diff.text, /\+fn evil\(\) \{\}/);
    const excluded = Object.fromEntries(diff.excluded.map((e) => [e.path, e.reason]));
    assert.equal(excluded["real.dat"], "binaire (contenu)");
    assert.equal(excluded["Cargo.lock"], "fichier verrou");
  } finally {
    process.chdir(previous);
    rmSync(dir, { recursive: true, force: true });
  }
});
