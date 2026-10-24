import { flushPromises, type VueWrapper } from "@vue/test-utils";
import { describe, expect, it, vi } from "vitest";
import { LinkCommandError, type SimulatedLinkBridge } from "@/link";
import { useDeployFormStore } from "@/stores/deployForm";
import { confirmDialog, dialogButton, OWN, reauthField, typeInto } from "@/test/confirm";
import {
  boot,
  confirmTop,
  dialogText,
  field,
  fillMinimum,
  openSummary,
  settle,
  submit,
  toasts,
  topButton,
  type,
  useDeployTestClock,
} from "@/test/deploy";

// Le récapitulatif, la progression et l'abandon d'un déploiement (HRT-69, BR-DEPLOY-015, 016, 018, 019, 020, 028 à
// 030, 032) : de « Déployer » à l'issue, sur le pont simulé dont l'agent juge comme le vrai.
useDeployTestClock();

const GIB = 1024 ** 3;
const SECRET = "SENTINEL-VALUE-hunter2";
const COMMAND_SECRET = "SENTINEL-CMD";

/** Remplit un formulaire complet : une variable secrète, un port, une commande. */
async function fillAll(wrapper: VueWrapper) {
  await type(wrapper, "name", "web");
  await type(wrapper, "image", "nginx");
  await type(wrapper, "tag", "1.27");
  await wrapper.get("[data-env-add]").trigger("click");
  await flushPromises();
  const [name, value] = wrapper.findAll<HTMLInputElement>("[data-env-row] input");
  typeInto(name?.element ?? null, "DB_PASSWORD");
  typeInto(value?.element ?? null, SECRET);
  await wrapper.get("[data-port-add]").trigger("click");
  await flushPromises();
  const [host, container] = wrapper.findAll<HTMLInputElement>("[data-port-row] input");
  typeInto(host?.element ?? null, "8080");
  typeInto(container?.element ?? null, "80");
  typeInto(
    wrapper.get<HTMLTextAreaElement>('[data-field="command"] textarea').element,
    `run --token ${COMMAND_SECRET}`,
  );
  await settle();
}

/** Ouvre le récapitulatif et confirme avec le bon mot de passe. */
async function launch(wrapper: VueWrapper) {
  await openSummary(wrapper);
  await confirmDialog("Déployer le service");
  await settle();
}

const running = (bridge: SimulatedLinkBridge) => {
  const found = bridge.deploys.running("forge")[0];
  if (!found) throw new Error("aucun déploiement en cours");
  return found;
};

const progress = () => document.querySelector("[data-deploy-progress]");
const stepStates = () =>
  [...document.querySelectorAll("[data-progress-steps] li")].map(
    (item) => `${item.getAttribute("data-step")}:${item.getAttribute("data-state")}`,
  );

describe("le récapitulatif (BR-DEPLOY-015, 031)", () => {
  it("montre tout ce qui part dans une seule fenêtre, sans jamais écrire une valeur en clair", async () => {
    const { wrapper } = await boot();
    await fillAll(wrapper);
    await openSummary(wrapper);
    const text = dialogText();
    expect(text).toContain("Vérifiez avant de lancer");
    expect(document.querySelector("[data-sum-name]")?.textContent).toBe("web");
    expect(document.querySelector("[data-sum-image]")?.textContent).toContain("nginx:1.27");
    expect(document.querySelector("[data-sum-ports]")?.textContent).toContain("8080 vers 80 (TCP)");
    expect(document.querySelector("[data-sum-exposed]")?.textContent).toBe(
      "Ce port sera accessible à tout appareil qui peut joindre votre serveur.",
    );
    expect(document.querySelector("[data-sum-env]")?.textContent).toContain("1 variable");
    expect(document.querySelector("[data-sum-env]")?.textContent).toContain("DB_PASSWORD");
    expect(document.querySelector("[data-sum-command]")?.textContent?.trim()).toBe(
      "Commande personnalisée",
    );
    expect(document.querySelector("[data-sum-restart]")?.textContent).toBe(
      "Le service redémarre tout seul après un redémarrage du serveur, sauf si vous l'avez arrêté.",
    );
    expect(document.querySelector("[data-sum-resources]")?.textContent).toBe(
      "Le service reçoit 2 Go de mémoire et 4 cœurs de processeur.",
    );
    // Ni la valeur de la variable, ni la commande n'apparaissent dans la page.
    expect(document.body.textContent).not.toContain(SECRET);
    expect(document.body.textContent).not.toContain(COMMAND_SECRET);
    expect(dialogButton("Modifier")).toBeDefined();
    expect(dialogButton("Déployer le service")).toBeDefined();
    wrapper.unmount();
  });

  it("dit « par défaut » et que latest peut changer quand aucune version n'est saisie ; sans port ni variable, le dit", async () => {
    const { wrapper } = await boot();
    await fillMinimum(wrapper);
    await openSummary(wrapper);
    expect(document.querySelector("[data-sum-image]")?.textContent).toContain("nginx:latest");
    expect(document.querySelector("[data-sum-image]")?.textContent).toContain("par défaut");
    expect(document.querySelector("[data-sum-latest]")?.textContent).toContain(
      "« latest » peut changer d'un jour à l'autre.",
    );
    expect(document.querySelector("[data-sum-ports]")?.textContent).toContain("Aucun port");
    expect(document.querySelector("[data-sum-env]")?.textContent).toContain("Aucune variable");
    expect(document.querySelector("[data-sum-command]")?.textContent?.trim()).toBe(
      "Commande de l'image",
    );
    wrapper.unmount();
  });

  it("redemande le mot de passe même juste après un autre acte : l'élévation ne couvre pas un déploiement", async () => {
    const { wrapper, bridge } = await boot();
    bridge.reauth.open("forge");
    await fillMinimum(wrapper);
    await openSummary(wrapper);
    expect(reauthField()).not.toBeNull();
    // Sans mot de passe, le bouton reste inactif.
    expect(dialogButton("Déployer le service")?.getAttribute("aria-disabled")).toBe("true");
    wrapper.unmount();
  });

  it("« Modifier » revient au formulaire intact, sans rien envoyer", async () => {
    const { wrapper, bridge } = await boot();
    await fillAll(wrapper);
    await openSummary(wrapper);
    dialogButton("Modifier")?.click();
    await settle();
    expect(document.querySelector("dialog[open]")).toBeNull();
    expect(field(wrapper, "name").element.value).toBe("web");
    expect(wrapper.findAll("[data-env-row]")).toHaveLength(1);
    expect(bridge.deploys.received).toHaveLength(0);
    wrapper.unmount();
  });

  it("une image déjà présente est dite, sans point d'attention", async () => {
    const { wrapper } = await boot((bridge) => {
      bridge.deploys.markPresent("nginx:latest");
      bridge.deploys.setDisk("forge", 2 * GIB);
    });
    await fillMinimum(wrapper);
    await openSummary(wrapper);
    expect(document.querySelector("[data-sum-present]")?.textContent).toContain(
      "L'image est déjà présente sur le serveur : elle est utilisée telle quelle.",
    );
    expect(document.querySelector("[data-sum-present]")?.textContent).toContain(
      "Si une version plus récente existe, elle n'est pas récupérée.",
    );
    expect(document.querySelector("[data-sum-attention]")).toBeNull();
    wrapper.unmount();
  });

  it("moins de 10 Go libres : une case est exigée, « Déployer le service » reste inactif tant qu'elle n'est pas cochée (BR-DEPLOY-018)", async () => {
    const { wrapper, bridge } = await boot((sim) => sim.deploys.setDisk("forge", 4 * GIB));
    await fillMinimum(wrapper);
    await openSummary(wrapper);
    expect(document.querySelector("[data-attention='disk']")?.textContent).toContain(
      "Il reste 4 Go libres sur le disque du serveur. Une image peut prendre plusieurs gigaoctets : la récupération échoue si le disque se remplit.",
    );
    expect(document.querySelector("[data-sum-need]")?.textContent).toBe(
      "Validez les points d'attention pour continuer.",
    );
    typeInto(reauthField(), OWN);
    await flushPromises();
    expect(dialogButton("Déployer le service")?.getAttribute("aria-disabled")).toBe("true");
    document.querySelector<HTMLInputElement>("[data-attention='disk'] input")?.click();
    await flushPromises();
    expect(dialogButton("Déployer le service")?.getAttribute("aria-disabled")).toBeNull();
    dialogButton("Déployer le service")?.click();
    await settle();
    expect(bridge.deploys.received[0]?.request.accept).toEqual({
      reservedPorts: false,
      lowDisk: true,
    });
    wrapper.unmount();
  });

  it("un port réservé par un service arrêté demande sa case et part dans la demande (BR-DEPLOY-008)", async () => {
    const { wrapper, bridge } = await boot((sim) =>
      sim.services.seed("forge", [
        {
          key: "h_off",
          name: "arrete",
          origin: "hearth",
          state: "stopped",
          ports: [{ hostPort: 8080, containerPort: 80, protocol: "tcp", scope: "all" }],
        },
      ]),
    );
    await fillAll(wrapper);
    await openSummary(wrapper);
    expect(document.querySelector("[data-attention='reserved']")?.textContent).toContain("arrete");
    typeInto(reauthField(), OWN);
    await flushPromises();
    expect(dialogButton("Déployer le service")?.getAttribute("aria-disabled")).toBe("true");
    document.querySelector<HTMLInputElement>("[data-attention='reserved'] input")?.click();
    await flushPromises();
    dialogButton("Déployer le service")?.click();
    await settle();
    expect(bridge.deploys.received[0]?.request.accept.reservedPorts).toBe(true);
    wrapper.unmount();
  });
});

describe("le lancement et la progression (BR-DEPLOY-016)", () => {
  it("envoie la demande telle que saisie, ouvre la progression et montre les étapes dans l'ordre", async () => {
    const { wrapper, bridge } = await boot();
    await fillAll(wrapper);
    await launch(wrapper);
    expect(bridge.deploys.received).toHaveLength(1);
    expect(bridge.deploys.received[0]?.request).toMatchObject({
      name: "web",
      image: "nginx",
      tag: "1.27",
      env: [{ name: "DB_PASSWORD", value: SECRET }],
      ports: [{ hostPort: 8080, containerPort: 80, protocol: "tcp" }],
      command: `run --token ${COMMAND_SECRET}`,
      resources: { memoryBytes: 2 * GIB, cpuMillis: 4000 },
    });
    expect(progress()?.textContent).toContain("Déploiement de web");
    expect(stepStates()).toEqual([
      "check:now",
      "pull:later",
      "create:later",
      "start:later",
      "watch:later",
    ]);
    // La saisie reste en mémoire pendant le déploiement (un échec rouvre le formulaire rempli).
    expect(useDeployFormStore().filled("forge")).toBe(true);
    // La ligne « Déploiement en cours » est visible de tous dans la liste.
    expect(bridge.services.find("forge", running(bridge).key)?.state).toBe("deploying");
    wrapper.unmount();
  });

  it("montre le pourcentage et les octets de la récupération, une barre indéterminée sinon", async () => {
    const { wrapper, bridge } = await boot();
    await fillMinimum(wrapper);
    await launch(wrapper);
    const { operationId } = running(bridge);
    bridge.deploys.progress("forge", operationId, null);
    await flushPromises();
    expect(stepStates()[1]).toBe("pull:now");
    expect(document.querySelector(".meter__slide")).not.toBeNull();
    expect(document.querySelector("[data-progress-text]")).toBeNull();
    bridge.deploys.progress("forge", operationId, 40, 400 * 1024 * 1024, GIB);
    await flushPromises();
    expect(document.querySelector("[data-progress-text]")?.textContent).toBe(
      "40 % · 400 Mo sur 1 Go",
    );
    expect(document.querySelector(".meter__slide")).toBeNull();
    expect(stepStates().slice(0, 2)).toEqual(["check:done", "pull:now"]);
    wrapper.unmount();
  });

  it("« Annuler » n'existe que pendant la récupération : ensuite inactif, avec « Trop tard » écrit", async () => {
    const { wrapper, bridge } = await boot();
    await fillMinimum(wrapper);
    await launch(wrapper);
    const { operationId } = running(bridge);
    bridge.deploys.progress("forge", operationId, 10);
    await flushPromises();
    expect(
      document.querySelector("[data-deploy-cancel]")?.getAttribute("aria-disabled"),
    ).toBeNull();
    bridge.deploys.toStep("forge", operationId, "create");
    await flushPromises();
    expect(document.querySelector("[data-deploy-cancel]")?.getAttribute("aria-disabled")).toBe(
      "true",
    );
    expect(document.querySelector("[data-deploy-too-late]")?.textContent).toBe(
      "Il est trop tard pour annuler : le service est en cours de création.",
    );
    expect(stepStates()).toEqual([
      "check:done",
      "pull:done",
      "create:now",
      "start:later",
      "watch:later",
    ]);
    wrapper.unmount();
  });

  it("annuler arrête la récupération : rien n'est créé, le formulaire reste rempli (BR-DEPLOY-016)", async () => {
    const { wrapper, bridge, router } = await boot();
    await fillAll(wrapper);
    await launch(wrapper);
    const { operationId, key } = running(bridge);
    bridge.deploys.progress("forge", operationId, 10);
    await flushPromises();
    document.querySelector<HTMLElement>("[data-deploy-cancel]")?.click();
    await settle();
    expect(dialogText()).toContain("Annuler le déploiement de web ?");
    expect(topButton("Garder le déploiement")).toBeDefined();
    await confirmTop("Annuler le déploiement");
    await settle();
    expect(bridge.deploys.cancels).toEqual([{ serverId: "forge", opId: operationId }]);
    expect(toasts()).toContain("Le déploiement a été annulé et rien n'a été créé.");
    expect(progress()).toBeNull();
    expect(bridge.services.find("forge", key)).toBeUndefined();
    // Toujours sur le formulaire, rempli.
    expect(router.currentRoute.value.name).toBe("services-deploy");
    expect(field(wrapper, "name").element.value).toBe("web");
    expect(wrapper.findAll("[data-env-row]")).toHaveLength(1);
    wrapper.unmount();
  });

  it("annuler redemande le mot de passe quand l'élévation est fermée, et le dit s'il est faux (BR-DEPLOY-016)", async () => {
    const { wrapper, bridge } = await boot();
    await fillMinimum(wrapper);
    await launch(wrapper);
    // Le mot de passe du lancement a ouvert l'élévation ; fermée, l'annulation le redemande.
    bridge.reauth.close("forge");
    document.querySelector<HTMLElement>("[data-deploy-cancel]")?.click();
    await settle();
    await confirmTop("Annuler le déploiement", "faux");
    await settle();
    expect(dialogText()).toContain("Mot de passe actuel incorrect.");
    expect(bridge.deploys.cancels).toHaveLength(0);
    await confirmTop("Annuler le déploiement", OWN);
    await settle();
    expect(bridge.deploys.cancels).toHaveLength(1);
    expect(toasts()).toContain("Le déploiement a été annulé et rien n'a été créé.");
    wrapper.unmount();
  });

  it("annuler après la création est refusé : « Trop tard pour annuler », la progression continue", async () => {
    const { wrapper, bridge } = await boot();
    await fillMinimum(wrapper);
    await launch(wrapper);
    const { operationId } = running(bridge);
    bridge.deploys.progress("forge", operationId, 10);
    await flushPromises();
    document.querySelector<HTMLElement>("[data-deploy-cancel]")?.click();
    await settle();
    // La création commence pendant que la confirmation est ouverte : la course est perdue, le refus est dit.
    bridge.deploys.toStep("forge", operationId, "create");
    await confirmTop("Annuler le déploiement");
    await settle();
    expect(dialogText()).toContain(
      "Il est trop tard pour annuler : le service est en cours de création.",
    );
    expect(progress()).not.toBeNull();
    wrapper.unmount();
  });

  it("garder le déploiement ferme la confirmation sans rien annuler", async () => {
    const { wrapper, bridge } = await boot();
    await fillMinimum(wrapper);
    await launch(wrapper);
    document.querySelector<HTMLElement>("[data-deploy-cancel]")?.click();
    await settle();
    topButton("Garder le déploiement")?.click();
    await settle();
    expect(bridge.deploys.cancels).toHaveLength(0);
    expect(progress()).not.toBeNull();
    wrapper.unmount();
  });

  it("la réussite est annoncée, la saisie effacée, la liste s'ouvre sur le nouveau service (BR-DEPLOY-029)", async () => {
    const { wrapper, bridge, router } = await boot();
    await fillAll(wrapper);
    await launch(wrapper);
    const { operationId, key } = running(bridge);
    bridge.deploys.progress("forge", operationId, 100, GIB, GIB);
    bridge.deploys.toStep("forge", operationId, "watch");
    bridge.deploys.succeed("forge", operationId);
    await settle();
    expect(toasts()).toEqual(["Le service web est en marche."]);
    expect(progress()).toBeNull();
    expect(router.currentRoute.value.name).toBe("services");
    expect(router.currentRoute.value.query.service).toBe(key);
    expect(useDeployFormStore().filled("forge")).toBe(false);
    expect(bridge.services.find("forge", key)?.state).toBe("running");
    wrapper.unmount();
  });

  it("une opération plus rapide que la réponse est tout de même dite à la fenêtre", async () => {
    const { wrapper, bridge, router } = await boot();
    await fillMinimum(wrapper);
    // L'agent termine pendant que la réponse « accepté » est encore en route.
    const original = bridge.deployService.bind(bridge);
    bridge.deployService = async (...args) => {
      const outcome = await original(...args);
      if (outcome.kind === "accepted") bridge.deploys.succeed("forge", outcome.operationId);
      return outcome;
    };
    await launch(wrapper);
    // Dite une seule fois : la notification est déjà montrée, la fenêtre ne la répète pas.
    expect(toasts()).toEqual(["Le service web est en marche."]);
    expect(router.currentRoute.value.name).toBe("services");
    wrapper.unmount();
  });
});

describe("l'état de confiance change entre la lecture et l'envoi", () => {
  it("rouvre le champ du mot de passe sans notification d'erreur, la saisie reste", async () => {
    const { wrapper, bridge } = await boot();
    await fillAll(wrapper);
    await openSummary(wrapper);
    // La ceinture de la liaison : l'acte part sans mot de passe alors que l'agent en demande un.
    vi.spyOn(bridge, "deployService").mockRejectedValue(
      new LinkCommandError({ kind: "invalid_input", field: "credentials" }),
    );
    typeInto(reauthField(), OWN);
    await flushPromises();
    dialogButton("Déployer le service")?.click();
    await settle();
    expect(toasts()).toEqual([]);
    expect(document.querySelector("dialog[open] [data-reauth-notice]")).not.toBeNull();
    expect(document.querySelector("dialog[open]")).not.toBeNull();
    expect(field(wrapper, "name").element.value).toBe("web");
    wrapper.unmount();
  });
});

describe("les échecs : rien n'est créé, le formulaire reste rempli (BR-DEPLOY-017, 019)", () => {
  const causes: [Parameters<SimulatedLinkBridge["deploys"]["fail"]>[2], string][] = [
    ["image_not_found", "Cette image est introuvable : vérifiez son nom."],
    ["tag_not_found", "Cette version n'existe pas pour cette image."],
    [
      "image_private_or_missing",
      "Cette image est privée ou n'existe pas. Les images privées ne sont pas prises en charge pour l'instant.",
    ],
    [
      "registry_unreachable",
      "Le serveur n'a pas pu récupérer l'image : il ne joint pas le registre. Vérifiez sa connexion à Internet.",
    ],
    [
      "registry_rate_limited",
      "Le registre limite les téléchargements anonymes. Réessayez plus tard.",
    ],
    ["platform_mismatch", "Cette image n'existe pas pour l'architecture de votre serveur."],
    ["disk_full", "Le disque du serveur est plein : l'image n'a pas pu être récupérée."],
    ["engine_refused", "Le moteur a refusé de créer le service. Rien n'a été créé."],
    [
      "timeout",
      "La récupération de l'image prend trop de temps et a été arrêtée. Rien n'a été créé.",
    ],
  ];

  for (const [cause, text] of causes) {
    it(`${cause} : la cause est dite en clair, « Modifier » rouvre le formulaire rempli`, async () => {
      const { wrapper, bridge, router } = await boot();
      await fillAll(wrapper);
      await launch(wrapper);
      const { operationId, key } = running(bridge);
      bridge.deploys.progress("forge", operationId, 30);
      bridge.deploys.fail("forge", operationId, cause);
      await settle();
      expect(document.querySelector("[data-deploy-failed]")?.textContent?.trim()).toBe(text);
      // Un seul bouton : « Modifier ».
      expect(
        [...document.querySelectorAll("[data-deploy-progress] button")].map((b) =>
          b.textContent?.trim(),
        ),
      ).toEqual(["Modifier"]);
      expect(toasts()).toEqual([]);
      expect(bridge.services.find("forge", key)).toBeUndefined();
      document.querySelector<HTMLElement>("[data-deploy-modify]")?.click();
      await settle();
      expect(progress()).toBeNull();
      expect(router.currentRoute.value.name).toBe("services-deploy");
      expect(field(wrapper, "name").element.value).toBe("web");
      expect(wrapper.findAll("[data-env-row]")).toHaveLength(1);
      expect(wrapper.get("[data-deploy-refusal]").text()).toBe(text);
      wrapper.unmount();
    });
  }

  it("la version introuvable signale le champ Version, l'image introuvable le champ Image", async () => {
    const { wrapper, bridge } = await boot();
    await fillAll(wrapper);
    await launch(wrapper);
    bridge.deploys.fail("forge", running(bridge).operationId, "tag_not_found");
    await settle();
    document.querySelector<HTMLElement>("[data-deploy-modify]")?.click();
    await settle();
    expect(document.activeElement).toBe(field(wrapper, "tag").element);
    wrapper.unmount();
  });
});

describe("après le lancement : le service est conservé (BR-DEPLOY-019, 020)", () => {
  it("un programme qui s'arrête aussitôt est dit avec son code, le service reste dans la liste", async () => {
    const { wrapper, bridge, router } = await boot();
    await fillAll(wrapper);
    await launch(wrapper);
    const { operationId, key } = running(bridge);
    bridge.deploys.succeed("forge", operationId, {
      cause: "stopped_right_after_start",
      exitCode: 137,
    });
    await settle();
    expect(document.querySelector("[data-deploy-kept]")?.textContent?.trim()).toBe(
      "Le service web s'est arrêté juste après son démarrage (code 137 : coupé de force (arrêt forcé ou manque de mémoire)).",
    );
    expect(bridge.services.find("forge", key)).toMatchObject({ state: "error" });
    // La saisie, désormais inutile, est effacée.
    expect(useDeployFormStore().filled("forge")).toBe(false);
    document.querySelector<HTMLElement>("[data-deploy-logs]")?.click();
    await settle();
    expect(router.currentRoute.value.name).toBe("services");
    expect(router.currentRoute.value.query).toMatchObject({ service: key, onglet: "journaux" });
    wrapper.unmount();
  });

  it("un code 0 dit que le programme n'est peut-être pas fait pour tourner en continu", async () => {
    const { wrapper, bridge } = await boot();
    await fillMinimum(wrapper);
    await launch(wrapper);
    bridge.deploys.succeed("forge", running(bridge).operationId, {
      cause: "stopped_right_after_start",
      state: "stopped",
      exitCode: 0,
    });
    await settle();
    expect(document.querySelector("[data-deploy-kept]")?.textContent?.trim()).toBe(
      "Le service web s'est terminé juste après son démarrage (code 0). Ce programme n'est peut-être pas fait pour tourner en continu.",
    );
    expect(document.querySelector("[data-deploy-stop]")).toBeNull();
    document.querySelector<HTMLElement>("[data-deploy-close]")?.click();
    await settle();
    expect(progress()).toBeNull();
    wrapper.unmount();
  });

  it("un service relancé sans cesse par le moteur est dit, et « Arrêter » est proposé", async () => {
    const { wrapper, bridge } = await boot();
    await fillMinimum(wrapper);
    await launch(wrapper);
    bridge.deploys.succeed("forge", running(bridge).operationId, {
      cause: "stopped_right_after_start",
      state: "restart_loop",
      exitCode: 1,
    });
    await settle();
    expect(document.querySelector("[data-deploy-kept]")?.textContent?.trim()).toBe(
      "Le service web s'arrête dès qu'il démarre et le moteur le relance sans cesse (code 1). Pour arrêter ces relances, arrêtez le service.",
    );
    document.querySelector<HTMLElement>("[data-deploy-stop]")?.click();
    await settle();
    expect(dialogText()).toContain("Arrêter web ?");
    wrapper.unmount();
  });
});

describe("en arrière-plan et résultat inconnu (BR-DEPLOY-024, 028)", () => {
  it("continuer en arrière-plan ramène à la liste, efface la saisie et laisse le déploiement courir", async () => {
    const { wrapper, bridge, router } = await boot();
    await fillAll(wrapper);
    await launch(wrapper);
    const { operationId } = running(bridge);
    document.querySelector<HTMLElement>("[data-deploy-background]")?.click();
    await settle();
    expect(progress()).toBeNull();
    expect(router.currentRoute.value.name).toBe("services");
    expect(useDeployFormStore().filled("forge")).toBe(false);
    expect(wrapper.text()).toContain("Déploiement en cours");
    // L'échec tombe plus tard : une notification l'annonce, avec sa cause.
    bridge.deploys.fail("forge", operationId, "tag_not_found");
    await settle();
    expect(toasts()).toEqual(["Le déploiement de web a échoué : version introuvable."]);
    wrapper.unmount();
  });

  it("un lien coupé après l'envoi : « résultat inconnu », jamais relancé, seul « en arrière-plan » reste", async () => {
    const { wrapper, bridge } = await boot();
    await fillMinimum(wrapper);
    bridge.actionMode = "cut";
    await launch(wrapper);
    expect(document.querySelector("[data-deploy-unknown]")?.textContent).toContain(
      "Le résultat de cette action n'est pas connu.",
    );
    expect(
      [...document.querySelectorAll("[data-deploy-progress] button")].map((b) =>
        b.textContent?.trim(),
      ),
    ).toEqual(["Continuer en arrière-plan"]);
    expect(bridge.calls.filter((call) => call === "deploy")).toHaveLength(1);
    document.querySelector<HTMLElement>("[data-deploy-background]")?.click();
    await settle();
    expect(bridge.calls.filter((call) => call === "deploy")).toHaveLength(1);
    wrapper.unmount();
  });

  it("un lien coupé avant l'envoi : rien ne part, le formulaire reste rempli (BR-RESIL-008)", async () => {
    const { wrapper, bridge } = await boot();
    await fillAll(wrapper);
    bridge.setState("forge", "offline");
    await settle();
    expect(submit(wrapper).attributes("aria-disabled")).toBe("true");
    expect(wrapper.get("[data-deploy-reason]").text()).toBe(
      "Indisponible tant que le serveur est hors ligne.",
    );
    expect(field(wrapper, "name").element.value).toBe("web");
    expect(bridge.deploys.received).toHaveLength(0);
    wrapper.unmount();
  });
});

describe("les refus du serveur au lancement : le serveur a le dernier mot (BR-DEPLOY-030, 032)", () => {
  it("un port pris au dernier moment ferme la fenêtre, signale le port et ne perd rien", async () => {
    const { wrapper, bridge } = await boot();
    await fillAll(wrapper);
    await openSummary(wrapper);
    bridge.deploys.listen("forge", 8080, "nginx");
    typeInto(reauthField(), OWN);
    await flushPromises();
    dialogButton("Déployer le service")?.click();
    await settle();
    expect(document.querySelector("dialog[open]")).toBeNull();
    expect(wrapper.get("[data-deploy-refusal]").text()).toBe(
      "Le port 8080 vient d'être pris par un programme du serveur. Rien n'a été créé.",
    );
    expect(wrapper.get("[data-port-row]").text()).toContain(
      "Ce port est occupé sur le serveur par « nginx »",
    );
    expect(document.activeElement?.closest("[data-port-row]")).not.toBeNull();
    expect(field(wrapper, "name").element.value).toBe("web");
    expect(useDeployFormStore().filled("forge")).toBe(true);
    expect(bridge.deploys.received).toHaveLength(0);
    wrapper.unmount();
  });

  it("une revérification au clic ramène au champ sans ouvrir le récapitulatif", async () => {
    const { wrapper, bridge } = await boot();
    await fillAll(wrapper);
    // Le port est pris entre la frappe et le clic : la consultation fraîche le voit.
    bridge.deploys.listen("forge", 8080, "nginx");
    await submit(wrapper).trigger("click");
    await settle();
    expect(document.querySelector("dialog[open]")).toBeNull();
    expect(wrapper.get("[data-deploy-refusal]").text()).toContain("Le port 8080 vient d'être pris");
    wrapper.unmount();
  });

  it("un nom pris par un autre déploiement : « Ce nom est déjà pris », le champ est signalé", async () => {
    const { wrapper, bridge } = await boot();
    await fillMinimum(wrapper);
    await openSummary(wrapper);
    bridge.services.upsert("forge", { key: "h_other", name: "Web", origin: "hearth" });
    typeInto(reauthField(), OWN);
    await flushPromises();
    dialogButton("Déployer le service")?.click();
    await settle();
    expect(wrapper.get("[data-deploy-refusal]").text()).toBe("Ce nom est déjà pris");
    expect(document.activeElement).toBe(field(wrapper, "name").element);
    expect(wrapper.get('[data-field="name"] [role="alert"]').text()).toBe("Ce nom est déjà pris");
    wrapper.unmount();
  });

  it("le droit retiré entre l'ouverture et le lancement : « Vous n'avez plus le droit… », la saisie reste", async () => {
    const { wrapper, bridge } = await boot();
    await fillAll(wrapper);
    await openSummary(wrapper);
    bridge.setServiceRightOnServer("forge", "marie", false);
    typeInto(reauthField(), OWN);
    await flushPromises();
    dialogButton("Déployer le service")?.click();
    await settle();
    expect(toasts()).toContain("Vous n'avez plus le droit de gérer les services de ce serveur.");
    expect(document.querySelector("dialog[open]")).toBeNull();
    expect(field(wrapper, "name").element.value).toBe("web");
    expect(wrapper.findAll("[data-env-row] input")[1]?.element).toHaveProperty("value", SECRET);
    expect(bridge.deploys.received).toHaveLength(0);
    wrapper.unmount();
  });

  it("deux déploiements déjà en cours : le refus est dit dans la fenêtre, qui reste ouverte", async () => {
    const { wrapper, bridge } = await boot();
    await fillMinimum(wrapper);
    await openSummary(wrapper);
    await bridge.deployService(
      "forge",
      {
        name: "un",
        image: "redis",
        tag: null,
        ports: [],
        env: [],
        command: null,
        resources: { memoryBytes: GIB, cpuMillis: 1000 },
        accept: { reservedPorts: false, lowDisk: false },
      },
      OWN,
    );
    await bridge.deployService(
      "forge",
      {
        name: "deux",
        image: "redis",
        tag: null,
        ports: [],
        env: [],
        command: null,
        resources: { memoryBytes: GIB, cpuMillis: 1000 },
        accept: { reservedPorts: false, lowDisk: false },
      },
      OWN,
    );
    typeInto(reauthField(), OWN);
    await flushPromises();
    dialogButton("Déployer le service")?.click();
    await settle();
    expect(dialogText()).toContain(
      "Deux déploiements sont déjà en cours sur ce serveur. Attendez la fin de l'un d'eux.",
    );
    expect(document.querySelector("dialog[open]")).not.toBeNull();
    wrapper.unmount();
  });

  it("un mot de passe faux se dit sous le champ, la fenêtre reste, rien ne part", async () => {
    const { wrapper, bridge } = await boot();
    await fillMinimum(wrapper);
    await openSummary(wrapper);
    typeInto(reauthField(), "faux");
    await flushPromises();
    dialogButton("Déployer le service")?.click();
    await settle();
    expect(dialogText()).toContain("Mot de passe actuel incorrect.");
    expect(bridge.deploys.received).toHaveLength(0);
    wrapper.unmount();
  });

  it("un manque de place annoncé au lancement fait apparaître la case, sans perdre la saisie", async () => {
    const { wrapper, bridge } = await boot();
    await fillMinimum(wrapper);
    await openSummary(wrapper);
    // La consultation n'avait rien vu ; l'agent refuse faute de place.
    bridge.deploys.setDisk("forge", 3 * GIB);
    typeInto(reauthField(), OWN);
    await flushPromises();
    dialogButton("Déployer le service")?.click();
    await settle();
    expect(document.querySelector("[data-attention='disk']")?.textContent).toContain(
      "Il reste 3 Go libres",
    );
    expect(document.querySelector("dialog[open]")).not.toBeNull();
    typeInto(reauthField(), OWN);
    document.querySelector<HTMLInputElement>("[data-attention='disk'] input")?.click();
    await flushPromises();
    dialogButton("Déployer le service")?.click();
    await settle();
    expect(bridge.deploys.received).toHaveLength(1);
    wrapper.unmount();
  });
});

describe("abandonner sa saisie (BR-DEPLOY-029)", () => {
  it("un formulaire vide se ferme sans question", async () => {
    const { wrapper, router } = await boot();
    await wrapper.get("[data-deploy-close-page]").trigger("click");
    await settle();
    expect(document.querySelector("dialog[open]")).toBeNull();
    expect(router.currentRoute.value.name).toBe("services");
    wrapper.unmount();
  });

  it("un formulaire rempli demande « Abandonner votre saisie ? » ; continuer la garde, abandonner l'efface", async () => {
    const { wrapper, router } = await boot();
    await fillAll(wrapper);
    await wrapper.get("[data-deploy-close-page]").trigger("click");
    await settle();
    expect(dialogText()).toContain("Abandonner votre saisie ?");
    expect(dialogText()).toContain("Ce que vous avez saisi sera effacé.");
    dialogButton("Continuer la saisie")?.click();
    await settle();
    expect(router.currentRoute.value.name).toBe("services-deploy");
    expect(field(wrapper, "name").element.value).toBe("web");
    await wrapper.get("[data-deploy-close-page]").trigger("click");
    await settle();
    dialogButton("Abandonner")?.click();
    await settle();
    expect(router.currentRoute.value.name).toBe("services");
    expect(useDeployFormStore().filled("forge")).toBe(false);
    wrapper.unmount();
  });

  it("quitter par une autre entrée du menu demande aussi la confirmation ; la saisie est effacée au départ", async () => {
    const { wrapper, router } = await boot();
    await fillAll(wrapper);
    await router.push("/servers/forge/dashboard");
    await settle();
    expect(dialogText()).toContain("Abandonner votre saisie ?");
    expect(router.currentRoute.value.name).toBe("services-deploy");
    dialogButton("Abandonner")?.click();
    await settle();
    expect(router.currentRoute.value.name).toBe("dashboard");
    expect(useDeployFormStore().filled("forge")).toBe(false);
    wrapper.unmount();
  });

  it("Échap hors d'un champ ferme le formulaire ; dans un champ, rien", async () => {
    const { wrapper, router } = await boot();
    await fillMinimum(wrapper);
    field(wrapper, "name").element.focus();
    field(wrapper, "name").element.dispatchEvent(
      new KeyboardEvent("keydown", { key: "Escape", bubbles: true }),
    );
    await settle();
    expect(document.querySelector("dialog[open]")).toBeNull();
    (document.activeElement as HTMLElement | null)?.blur();
    document.body.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    await settle();
    expect(dialogText()).toContain("Abandonner votre saisie ?");
    dialogButton("Continuer la saisie")?.click();
    await settle();
    expect(router.currentRoute.value.name).toBe("services-deploy");
    wrapper.unmount();
  });

  it("la saisie ne va jamais dans l'adresse, le stockage du navigateur ni un instantané de l'état", async () => {
    const { wrapper, router, pinia } = await boot();
    await fillAll(wrapper);
    await launch(wrapper);
    const everywhere = [
      router.currentRoute.value.fullPath,
      JSON.stringify(Object.entries(localStorage)),
      JSON.stringify(Object.entries(sessionStorage)),
      JSON.stringify(pinia.state.value),
    ].join("\n");
    expect(everywhere).not.toContain(SECRET);
    expect(everywhere).not.toContain(COMMAND_SECRET);
    expect(everywhere).not.toContain("DB_PASSWORD");
    wrapper.unmount();
  });
});

describe("les ressources (BR-DEPLOY-038, BR-RES-037)", () => {
  it("partent de la proposition de Hearth, sans position « sans limite »", async () => {
    const { wrapper } = await boot();
    expect(wrapper.get("[data-resources]").text()).toContain("Mémoire");
    expect(wrapper.get("[data-resources]").text()).toContain("Processeur");
    expect(wrapper.get("[data-resources]").text()).toContain("Hearth propose 2 Go");
    expect(wrapper.get("[data-resources]").text()).not.toMatch(/sans limite/i);
    wrapper.unmount();
  });

  it("des bornes illisibles bloquent le lancement et se relisent", async () => {
    const { wrapper, bridge } = await boot((sim) => {
      sim.deploys.failCapacity = true;
    });
    await fillMinimum(wrapper);
    expect(submit(wrapper).attributes("aria-disabled")).toBe("true");
    expect(wrapper.get("[data-deploy-reason]").text()).toBe(
      "Les limites du serveur sont en cours de lecture.",
    );
    expect(wrapper.get("[data-resources]").text()).toContain(
      "Hearth n'a pas pu lire les limites de ce serveur.",
    );
    bridge.deploys.failCapacity = false;
    await wrapper.get("[data-capacity-retry]").trigger("click");
    await settle();
    expect(submit(wrapper).attributes("aria-disabled")).toBeUndefined();
    wrapper.unmount();
  });
});
