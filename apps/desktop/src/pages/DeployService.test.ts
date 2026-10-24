import { flushPromises, type VueWrapper } from "@vue/test-utils";
import { describe, expect, it, vi } from "vitest";
import type { SimulatedLinkBridge } from "@/link";
import { useDeployFormStore } from "@/stores/deployForm";
import { useServiceRightsStore } from "@/stores/serviceRights";
import { useServicesStore } from "@/stores/services";
import { dialogButton, typeInto } from "@/test/confirm";
import {
  blur,
  boot,
  field,
  fillMinimum,
  openSummary,
  reason,
  settle,
  submit,
  type,
  useDeployTestClock,
} from "@/test/deploy";
import { mountContext } from "@/test/mount";

// Déployer une image depuis le client (HRT-69, BR-DEPLOY-001 à 009, 014 à 016, 020, 027 à 031, 033) : le formulaire,
// sa route et son bouton, sur le pont simulé dont l'agent juge comme le vrai. Le récapitulatif, la progression et
// l'abandon sont dans `DeployFlow.test.ts`.
useDeployTestClock();

const GIB = 1024 ** 3;

describe("la route (BR-DEPLOY-001, 027)", () => {
  it("s'ouvre pour un compte qui a le droit, moteur prêt, avec le focus sur « Nom du service »", async () => {
    const { wrapper, router } = await boot();
    expect(router.currentRoute.value.name).toBe("services-deploy");
    expect(wrapper.get("h1").text()).toBe("Déployer un service");
    expect(wrapper.find("[data-deploy-form]").exists()).toBe(true);
    expect(document.activeElement).toBe(field(wrapper, "name").element);
    // L'entrée « Services » reste marquée.
    expect(wrapper.get(".nav__item.router-link-active").text()).toBe("Services");
    wrapper.unmount();
  });

  it("renvoie vers la liste un compte en lecture seule", async () => {
    const { wrapper, router } = await boot(
      (bridge) => bridge.setServiceRightOnServer("salon", "paul", false),
      "/servers/salon/services/deployer",
      "salon",
    );
    expect(router.currentRoute.value.name).toBe("services");
    expect(wrapper.find("[data-deploy-form]").exists()).toBe(false);
    wrapper.unmount();
  });

  it("renvoie vers la liste un compte qui n'a pas le droit de gérer les services", async () => {
    const { wrapper, router } = await boot((bridge) =>
      bridge.setServiceRightOnServer("forge", "marie", false),
    );
    expect(router.currentRoute.value.name).toBe("services");
    wrapper.unmount();
  });

  it("renvoie vers la liste un moteur absent, non pris en charge ou d'une version non prise en charge", async () => {
    for (const state of ["absent", "unsupported_engine", "unsupported_version"] as const) {
      const { wrapper, router } = await boot((bridge) =>
        bridge.services.setEngine("forge", { state }),
      );
      expect(router.currentRoute.value.name, state).toBe("services");
      wrapper.unmount();
      document.body.innerHTML = "";
      useServicesStore().reset();
      useServiceRightsStore().reset();
    }
  });

  it("renvoie vers la liste un agent qui ne gère pas les services", async () => {
    const { wrapper, router } = await boot((bridge) =>
      bridge.services.setUnsupported("forge", "0.1.0"),
    );
    expect(router.currentRoute.value.name).toBe("services");
    wrapper.unmount();
  });

  it("ouvre le formulaire quand le moteur n'est pas prêt, l'explique et désactive le lancement, saisie conservée", async () => {
    const { wrapper } = await boot((bridge) =>
      bridge.services.setEngine("forge", { state: "stopped" }),
    );
    expect(wrapper.find("[data-deploy-form]").exists()).toBe(true);
    expect(wrapper.text()).toContain("Moteur arrêté");
    await fillMinimum(wrapper);
    expect(field(wrapper, "name").element.value).toBe("web");
    expect(submit(wrapper).attributes("aria-disabled")).toBe("true");
    expect(reason(wrapper)).toBe(
      "Le déploiement est indisponible : le moteur de conteneurs est arrêté.",
    );
    wrapper.unmount();
  });
});

describe("la garde et les bornes de la machine", () => {
  it("un droit illisible renvoie vers la liste au lieu de faire échouer la navigation", async () => {
    const ctx = await mountContext();
    ctx.bridge.services.seed("forge", [{ key: "h_db", name: "base", origin: "hearth" }]);
    vi.spyOn(useServiceRightsStore(), "load").mockRejectedValue(new Error("illisible"));
    await ctx.router.push("/servers/forge/services/deployer");
    await settle();
    expect(ctx.router.currentRoute.value.name).toBe("services");
  });

  it("changer de serveur ignore la réponse encore en vol de l'ancien (même nom, même clé de consultation)", async () => {
    const { wrapper, bridge, router } = await boot((sim) => {
      sim.services.seed("salon", [{ key: "h_x", name: "x", origin: "hearth" }]);
    });
    const real = bridge.checkDeployment.bind(bridge);
    let release: (value: Awaited<ReturnType<typeof real>>) => void = () => undefined;
    let first = true;
    vi.spyOn(bridge, "checkDeployment").mockImplementation((serverId, input) => {
      if (!first) return real(serverId, input);
      first = false;
      return new Promise((resolve) => {
        release = resolve;
      });
    });
    await fillMinimum(wrapper);
    await router.push("/servers/salon/services/deployer");
    await settle();
    dialogButton("Abandonner")?.click();
    await settle();
    expect(router.currentRoute.value.params.id).toBe("salon");
    await fillMinimum(wrapper);
    // La réponse tardive de « forge » dit « pris » : elle n'a rien à faire dans le formulaire de « salon ».
    release({ name: "taken", invalidFields: [], ports: [], disk: null, imagePresent: false });
    await settle();
    expect(wrapper.text()).not.toContain("Ce nom est déjà pris");
    expect(submit(wrapper).attributes("aria-disabled")).toBeUndefined();
    wrapper.unmount();
  });

  it("changer de serveur relit les bornes de la machine : celles de l'ancien ne restent pas", async () => {
    const { wrapper, bridge, router } = await boot((sim) => {
      sim.services.seed("salon", [{ key: "h_x", name: "x", origin: "hearth" }]);
      sim.deploys.setCapacity("salon", { cpus: 2, memoryBytes: 4 * GIB });
    });
    expect(bridge.calls.filter((call) => call === "engine capacity")).toHaveLength(1);
    await router.push("/servers/salon/services/deployer");
    await settle();
    expect(bridge.calls.filter((call) => call === "engine capacity")).toHaveLength(2);
    expect(wrapper.get("[data-resources]").text()).toContain("2 cœurs");
    wrapper.unmount();
  });
});

describe("le bouton de la section Services (BR-DEPLOY-001, BR-GRANT-014)", () => {
  async function section(setup?: (bridge: SimulatedLinkBridge) => void, server = "forge") {
    return boot(setup, `/servers/${server}/services`, server);
  }

  it("ouvre le formulaire pour un compte qui a le droit", async () => {
    const { wrapper, router } = await section();
    const button = wrapper.get("[data-deploy-open]");
    expect(button.attributes("aria-disabled")).toBeUndefined();
    await button.trigger("click");
    await settle();
    expect(router.currentRoute.value.name).toBe("services-deploy");
    wrapper.unmount();
  });

  it("reste visible et désactivé, avec la raison du droit, pour un compte sans le droit", async () => {
    const { wrapper } = await section((bridge) =>
      bridge.setServiceRightOnServer("forge", "marie", false),
    );
    const button = wrapper.get("[data-deploy-open]");
    expect(button.attributes("aria-disabled")).toBe("true");
    expect(wrapper.text()).toContain("Votre compte ne peut pas gérer les services de ce serveur.");
    wrapper.unmount();
  });

  it("cède sa place aux journaux d'un service, qui ont besoin de toute la hauteur", async () => {
    const { wrapper } = await section();
    expect(wrapper.find("[data-deploy-open]").exists()).toBe(true);
    await wrapper.get('[data-row][data-key="h_db"]').trigger("click");
    await flushPromises();
    expect(wrapper.find("[data-deploy-open]").exists()).toBe(true);
    await wrapper.get('[data-tab="logs"]').trigger("click");
    await settle();
    expect(wrapper.find("[data-deploy-open]").exists()).toBe(false);
    wrapper.unmount();
  });

  it("est absent quand le moteur est absent ou non pris en charge", async () => {
    for (const state of [
      "absent",
      "unsupported_engine",
      "unsupported_version",
      "disabled",
    ] as const) {
      const { wrapper } = await section((bridge) => bridge.services.setEngine("forge", { state }));
      expect(wrapper.find("[data-deploy-open]").exists(), state).toBe(false);
      wrapper.unmount();
      document.body.innerHTML = "";
      useServicesStore().reset();
      useServiceRightsStore().reset();
    }
  });

  it("est visible, désactivé et expliqué quand le moteur est arrêté", async () => {
    const { wrapper } = await section((bridge) =>
      bridge.services.setEngine("forge", { state: "stopped" }),
    );
    expect(wrapper.get("[data-deploy-open]").attributes("aria-disabled")).toBe("true");
    expect(wrapper.text()).toContain(
      "Le déploiement est indisponible : le moteur de conteneurs est arrêté.",
    );
    wrapper.unmount();
  });
});

describe("le formulaire (BR-DEPLOY-002 à 007, 014)", () => {
  it("vide : « Déployer » est inactif et dit quoi renseigner", async () => {
    const { wrapper } = await boot();
    expect(submit(wrapper).attributes("aria-disabled")).toBe("true");
    expect(reason(wrapper)).toBe("Renseignez le nom et l'image pour continuer.");
    expect(wrapper.find("[data-env-row]").exists()).toBe(false);
    expect(wrapper.find("[data-port-row]").exists()).toBe(false);
    wrapper.unmount();
  });

  it("s'active dès que le nom et l'image sont valides : un déploiement minimal, sans port ni dossier", async () => {
    const { wrapper } = await boot();
    await fillMinimum(wrapper);
    expect(submit(wrapper).attributes("aria-disabled")).toBeUndefined();
    expect(wrapper.find("[data-deploy-reason]").exists()).toBe(false);
    wrapper.unmount();
  });

  it("n'affiche une erreur de forme qu'une fois le champ quitté, jamais pendant la frappe", async () => {
    const { wrapper } = await boot();
    await type(wrapper, "name", "-");
    expect(wrapper.text()).not.toContain("Le nom doit commencer par une lettre ou un chiffre");
    await blur(wrapper, "name");
    expect(wrapper.get('[data-field="name"] [role="alert"]').text()).toBe(
      "Le nom doit commencer par une lettre ou un chiffre",
    );
    expect(field(wrapper, "name").attributes("aria-invalid")).toBe("true");
    await type(wrapper, "name", "a");
    expect(wrapper.get('[data-field="name"] [role="alert"]').text()).toBe(
      "Le nom doit faire au moins 2 caractères",
    );
    wrapper.unmount();
  });

  it("dit les erreurs de nom, d'image et de version avec les textes de la spec", async () => {
    const { wrapper } = await boot();
    const cases: [string, string, string][] = [
      ["name", "", "Le nom est requis"],
      ["name", "é", "Le nom ne peut contenir que lettres, chiffres, tirets et points"],
      ["name", "a".repeat(64), "Le nom ne peut pas dépasser 63 caractères"],
      ["image", "", "L'image est requise"],
      ["image", "Nginx", "Le nom de l'image n'est pas valide"],
      [
        "image",
        "nginx@sha256:abc",
        "Les identifiants d'empreinte (@sha256) ne sont pas pris en charge pour l'instant",
      ],
      [
        "tag",
        "bad tag",
        "La version n'est pas valide (lettres, chiffres, points, tirets et soulignés, 128 caractères au plus)",
      ],
    ];
    for (const [name, value, text] of cases) {
      await type(wrapper, name, value);
      await blur(wrapper, name);
      expect(wrapper.get(`[data-field="${name}"] [role="alert"]`).text(), `${name} ${value}`).toBe(
        text,
      );
    }
    wrapper.unmount();
  });

  it("déplace la version écrite dans l'image vers le champ Version et le dit (BR-DEPLOY-005)", async () => {
    const { wrapper } = await boot();
    await type(wrapper, "image", "nginx:1.27");
    await blur(wrapper, "image");
    expect(field(wrapper, "image").element.value).toBe("nginx");
    expect(field(wrapper, "tag").element.value).toBe("1.27");
    expect(wrapper.get("[data-version-moved]").text()).toBe(
      "La version a été déplacée dans le champ Version.",
    );
    wrapper.unmount();
  });

  it("dit que « latest » peut changer quand aucune version n'est saisie", async () => {
    const { wrapper } = await boot();
    expect(wrapper.find("[data-latest]").exists()).toBe(false);
    await type(wrapper, "image", "nginx");
    expect(wrapper.get("[data-latest]").text()).toContain(
      "« latest » peut changer d'un jour à l'autre.",
    );
    await type(wrapper, "tag", "1.27");
    expect(wrapper.find("[data-latest]").exists()).toBe(false);
    wrapper.unmount();
  });

  it("dit « Ce nom est déjà pris » sans dire par qui, sans tenir compte de la casse (BR-DEPLOY-004)", async () => {
    const { wrapper } = await boot((bridge) =>
      bridge.services.seed("forge", [{ key: "c_x", name: "Base", origin: "external" }]),
    );
    await type(wrapper, "name", "base");
    expect(wrapper.get('[data-field="name"] [role="alert"]').text()).toBe("Ce nom est déjà pris");
    expect(wrapper.text()).not.toContain("créé hors de Hearth");
    await type(wrapper, "image", "nginx");
    expect(reason(wrapper)).toBe("Corrigez les champs signalés pour continuer.");
    wrapper.unmount();
  });

  it("une variable : nom et valeur, secret masqué d'office, montrer et masquer à la main (BR-DEPLOY-006)", async () => {
    const { wrapper } = await boot();
    await wrapper.get("[data-env-add]").trigger("click");
    await flushPromises();
    const [name, value] = wrapper.findAll<HTMLInputElement>("[data-env-row] input");
    typeInto(name?.element ?? null, "DB_PASSWORD");
    typeInto(value?.element ?? null, "hunter2");
    await flushPromises();
    expect(value?.attributes("type")).toBe("password");
    await wrapper.get("[data-env-toggle]").trigger("click");
    expect(value?.attributes("type")).toBe("text");
    await wrapper.get("[data-env-toggle]").trigger("click");
    expect(value?.attributes("type")).toBe("password");
    // Un nom ordinaire est en clair, masquable à la main.
    typeInto(name?.element ?? null, "PORT");
    await flushPromises();
    expect(value?.attributes("type")).toBe("password");
    await wrapper.get("[data-env-toggle]").trigger("click");
    expect(value?.attributes("type")).toBe("text");
    expect(wrapper.get("[data-env-visible]").text()).toContain(
      "visibles par les comptes qui peuvent gérer les services de ce serveur",
    );
    wrapper.unmount();
  });

  it("dit les défauts d'une variable : nom en double, nom mal formé, compteur et retrait", async () => {
    const { wrapper } = await boot();
    for (let i = 0; i < 2; i += 1) await wrapper.get("[data-env-add]").trigger("click");
    await flushPromises();
    const inputs = () => wrapper.findAll<HTMLInputElement>("[data-env-row] input");
    typeInto(inputs()[0]?.element ?? null, "A");
    typeInto(inputs()[2]?.element ?? null, "A");
    await flushPromises();
    await inputs()[2]?.trigger("blur");
    await flushPromises();
    expect(wrapper.get("[data-env-section]").text()).toContain("Cette variable est déjà définie");
    typeInto(inputs()[2]?.element ?? null, "1x");
    await inputs()[2]?.trigger("blur");
    await flushPromises();
    expect(wrapper.get("[data-env-section]").text()).toContain(
      "Le nom de la variable ne peut contenir que lettres, chiffres et soulignés, et ne peut pas commencer par un chiffre",
    );
    expect(wrapper.get("[data-env-count]").text()).toBe("2 sur 100");
    await wrapper.findAll("[data-env-remove]")[1]?.trigger("click");
    await flushPromises();
    expect(wrapper.get("[data-env-count]").text()).toBe("1 sur 100");
    wrapper.unmount();
  });

  it("limite à 100 variables et à 50 ports : le bouton d'ajout reste visible, inactif, « La limite est atteinte. »", async () => {
    const { wrapper } = await boot();
    const store = useDeployFormStore();
    const draft = store.of("forge");
    for (let i = 0; i < 100; i += 1)
      draft.env.push({ id: store.rowId(), name: `V${i}`, value: "", reveal: null });
    for (let i = 0; i < 50; i += 1) {
      draft.ports.push({
        id: store.rowId(),
        host: String(2000 + i),
        container: "80",
        protocol: "tcp",
      });
    }
    await flushPromises();
    expect(wrapper.get("[data-env-add]").attributes("aria-disabled")).toBe("true");
    expect(wrapper.get("[data-env-limit]").text()).toBe("La limite est atteinte.");
    expect(wrapper.get("[data-port-add]").attributes("aria-disabled")).toBe("true");
    expect(wrapper.get("[data-ports-limit]").text()).toBe("La limite est atteinte.");
    wrapper.unmount();
  });

  it("une commande : aide sur les secrets, guillemets fermés, 4 096 caractères (BR-DEPLOY-014)", async () => {
    const { wrapper } = await boot();
    expect(wrapper.get("[data-command-secret]").text()).toBe(
      "Ne mettez pas de secret dans la commande : elle est visible de tous les comptes. Utilisez les variables.",
    );
    const command = wrapper.get<HTMLTextAreaElement>('[data-field="command"] textarea');
    typeInto(command.element, 'run "oups');
    await command.trigger("blur");
    await flushPromises();
    expect(wrapper.get('[data-field="command"] [role="alert"]').text()).toBe(
      "Les guillemets de la commande ne sont pas fermés",
    );
    typeInto(command.element, "x".repeat(4097));
    await flushPromises();
    expect(wrapper.get('[data-field="command"] [role="alert"]').text()).toBe(
      "La commande est trop longue (4 096 caractères au plus)",
    );
    typeInto(command.element, 'run "ok"');
    await flushPromises();
    expect(wrapper.find('[data-field="command"] [role="alert"]').exists()).toBe(false);
    wrapper.unmount();
  });
});

describe("les ports (BR-DEPLOY-007 à 009, 031)", () => {
  async function addPort(wrapper: VueWrapper, host: string, container = "80") {
    await wrapper.get("[data-port-add]").trigger("click");
    await flushPromises();
    const rows = wrapper.findAll("[data-port-row]");
    const row = rows[rows.length - 1];
    const [h, c] = row?.findAll<HTMLInputElement>("input") ?? [];
    typeInto(h?.element ?? null, host);
    typeInto(c?.element ?? null, container);
    await settle();
    return row;
  }

  it("dit que tout appareil qui peut joindre le serveur accède au port, dès qu'une rangée existe", async () => {
    const { wrapper } = await boot();
    expect(wrapper.find("[data-ports-exposed]").exists()).toBe(false);
    await addPort(wrapper, "8080");
    expect(wrapper.get("[data-ports-exposed]").text()).toBe(
      "Ce port sera accessible à tout appareil qui peut joindre votre serveur.",
    );
    wrapper.unmount();
  });

  it("dit les erreurs de forme : entier, bornes, doublon dans le formulaire", async () => {
    const { wrapper } = await boot();
    const first = await addPort(wrapper, "80a");
    await first?.findAll("input")[0]?.trigger("blur");
    await flushPromises();
    expect(first?.text()).toContain("Le port doit être un nombre entier");
    typeInto(first?.findAll<HTMLInputElement>("input")[0]?.element ?? null, "70000");
    await flushPromises();
    expect(first?.text()).toContain("Le port doit être compris entre 1 et 65535");
    typeInto(first?.findAll<HTMLInputElement>("input")[0]?.element ?? null, "8080");
    await settle();
    const second = await addPort(wrapper, "8080", "81");
    await second?.findAll("input")[0]?.trigger("blur");
    await flushPromises();
    expect(second?.text()).toContain("Ce port est déjà utilisé plus haut dans le formulaire");
    wrapper.unmount();
  });

  it("avertit sous 1024 sans bloquer (BR-DEPLOY-007)", async () => {
    const { wrapper } = await boot();
    await fillMinimum(wrapper);
    const row = await addPort(wrapper, "80");
    expect(row?.get("[data-port-privileged]").text()).toBe(
      "Les ports sous 1024 sont réservés aux programmes du système : vérifiez que c'est voulu.",
    );
    expect(submit(wrapper).attributes("aria-disabled")).toBeUndefined();
    wrapper.unmount();
  });

  it("un port tenu par un service bloque, nomme le service et propose le plus petit port libre en un bouton (BR-DEPLOY-008, 009)", async () => {
    const { wrapper } = await boot((bridge) =>
      bridge.services.seed("forge", [
        {
          key: "h_up",
          name: "up",
          origin: "hearth",
          ports: [{ hostPort: 8080, containerPort: 80, protocol: "tcp", scope: "all" }],
        },
      ]),
    );
    await fillMinimum(wrapper);
    const row = await addPort(wrapper, "8080");
    expect(row?.text()).toContain("Ce port est pris par le service « up »");
    expect(row?.get("[data-port-suggested]").text()).toContain(
      "Le port libre le plus proche est le 8081",
    );
    expect(submit(wrapper).attributes("aria-disabled")).toBe("true");
    await row?.get("[data-port-use]").trigger("click");
    await settle();
    expect(row?.findAll<HTMLInputElement>("input")[0]?.element.value).toBe("8081");
    expect(row?.text()).not.toContain("Ce port est pris par le service");
    expect(submit(wrapper).attributes("aria-disabled")).toBeUndefined();
    wrapper.unmount();
  });

  it("un conteneur hors Hearth n'est jamais nommé ; le port de Hearth lui-même est refusé", async () => {
    const { wrapper } = await boot((bridge) => {
      bridge.services.seed("forge", [
        {
          key: "c_secret",
          name: "conteneur-secret",
          origin: "external",
          ports: [{ hostPort: 9000, containerPort: 80, protocol: "tcp", scope: "all" }],
        },
      ]);
    });
    const row = await addPort(wrapper, "9000");
    expect(row?.text()).toContain("Ce port est tenu par un conteneur créé hors de Hearth.");
    expect(wrapper.text()).not.toContain("conteneur-secret");
    const agent = await addPort(wrapper, "7443");
    expect(agent?.text()).toContain("Ce port est utilisé par Hearth lui-même.");
    wrapper.unmount();
  });

  it("un port occupé par un programme du serveur le nomme quand l'agent le sait", async () => {
    const { wrapper } = await boot((bridge) => bridge.deploys.listen("forge", 5432, "postgres"));
    const row = await addPort(wrapper, "5432");
    expect(row?.text()).toContain("Ce port est occupé sur le serveur par « postgres »");
    wrapper.unmount();
  });

  it("un port réservé par un service arrêté avertit et demande la case du récapitulatif", async () => {
    const { wrapper } = await boot((bridge) =>
      bridge.services.seed("forge", [
        {
          key: "h_off",
          name: "arrete",
          origin: "hearth",
          state: "stopped",
          ports: [{ hostPort: 8080, containerPort: 80, protocol: "tcp", scope: "all" }],
        },
      ]),
    );
    await fillMinimum(wrapper);
    const row = await addPort(wrapper, "8080");
    expect(row?.get("[data-port-warning]").text()).toContain(
      "réservé par le service arrêté « arrete »",
    );
    expect(row?.text()).toContain("Vous devrez le confirmer au lancement");
    expect(submit(wrapper).attributes("aria-disabled")).toBeUndefined();
    await openSummary(wrapper);
    expect(document.querySelector("[data-attention='reserved']")?.textContent).toContain("arrete");
    wrapper.unmount();
  });
});
