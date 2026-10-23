import { describe, expect, it } from "vitest";
import type { ServiceOperation } from "@/link";
import { type DeployDoor, deployDoor, routeOpen, submitReason } from "./availability";
import { emptyDraft, isFilled, movedVersion, valueShown, wipeDraft } from "./draft";
import { formIssues, readablePorts, shownVersion, toCheckInput, toRequest } from "./form";
import {
  deployOutcome,
  exitSummary,
  failureField,
  failureText,
  isFieldRefusal,
  portVerdict,
  refusalField,
  refusalText,
} from "./messages";
import {
  commandIssue,
  effectiveTag,
  envNameIssue,
  envValueIssue,
  imageIssue,
  looksSecret,
  MAX_COMMAND_ARG_CHARS,
  MAX_COMMAND_ARGS,
  MAX_COMMAND_CHARS,
  nameIssue,
  parsePort,
  splitCommand,
  splitImage,
  tagIssue,
} from "./rules";

// Les règles de saisie d'un déploiement (HRT-69) sont le MIROIR de celles de l'agent
// (`crates/hearth-agent/src/domain/services/{deploy,command}.rs`) : les vecteurs ci-dessous sont ceux de ses tests.

describe("le nom (BR-DEPLOY-003, de 2 à 63 caractères)", () => {
  it("accepte lettres sans accent, chiffres, tirets et points", () => {
    for (const name of ["web", "a1", "9lives", "my.service-2", "A".repeat(63)]) {
      expect(nameIssue(name), name).toBeNull();
    }
  });

  it("refuse chaque défaut avec son code", () => {
    expect(nameIssue("")).toBe("required");
    expect(nameIssue("a")).toBe("too_short");
    expect(nameIssue("a".repeat(64))).toBe("too_long");
    expect(nameIssue("-web")).toBe("first");
    expect(nameIssue(".web")).toBe("first");
    expect(nameIssue("café")).toBe("chars");
    expect(nameIssue("a b")).toBe("chars");
    expect(nameIssue("a_b")).toBe("chars");
    expect(nameIssue("é")).toBe("chars");
  });
});

describe("l'image et la version (BR-DEPLOY-005)", () => {
  it("sépare le dépôt et la version après le dernier « : » situé après le dernier « / »", () => {
    expect(splitImage("nginx:1.27")).toEqual({ repository: "nginx", embedded: "1.27" });
    expect(splitImage("registry:5000/team/app")).toEqual({
      repository: "registry:5000/team/app",
      embedded: null,
    });
    expect(splitImage("registry:5000/team/app:v2")).toEqual({
      repository: "registry:5000/team/app",
      embedded: "v2",
    });
    expect(splitImage("nginx:")).toEqual({ repository: "nginx", embedded: null });
  });

  it("accepte les références de Docker", () => {
    for (const image of [
      "nginx",
      "library/nginx",
      "ghcr.io/org/app:v2",
      "registry:5000/team/app:latest",
      "localhost/app",
      "a.b-c/d__e",
    ]) {
      expect(imageIssue(image), image).toBeNull();
    }
  });

  it("refuse une image mal formée, une empreinte, une image vide ou trop longue", () => {
    expect(imageIssue("")).toBe("required");
    expect(imageIssue("   ")).toBe("required");
    expect(imageIssue("nginx@sha256:abc")).toBe("digest");
    for (const image of ["Nginx", "ng inx", "a//b", "-a", "a..b", "a/-b", "a/b-"]) {
      expect(imageIssue(image), image).toBe("format");
    }
    expect(imageIssue("a".repeat(300))).toBe("too_long");
  });

  it("juge la version, vide permise (latest), et deux versions différentes", () => {
    expect(tagIssue("", "nginx")).toBeNull();
    expect(tagIssue("1.27", "nginx")).toBeNull();
    expect(tagIssue("_x.y-z", "nginx")).toBeNull();
    expect(tagIssue("bad tag", "nginx")).toBe("format");
    expect(tagIssue("-1", "nginx")).toBe("format");
    expect(tagIssue("a".repeat(129), "nginx")).toBe("too_long");
    expect(tagIssue("1.28", "nginx:1.27")).toBe("conflict");
    expect(tagIssue("1.27", "nginx:1.27")).toBeNull();
  });

  it("la version écrite dans l'image passe dans le champ Version ; vide vaut latest", () => {
    expect(movedVersion("nginx:1.27", "")).toEqual({ image: "nginx", tag: "1.27", moved: true });
    expect(movedVersion("nginx:1.27", "1.27").moved).toBe(true);
    // Deux versions différentes : on ne choisit pas pour l'utilisateur.
    expect(movedVersion("nginx:1.27", "1.28").moved).toBe(false);
    expect(movedVersion("nginx", "").moved).toBe(false);
    expect(effectiveTag("nginx", "")).toBe("latest");
    expect(effectiveTag("nginx:2", "")).toBe("2");
    expect(effectiveTag("nginx", " 3 ")).toBe("3");
  });
});

describe("les variables (BR-DEPLOY-006)", () => {
  it("juge le nom : lettres, chiffres, soulignés, pas de chiffre en premier", () => {
    for (const name of ["A_B1", "_X", "path"]) expect(envNameIssue(name), name).toBeNull();
    expect(envNameIssue("")).toBe("required");
    for (const name of ["1A", "A-B", "A B", "é"]) expect(envNameIssue(name), name).toBe("format");
    expect(envNameIssue("A".repeat(256))).toBe("too_long");
  });

  it("juge la valeur : une seule ligne, 8 192 caractères au plus, vide permise", () => {
    expect(envValueIssue("")).toBeNull();
    expect(envValueIssue("x".repeat(8192))).toBeNull();
    expect(envValueIssue("x".repeat(8193))).toBe("too_long");
    expect(envValueIssue("ligne\nsuivante")).toBe("multiline");
    expect(envValueIssue("nul\0")).toBe("multiline");
    expect(envValueIssue("cr\r")).toBe("multiline");
  });

  it("masque d'office la valeur d'un nom qui évoque un secret, sans tenir compte de la casse", () => {
    for (const name of [
      "DB_PASSWORD",
      "api_key",
      "Token",
      "PASS",
      "secret_sauce",
      "AWS_CREDENTIALS",
    ]) {
      expect(looksSecret(name), name).toBe(true);
    }
    for (const name of ["PORT", "HOME", "LOG_LEVEL"]) expect(looksSecret(name), name).toBe(false);
  });

  it("le choix de l'utilisateur l'emporte sur le nom", () => {
    expect(valueShown({ name: "DB_PASSWORD", reveal: null })).toBe(false);
    expect(valueShown({ name: "DB_PASSWORD", reveal: true })).toBe(true);
    expect(valueShown({ name: "PORT", reveal: null })).toBe(true);
    expect(valueShown({ name: "PORT", reveal: false })).toBe(false);
  });
});

describe("les ports (BR-DEPLOY-007)", () => {
  it("lit un entier de 1 à 65535", () => {
    expect(parsePort("8080")).toEqual({ port: 8080 });
    expect(parsePort(" 1 ")).toEqual({ port: 1 });
    expect(parsePort("65535")).toEqual({ port: 65535 });
    expect(parsePort("")).toEqual({ issue: "required" });
    for (const text of ["80a", "8.5", "-1", "1e3", "0x50"]) {
      expect(parsePort(text), text).toEqual({ issue: "integer" });
    }
    expect(parsePort("0")).toEqual({ issue: "range" });
    expect(parsePort("65536")).toEqual({ issue: "range" });
  });
});

describe("la commande de lancement (BR-DEPLOY-014), sans interpréteur", () => {
  const args = (line: string) => {
    const result = splitCommand(line);
    if ("issue" in result) throw new Error(result.issue);
    return result.args;
  };

  it("les espaces séparent et les guillemets regroupent", () => {
    expect(args("run --port 8080")).toEqual(["run", "--port", "8080"]);
    expect(args("  a   b  ")).toEqual(["a", "b"]);
    expect(args('sh -c "echo hi there"')).toEqual(["sh", "-c", "echo hi there"]);
    expect(args("say 'a \"b\" c'")).toEqual(["say", 'a "b" c']);
    // Des morceaux accolés ne font qu'un argument ; des guillemets vides font un argument vide.
    expect(args('a"b c"d')).toEqual(["ab cd"]);
    expect(args('x "" y')).toEqual(["x", "", "y"]);
  });

  it("la syntaxe d'un interpréteur est du texte ordinaire", () => {
    expect(args("a | b > c && d ; e $HOME `x` \\n")).toEqual([
      "a",
      "|",
      "b",
      ">",
      "c",
      "&&",
      "d",
      ";",
      "e",
      "$HOME",
      "`x`",
      "\\n",
    ]);
  });

  it("refuse un guillemet ouvert, un caractère de contrôle et les bornes dépassées", () => {
    expect(splitCommand('a "b')).toEqual({ issue: "quote" });
    expect(splitCommand("a 'b")).toEqual({ issue: "quote" });
    expect(splitCommand("a\0b")).toEqual({ issue: "control" });
    expect(splitCommand("a\u001b[31m")).toEqual({ issue: "control" });
    expect(splitCommand("a".repeat(MAX_COMMAND_CHARS + 1))).toEqual({ issue: "too_long" });
    expect(
      splitCommand(
        Array(MAX_COMMAND_ARGS + 1)
          .fill("x")
          .join(" "),
      ),
    ).toEqual({ issue: "args" });
    expect(args(Array(MAX_COMMAND_ARGS).fill("x").join(" "))).toHaveLength(MAX_COMMAND_ARGS);
    expect(splitCommand("y".repeat(MAX_COMMAND_ARG_CHARS + 1))).toEqual({ issue: "args" });
  });

  it("une commande vide ou faite d'espaces est la commande de l'image : pas un défaut", () => {
    expect(commandIssue("")).toBeNull();
    expect(commandIssue("   ")).toBeNull();
    expect(commandIssue('a "b')).toBe("quote");
    expect(commandIssue("tab\tet\nretour")).toBeNull();
  });
});

describe("le brouillon et ce qu'on en tire", () => {
  function filled() {
    const draft = emptyDraft();
    draft.name = "web";
    draft.image = "nginx:1.27";
    draft.env.push({ id: 1, name: "DB_PASSWORD", value: "SENTINEL-VALUE", reveal: null });
    draft.ports.push({ id: 2, host: "8080", container: "80", protocol: "tcp" });
    draft.command = "run --token SENTINEL-CMD";
    draft.resources = { memoryBytes: 1024 ** 3, cpuMillis: 1000 };
    return draft;
  }

  it("un brouillon vide n'est pas rempli ; un seul champ suffit", () => {
    expect(isFilled(emptyDraft())).toBe(false);
    const draft = emptyDraft();
    draft.tag = "1";
    expect(isFilled(draft)).toBe(true);
  });

  it("la consultation ne porte JAMAIS un secret : ni variable, ni commande (BR-DEPLOY-033)", () => {
    const input = toCheckInput(filled());
    expect(input).toEqual({
      name: "web",
      image: "nginx:1.27",
      tag: "1.27",
      ports: [{ hostPort: 8080, containerPort: 80, protocol: "tcp" }],
    });
    const wire = JSON.stringify(input);
    expect(wire).not.toContain("SENTINEL");
    expect(wire).not.toContain("DB_PASSWORD");
    expect(toCheckInput(emptyDraft())).toBeNull();
  });

  it("la demande reprend la saisie, sans « sans limite » : les ressources sont obligatoires", () => {
    const request = toRequest(filled(), { reservedPorts: true, lowDisk: false });
    expect(request).toMatchObject({
      name: "web",
      tag: "1.27",
      env: [{ name: "DB_PASSWORD", value: "SENTINEL-VALUE" }],
      command: "run --token SENTINEL-CMD",
      resources: { memoryBytes: 1024 ** 3, cpuMillis: 1000 },
      accept: { reservedPorts: true, lowDisk: false },
    });
    const draft = filled();
    draft.resources = null;
    expect(toRequest(draft, { reservedPorts: false, lowDisk: false })).toBeNull();
    draft.resources = { memoryBytes: null, cpuMillis: 1000 };
    expect(toRequest(draft, { reservedPorts: false, lowDisk: false })).toBeNull();
    // Un défaut de forme : rien ne part.
    const bad = filled();
    bad.name = "x";
    expect(toRequest(bad, { reservedPorts: false, lowDisk: false })).toBeNull();
  });

  it("une commande vide part comme « aucune commande » ; une version vide comme « aucune version »", () => {
    const draft = filled();
    draft.command = "  ";
    draft.image = "nginx";
    const request = toRequest(draft, { reservedPorts: false, lowDisk: false });
    expect(request?.command).toBeNull();
    expect(request?.tag).toBeNull();
    expect(shownVersion(draft)).toEqual({ tag: "latest", byDefault: true });
    draft.tag = "1.2";
    expect(shownVersion(draft)).toEqual({ tag: "1.2", byDefault: false });
  });

  it("repère les défauts de forme par rangée : doublons de variable et de port", () => {
    const draft = filled();
    draft.env.push({ id: 3, name: "DB_PASSWORD", value: "", reveal: null });
    draft.env.push({ id: 4, name: "1X", value: "ligne\nsuivante", reveal: null });
    draft.ports.push({ id: 5, host: "8080", container: "81", protocol: "tcp" });
    draft.ports.push({ id: 6, host: "8080", container: "81", protocol: "udp" });
    draft.ports.push({ id: 7, host: "", container: "x", protocol: "tcp" });
    const issues = formIssues(draft);
    expect(issues.env.map((row) => [row.id, row.name, row.value])).toEqual([
      [1, null, null],
      [3, "duplicate", null],
      [4, "format", "multiline"],
    ]);
    expect(issues.ports.map((row) => [row.id, row.host, row.container])).toEqual([
      [2, null, null],
      [5, "duplicate", null],
      [6, null, null],
      [7, "required", "integer"],
    ]);
    expect(issues.any).toBe(true);
    expect(readablePorts(draft).map((port) => port.id)).toEqual([2, 5, 6]);
    expect(formIssues(filled()).any).toBe(false);
  });

  it("effacer écrase les valeurs puis lâche tout", () => {
    const draft = filled();
    const row = draft.env[0];
    wipeDraft(draft);
    expect(isFilled(draft)).toBe(false);
    expect(row?.value).toBe("");
    expect(row?.name).toBe("");
    expect(draft.resources).toBeNull();
  });
});

describe("les constats du serveur sur un port (BR-DEPLOY-008, 009)", () => {
  const port = (patch: Partial<Parameters<typeof portVerdict>[0] & object>) => ({
    hostPort: 80,
    protocol: "tcp" as const,
    status: "free" as const,
    holder: null,
    suggested: null,
    privileged: false,
    ...patch,
  });

  it("un port libre ne dit rien", () => {
    expect(portVerdict(port({}))).toEqual({
      error: null,
      warning: null,
      reserved: false,
      suggested: null,
    });
    expect(portVerdict(undefined).error).toBeNull();
  });

  it("un service en marche, un conteneur hors Hearth (jamais nommé), un programme et l'agent bloquent et proposent un port libre", () => {
    expect(
      portVerdict(port({ status: "held_by_service", holder: "web", suggested: 81 })),
    ).toMatchObject({
      error: "Ce port est pris par le service « web »",
      suggested: 81,
    });
    const hidden = portVerdict(port({ status: "held_by_unmanaged", holder: "conteneur-secret" }));
    expect(hidden.error).toBe("Port tenu par un conteneur créé hors de Hearth");
    expect(JSON.stringify(hidden)).not.toContain("conteneur-secret");
    expect(portVerdict(port({ status: "held_by_program", holder: "sshd" })).error).toBe(
      "Ce port est occupé sur le serveur par « sshd »",
    );
    expect(portVerdict(port({ status: "held_by_program" })).error).toBe(
      "Ce port est occupé sur le serveur",
    );
    expect(portVerdict(port({ status: "agent_port" })).error).toBe(
      "Ce port est utilisé par Hearth lui-même.",
    );
  });

  it("un port réservé par un service arrêté avertit, demande la case du récapitulatif et ne bloque pas", () => {
    const verdict = portVerdict(port({ status: "reserved_by_stopped", holder: "base" }));
    expect(verdict.error).toBeNull();
    expect(verdict.reserved).toBe(true);
    expect(verdict.warning).toContain("réservé par le service arrêté « base »");
  });

  it("un texte du serveur est rendu inoffensif : caractères de contrôle et sens de lecture écartés", () => {
    const verdict = portVerdict(
      port({ status: "held_by_service", holder: "web\u202eevil\nsuite" }),
    );
    expect(verdict.error).not.toMatch(/[\u202e\n]/);
  });
});

describe("les refus de l'agent (BR-DEPLOY-030, 032, 035)", () => {
  it("un refus qui vise un champ y ramène ; les autres se disent dans la fenêtre", () => {
    expect(isFieldRefusal({ kind: "name_taken" })).toBe(true);
    expect(isFieldRefusal({ kind: "invalid", field: "image", reason: "bad_format" })).toBe(true);
    expect(isFieldRefusal({ kind: "deploy_limit" })).toBe(false);
    expect(isFieldRefusal({ kind: "no_space", freeBytes: 1 })).toBe(false);
    expect(refusalField({ kind: "name_being_deleted" })).toBe("name");
    expect(
      refusalField({
        kind: "port_in_use",
        port: 80,
        protocol: "tcp",
        status: "free",
        holder: null,
        suggested: null,
      }),
    ).toBe("ports");
    expect(refusalField({ kind: "limits_required" })).toBe("resources");
  });

  it("dit le port pris au dernier moment sans nommer un conteneur hors Hearth", () => {
    expect(
      refusalText({
        kind: "port_in_use",
        port: 8080,
        protocol: "tcp",
        status: "held_by_unmanaged",
        holder: "secret",
        suggested: 8081,
      }),
    ).toBe(
      "Le port 8080 vient d'être pris par un conteneur créé hors de Hearth. Rien n'a été créé.",
    );
    expect(refusalText({ kind: "deploy_limit" })).toBe(
      "Deux déploiements sont déjà en cours sur ce serveur. Attendez la fin de l'un d'eux.",
    );
    expect(refusalText({ kind: "setting_refused" })).toBe(
      "Ce réglage n'est pas permis pour un service créé par Hearth.",
    );
    expect(refusalText({ kind: "engine_unavailable", engine: "stopped" })).toContain(
      "Moteur arrêté",
    );
  });
});

describe("l'issue d'un déploiement, dite par sa cause (BR-DEPLOY-017, 019, 020)", () => {
  const op = (patch: Partial<ServiceOperation>): ServiceOperation => ({
    id: "01OP",
    key: "h_web",
    kind: "deploy",
    status: "failed",
    step: "pull",
    steps: [],
    percent: null,
    itemsDone: null,
    bytesDone: null,
    bytesTotal: null,
    etaS: null,
    cancellable: false,
    decision: null,
    cause: null,
    ...patch,
  });

  it("chaque cause de récupération a sa phrase, son fragment et son champ", () => {
    const table: [ServiceOperation["cause"], string, "name" | "image" | "tag" | "ports" | null][] =
      [
        ["image_not_found", "Image introuvable : vérifiez le nom de l'image.", "image"],
        ["tag_not_found", "Cette version n'existe pas pour cette image.", "tag"],
        [
          "image_private_or_missing",
          "Cette image est privée ou n'existe pas. Les images privées ne sont pas prises en charge pour l'instant.",
          "image",
        ],
        [
          "registry_unreachable",
          "Le serveur n'a pas pu récupérer l'image : il ne joint pas le registre. Vérifiez sa connexion à Internet.",
          null,
        ],
        [
          "registry_rate_limited",
          "Le registre limite les téléchargements anonymes. Réessayez plus tard.",
          null,
        ],
        [
          "platform_mismatch",
          "Cette image n'existe pas pour l'architecture de votre serveur.",
          "image",
        ],
        ["disk_full", "Le disque du serveur est plein : l'image n'a pas pu être récupérée.", null],
        ["engine_refused", "Le moteur a refusé de créer le service. Rien n'a été créé.", null],
        [
          "timeout",
          "La récupération de l'image prend trop de temps et a été arrêtée. Rien n'a été créé.",
          null,
        ],
        [
          "port_in_use",
          "Un port demandé vient d'être pris sur le serveur. Rien n'a été créé.",
          "ports",
        ],
        ["name_taken", "Ce nom est déjà pris. Rien n'a été créé.", "name"],
      ];
    for (const [cause, text, field] of table) {
      expect(failureText(op({ cause })), String(cause)).toBe(text);
      expect(failureField(op({ cause })), String(cause)).toBe(field);
    }
    // Une cause que ce client ne connaît pas n'invente rien.
    expect(failureText(op({ cause: "unknown" }))).toBe(
      "Le déploiement a échoué. Rien n'a été créé.",
    );
  });

  it("annonce le succès, l'échec en arrière-plan, l'annulation et le résultat inconnu", () => {
    expect(deployOutcome(op({ status: "succeeded" }), "web")).toEqual({
      message: "Le service web est en marche.",
      tone: "success",
    });
    expect(deployOutcome(op({ status: "failed", cause: "tag_not_found" }), "web")).toEqual({
      message: "Le déploiement de web a échoué : version introuvable.",
      tone: "error",
    });
    expect(
      deployOutcome(op({ status: "cancelled", cause: "cancelled_by_account" }), "web")?.message,
    ).toBe("Déploiement annulé. Rien n'a été créé.");
    expect(
      deployOutcome(op({ status: "interrupted", cause: "engine_unavailable" }), "web")?.tone,
    ).toBe("warn");
    expect(
      deployOutcome(op({ status: "succeeded", cause: "stopped_right_after_start" }), "web")?.tone,
    ).toBe("error");
    // Pas un déploiement, ou pas fini : ce n'est pas à ce module d'en parler.
    expect(deployOutcome(op({ kind: "update" }), "web")).toBeNull();
    expect(deployOutcome(op({ status: "running" }), "web")).toBeNull();
  });

  it("dit le code de sortie en minuscules pour entrer dans une phrase", () => {
    expect(exitSummary(137)).toBe("code 137 : coupé de force (arrêt forcé ou manque de mémoire)");
    expect(exitSummary(1)).toBe("code 1");
    expect(exitSummary(null)).toBe("");
  });
});

describe("le bouton « Déployer un service » et la route (BR-DEPLOY-001, 027, BR-GRANT-014)", () => {
  const allowed = { kind: "allowed" } as const;
  const denied = { kind: "denied", isAdmin: false, command: null } as const;

  it("est absent quand le moteur n'est pas utilisable", () => {
    for (const engine of [
      "absent",
      "unsupported_engine",
      "unsupported_version",
      "disabled",
    ] as const) {
      expect(deployDoor({ engine, gate: allowed }), engine).toEqual({ kind: "hidden" });
    }
    expect(deployDoor({ engine: null, gate: allowed })).toEqual({ kind: "hidden" });
    expect(deployDoor({ engine: "ready", gate: { kind: "engine-disabled" } })).toEqual({
      kind: "hidden",
    });
  });

  it("est visible et inactif sans le droit, quel que soit le moteur utilisable ; la raison est celle du droit", () => {
    expect(deployDoor({ engine: "ready", gate: denied })).toEqual({
      kind: "disabled",
      byRights: true,
      reason: null,
    });
    expect(deployDoor({ engine: "stopped", gate: { kind: "reading" } }).kind).toBe("disabled");
  });

  it("est inactif avec sa raison quand le moteur n'est pas prêt, actif quand il l'est", () => {
    expect(deployDoor({ engine: "stopped", gate: allowed })).toEqual({
      kind: "disabled",
      byRights: false,
      reason: "Indisponible : le moteur de conteneurs est arrêté.",
    });
    expect(deployDoor({ engine: "access_denied", gate: allowed })).toMatchObject({
      reason: "Indisponible : l'agent n'a pas le droit d'utiliser le moteur.",
    });
    expect(deployDoor({ engine: "unresponsive", gate: allowed })).toMatchObject({
      reason: "Indisponible : le moteur de conteneurs ne répond pas.",
    });
    expect(deployDoor({ engine: "ready", gate: allowed })).toEqual({ kind: "open" });
  });

  it("la route s'ouvre quand le bouton est actif ou quand seul le moteur manque", () => {
    const open = (door: DeployDoor) => routeOpen(door);
    expect(open({ kind: "open" })).toBe(true);
    expect(open({ kind: "disabled", byRights: false, reason: "x" })).toBe(true);
    expect(open({ kind: "disabled", byRights: true, reason: null })).toBe(false);
    expect(open({ kind: "hidden" })).toBe(false);
  });

  it("le motif du pied : le lien d'abord, puis le moteur, les bornes, le formulaire vide, les champs", () => {
    const base = {
      link: null,
      door: { kind: "open" } as DeployDoor,
      rightsLost: false,
      capacityReady: true,
      pristine: false,
      invalid: false,
    };
    expect(submitReason(base)).toBeNull();
    expect(
      submitReason({ ...base, link: "Indisponible tant que le serveur est hors ligne." })?.text,
    ).toBe("Indisponible tant que le serveur est hors ligne.");
    expect(
      submitReason({
        ...base,
        door: {
          kind: "disabled",
          byRights: false,
          reason: "Indisponible : le moteur de conteneurs est arrêté.",
        },
      })?.text,
    ).toBe("Indisponible : le moteur de conteneurs est arrêté.");
    expect(submitReason({ ...base, rightsLost: true })?.text).toBe(
      "Vous n'avez plus le droit de gérer les services de ce serveur.",
    );
    expect(submitReason({ ...base, capacityReady: false })?.text).toBe(
      "Lecture des limites du serveur en cours.",
    );
    expect(submitReason({ ...base, pristine: true })?.text).toBe(
      "Renseignez le nom et l'image pour continuer.",
    );
    expect(submitReason({ ...base, invalid: true })).toEqual({
      text: "Corrigez les champs signalés pour continuer.",
      fixable: true,
    });
  });
});
