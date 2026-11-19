import i18n from 'i18next'
import { initReactI18next } from 'react-i18next'
import en from './en.json'

// fr.json and ar.json will be added post-MVP; `dir` handling below already supports RTL.
export const resources = {
  en: { translation: en },
} as const

function applyDirection(lng: string) {
  document.documentElement.lang = lng
  document.documentElement.dir = i18n.dir(lng)
}

i18n.on('languageChanged', applyDirection)

void i18n.use(initReactI18next).init({
  resources,
  lng: 'en',
  fallbackLng: 'en',
  interpolation: { escapeValue: false }, // React already escapes.
})

export default i18n
