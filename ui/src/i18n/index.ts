// English first, but every user-facing string goes through i18n from day
// one (docs/SPEC.md rules). Add a new locale by adding a `resources` entry
// below.
import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import common from "./locales/en/common.json";

export const defaultNS = "common";

void i18n.use(initReactI18next).init({
  lng: "en",
  fallbackLng: "en",
  defaultNS,
  resources: {
    en: { common },
  },
  interpolation: {
    escapeValue: false,
  },
});

export default i18n;
