import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";
import type { EnvRow, PortRow } from "@/deploy/draft";
import type { PortVerdict } from "@/deploy/messages";
import EnvVarRow from "./EnvVarRow.vue";
import PortMappingRow from "./PortMappingRow.vue";
import ProgressSteps from "./ProgressSteps.vue";

// Les rangées du formulaire de déploiement et la liste d'étapes (HRT-69). Elles ne jugent rien : les défauts
// arrivent en phrases, les constats du serveur dans `verdict`.
const env = (patch: Partial<EnvRow> = {}): EnvRow => ({
  id: 1,
  name: "DB_PASSWORD",
  value: "hunter2",
  reveal: null,
  ...patch,
});

describe("EnvVarRow", () => {
  it("masque la valeur d'un nom qui évoque un secret, la montre sinon, et l'utilisateur décide", async () => {
    const secret = mount(EnvVarRow, { props: { row: env() } });
    const value = () => secret.findAll("input")[1];
    expect(value()?.attributes("type")).toBe("password");
    expect(secret.get("[data-env-toggle]").attributes("aria-label")).toBe("Afficher la valeur");
    expect(secret.get("[data-env-toggle]").attributes("aria-pressed")).toBe("false");
    await secret.get("[data-env-toggle]").trigger("click");
    expect(secret.emitted("toggle")).toHaveLength(1);
    await secret.setProps({ row: env({ reveal: true }) });
    expect(value()?.attributes("type")).toBe("text");
    expect(secret.get("[data-env-toggle]").attributes("aria-label")).toBe("Masquer la valeur");
    const plain = mount(EnvVarRow, { props: { row: env({ name: "PORT", value: "80" }) } });
    expect(plain.findAll("input")[1]?.attributes("type")).toBe("text");
  });

  it("n'écrit les libellés que sur la première rangée et nomme le bouton de retrait", () => {
    const first = mount(EnvVarRow, { props: { row: env(), first: true } });
    expect(first.findAll("label.sr-only")).toHaveLength(0);
    const next = mount(EnvVarRow, { props: { row: env({ id: 2, name: "TZ" }) } });
    expect(next.findAll("label.sr-only")).toHaveLength(2);
    expect(next.get("[data-env-remove]").attributes("aria-label")).toBe("Retirer la variable TZ");
    const bare = mount(EnvVarRow, { props: { row: env({ name: "" }) } });
    expect(bare.get("[data-env-remove]").attributes("aria-label")).toBe("Retirer cette variable");
  });

  it("émet nom, valeur, retrait et sortie de champ ; montre les défauts qu'on lui donne", async () => {
    const wrapper = mount(EnvVarRow, {
      props: {
        row: env(),
        nameError: "Cette variable est déjà définie",
        valueError: "La valeur doit tenir sur une seule ligne",
      },
    });
    expect(wrapper.findAll('[role="alert"]').map((alert) => alert.text())).toEqual([
      "Cette variable est déjà définie",
      "La valeur doit tenir sur une seule ligne",
    ]);
    const [name, value] = wrapper.findAll("input");
    await name?.setValue("A");
    await value?.setValue("B");
    await name?.trigger("blur");
    await value?.trigger("blur");
    await wrapper.get("[data-env-remove]").trigger("click");
    expect(wrapper.emitted("update:name")).toEqual([["A"]]);
    expect(wrapper.emitted("update:value")).toEqual([["B"]]);
    expect(wrapper.emitted("blur-name")).toHaveLength(1);
    expect(wrapper.emitted("blur-value")).toHaveLength(1);
    expect(wrapper.emitted("remove")).toHaveLength(1);
  });
});

const port = (patch: Partial<PortRow> = {}): PortRow => ({
  id: 1,
  host: "8080",
  container: "80",
  protocol: "tcp",
  ...patch,
});
const quiet: PortVerdict = { error: null, warning: null, reserved: false, suggested: null };

describe("PortMappingRow", () => {
  const mountRow = (props: Record<string, unknown> = {}) =>
    mount(PortMappingRow, {
      props: {
        row: port(),
        verdict: quiet,
        privileged: false,
        waiting: false,
        position: 1,
        ...props,
      },
    });

  it("ne montre rien sous une rangée sans défaut", () => {
    const wrapper = mountRow();
    expect(wrapper.find('[role="alert"]').exists()).toBe(false);
    expect(wrapper.find("[data-port-warning]").exists()).toBe(false);
    expect(wrapper.find("[data-port-suggested]").exists()).toBe(false);
  });

  it("dit l'erreur du serveur et propose le port libre en un bouton", async () => {
    const wrapper = mountRow({
      verdict: { ...quiet, error: "Ce port est pris par le service « web »", suggested: 8081 },
    });
    expect(wrapper.get('[role="alert"]').text()).toBe("Ce port est pris par le service « web »");
    expect(wrapper.get("[data-port-suggested]").text()).toContain(
      "Port libre le plus proche : 8081",
    );
    await wrapper.get("[data-port-use]").trigger("click");
    expect(wrapper.emitted("suggest")).toEqual([[8081]]);
    expect(wrapper.get("[data-port-use]").text()).toBe("Utiliser le port 8081");
  });

  it("l'erreur de forme passe avant le constat du serveur ; l'avertissement n'apparaît que sans erreur", () => {
    const wrapper = mountRow({
      hostError: "Le port doit être un nombre entier",
      verdict: { ...quiet, error: "Ce port est occupé sur le serveur" },
      privileged: true,
    });
    expect(wrapper.findAll('[role="alert"]').map((alert) => alert.text())).toEqual([
      "Le port doit être un nombre entier",
    ]);
    expect(wrapper.find("[data-port-privileged]").exists()).toBe(false);
    const warned = mountRow({ privileged: true });
    expect(warned.get("[data-port-privileged]").text()).toContain("Les ports sous 1024");
  });

  it("un port réservé par un service arrêté avertit et demande confirmation au lancement", () => {
    const wrapper = mountRow({
      verdict: {
        ...quiet,
        warning: "Ce port est réservé par le service arrêté « base ».",
        reserved: true,
      },
    });
    expect(wrapper.get("[data-port-warning]").text()).toContain("réservé par le service arrêté");
    expect(wrapper.get("[data-port-warning]").text()).toContain("À confirmer au lancement");
  });

  it("montre l'attente de la réponse du serveur et nomme le bouton de retrait par son rang", async () => {
    const wrapper = mountRow({ waiting: true, position: 3 });
    expect(wrapper.find("[data-port-waiting]").exists()).toBe(true);
    expect(wrapper.get("[data-port-remove]").attributes("aria-label")).toBe(
      "Retirer la correspondance de ports 3",
    );
    await wrapper.get("[data-port-remove]").trigger("click");
    expect(wrapper.emitted("remove")).toHaveLength(1);
  });

  it("émet les trois champs et le protocole", async () => {
    const wrapper = mountRow({ first: true });
    const [host, container] = wrapper.findAll("input");
    await host?.setValue("9090");
    await container?.setValue("90");
    await wrapper.get("select").setValue("udp");
    expect(wrapper.emitted("update:host")).toEqual([["9090"]]);
    expect(wrapper.emitted("update:container")).toEqual([["90"]]);
    expect(wrapper.emitted("update:protocol")).toEqual([["udp"]]);
    expect(wrapper.findAll("label.sr-only")).toHaveLength(0);
  });
});

describe("ProgressSteps", () => {
  const steps = [
    { key: "check", label: "Vérification", state: "done" as const },
    {
      key: "pull",
      label: "Récupération de l'image",
      state: "now" as const,
      bar: { ratio: 0.4, text: "40 % · 400 Mo sur 1 Go" },
    },
    { key: "create", label: "Création du service", state: "later" as const },
  ];

  it("dit l'état de chaque étape en texte, marque l'étape en cours et montre la barre de celle qui progresse", () => {
    const wrapper = mount(ProgressSteps, { props: { steps, label: "Étapes du déploiement" } });
    expect(wrapper.get("ol").attributes("aria-label")).toBe("Étapes du déploiement");
    expect(wrapper.findAll("li").map((item) => item.text())).toEqual([
      "Vérification , faite",
      "Récupération de l'image , en cours40 % · 400 Mo sur 1 Go",
      "Création du service , à venir",
    ]);
    expect(wrapper.get('[data-step="pull"]').attributes("aria-current")).toBe("step");
    expect(wrapper.findAll('[aria-current="step"]')).toHaveLength(1);
    expect(wrapper.get("[data-progress-text]").text()).toBe("40 % · 400 Mo sur 1 Go");
  });

  it("borne la jauge de 0 à 100 % et passe en indéterminé quand la progression est inconnue", () => {
    const over = mount(ProgressSteps, {
      props: {
        steps: [{ key: "pull", label: "Pull", state: "now", bar: { ratio: 3, text: null } }],
        label: "x",
      },
    });
    expect(over.get("rect.meter__value").attributes("width")).toBe("100");
    const unknown = mount(ProgressSteps, {
      props: {
        steps: [{ key: "pull", label: "Pull", state: "now", bar: { ratio: null, text: null } }],
        label: "x",
      },
    });
    expect(unknown.find(".meter__slide").exists()).toBe(true);
    expect(unknown.find("[data-progress-text]").exists()).toBe(false);
  });
});
