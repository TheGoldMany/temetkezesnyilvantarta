import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import hu from "./hu.json";

// All UI strings live in hu.json; another language can be added as a new file.
void i18n.use(initReactI18next).init({
  resources: { hu: { translation: hu } },
  lng: "hu",
  fallbackLng: "hu",
  interpolation: { escapeValue: false },
  returnNull: false,
});

export default i18n;
