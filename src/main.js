import { mount } from 'svelte'
import './app.css'
import App from './App.svelte'
import { initLang } from './lib/i18n.js'

// The language is resolved BEFORE the mount, not inside it: the first frame is already in this
// PC's language, so nothing is ever read in English and then swapped underneath the reader.
// `.then` rather than a top-level await — that one needs a build target this project does not set.
initLang().then(() =>
  mount(App, {
    target: document.getElementById('app'),
  })
)
