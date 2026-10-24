import { describe, expect, it } from "vitest";
import wire from "./fixtures/terraria-settings.json";
import { toSetting, WIRE_QUESTION_KEYS, WIRE_SETTING_KEYS } from "./simulated-terraria-settings";

// `toSetting` relit le JSON généré depuis la fiche : si l'agent sert un champ de plus (ou perd un champ attendu), ce
// test échoue au lieu de laisser le simulateur s'écarter en silence du vrai catalogue.

type Wire = Parameters<typeof toSetting>[0];
const items = (wire as unknown as { settings: Wire[] }).settings;
const REQUIRED_SETTING = ["question", "key", "theme", "level", "channel"];
const REQUIRED_QUESTION = ["id", "label", "help", "secret", "kind"];

describe("la fixture Terraria du simulateur suit le fil de l'agent", () => {
  it("ne porte aucune clé que toSetting ignore", () => {
    for (const item of items) {
      for (const name of Object.keys(item))
        expect(WIRE_SETTING_KEYS as readonly string[]).toContain(name);
      for (const name of Object.keys(item.question))
        expect(WIRE_QUESTION_KEYS as readonly string[]).toContain(name);
    }
  });

  it("porte les champs obligatoires de chaque réglage", () => {
    expect(items.length).toBeGreaterThan(0);
    for (const item of items) {
      for (const name of REQUIRED_SETTING) expect(item).toHaveProperty(name);
      for (const name of REQUIRED_QUESTION) expect(item.question).toHaveProperty(name);
    }
  });

  it("convertit sans perdre une valeur de la source", () => {
    expect(items.map(toSetting)).toHaveLength(items.length);
    for (const item of items) {
      const setting = toSetting(item);
      expect(setting.key).toBe(item.key);
      expect(setting.theme).toBe(item.theme);
      expect(setting.level).toBe(item.level);
      expect(setting.gameDefault).toBe(item.game_default ?? null);
      expect(setting.imposedWhy).toBe(item.imposed_why ?? null);
      expect(setting.question.id).toBe(item.question.id);
      expect(setting.question.proposed).toBe(item.question.proposed ?? null);
      expect(setting.question.min).toBe(item.question.min ?? null);
      expect(setting.question.maxLen).toBe(item.question.max_len ?? null);
      expect(setting.question.choices).toEqual(item.question.choices ?? []);
    }
  });
});
