# KiBoard — host de Windows

[English](README.md) · **Español**

El lado del PC. Una aplicación Tauri 2 + Rust que corre en la bandeja, **se anuncia sola en la red
local** por mDNS, detecta la app en primer plano, ejecuta las acciones y sirve los layouts a los
teléfonos emparejados.

Contiene además el **editor visual de mazos**: un Stream Deck dibujado que el usuario configura
arrastrando apps y acciones sobre las teclas.

## Qué vive aquí

- **Servicio** — anuncio mDNS, servidor WebSocket, emparejamiento por código de 6 dígitos con un
  token revocable por dispositivo.
- **Detección** — app en primer plano, shell detectado por árbol de procesos, UI Automation.
- **Ejecución** — el DSL de macros (pasos encadenados, atajos, texto literal, pulsaciones UIA,
  OBS, volumen, ratón), capturas, y lanzar o enfocar apps instaladas.
- **Catálogo de apps** — enumera apps Win32 y UWP vía `shell:AppsFolder`, con sus íconos reales.
- **Editor** — UI en Svelte 5 con arrastrar y soltar, inspector de tecla y vista previa en vivo
  hacia el teléfono.

## Repositorios relacionados

| Repositorio | Qué es | Visibilidad |
|---|---|---|
| [`KiBoard-protocol`](https://github.com/maxia-cl/KiBoard-protocol) | Contrato de mensajes, tokens visuales, fixtures, documentación | Privado |
| [`KiBoard-app`](https://github.com/maxia-cl/KiBoard-app) | App móvil | Privado |
| [`KiBoard-windows-host-releases`](https://github.com/maxia-cl/KiBoard-windows-host-releases) | Instaladores y feed de actualización | Público |

`KiBoard-protocol` es la **fuente de verdad**. Primero se cambia ahí, después aquí.
Las compilaciones se publican en `KiBoard-windows-host-releases`, nunca aquí: este repo se queda
privado y el updater necesita un feed público.

## Estado

**FP hasta F6 listas, F7 a medias.** `KiBoard-protocol` está fijado como submódulo de git en
`KiBoard-protocol/`, tag `v0.3.0-f7`; `npm run dev`/`build` regeneran `src/tokens.g.css` desde ahí
automáticamente.

- **F0/F1** — el host de v1 se portó desde `ricardomendezv/Kiboard` con un `git subtree split`
  (historial completo) y se modularizó en `config.rs`, `net/`, `engine/`, `platform/`,
  `integrations/obs.rs` sin cambiar el comportamiento. Después, mDNS real (`net/discovery.rs`) y
  emparejamiento v2 por código de 6 dígitos, con tokens por dispositivo revocables uno a uno. Un
  `hello` con forma de v1 se rechaza con `protocol_too_old`.
- **F2/F3** — el modelo Deck/Page/Key, la repaginación sobre la grilla que declare cada cliente, y
  la regla de §4.2: el teléfono manda una POSICIÓN, nunca una acción. Las sesiones persisten; el
  host traduce es/en/zh.
- **F4** — el catálogo de apps vía `Get-StartApps`, un deck **Launcher** generado, y `launch:`,
  `focus:` y `kill:`. Las apps empaquetadas necesitan una segunda identidad (el AppUserModel ID de
  la ventana), porque todas sus ventanas pertenecen a `ApplicationFrameHost.exe`.
- **F5** — el editor sobre datos reales: `get_decks`/`save_decks`/`app_catalogue` devuelven el mismo
  struct `Deck` que viaja por el cable, y los decks sin guardar llegan al teléfono en vivo detrás de
  un único accessor, usado para dibujar *y* para resolver la pulsación.
- **F6** — paridad con Elgato, y más allá: teclas de dos estados resueltas en el host, cadenas de
  acciones, imágenes propias, compartir por archivo. En las teclas `obs:` **manda OBS**: una tecla
  de escena se enciende solo mientras está al aire, y el deck se repinta cuando OBS cambia sin que
  nadie pulse nada.
- **F7 hasta ahora** — el panel de emparejamiento y `pairing_status` muestran `ip:port`, para poder
  decirle a un teléfono qué escribir en una red que no deja pasar multicast. Y el transporte ahora
  es **`wss://`** (§2.2): un certificado autofirmado por instalación en `cert.der`/`key.der` junto a
  `config.json`, que el cliente fija. **No hay respaldo en texto plano y es un cambio que rompe
  compatibilidad**: host y teléfono hay que recompilarlos e instalarlos juntos.

Sigue abierto en F7: el onboarding de primer uso, el updater/firma/fichas de tienda, telemetría del
embudo de emparejamiento, y la mitad QR del respaldo de dirección manual (el teléfono todavía no
tiene escáner).

41 tests, clippy limpio.

## Publicación y actualizaciones

Las compilaciones se publican en
[`KiBoard-windows-host-releases`](https://github.com/maxia-cl/KiBoard-windows-host-releases), que es
público porque el updater necesita un feed que pueda leer sin token. El endpoint del updater en
`src-tauri/tauri.conf.json` apunta ahí.

**Dos cosas siguen siendo de v1 y hay que reemplazarlas antes del primer release de v2:**

1. **La clave de firma.** `plugins.updater.pubkey` sigue siendo la clave con la que firma KiBoard
   v1. Un release de v2 firmado con una clave de v2 sería rechazado por ella. Hay que generar una,
   guardar la mitad privada fuera de este repo, y pegar la pública en `tauri.conf.json`:

   ```bash
   npx tauri signer generate -w kiboard2-updater.key
   ```

   La clave privada y su contraseña pasan a ser los secretos `TAURI_SIGNING_PRIVATE_KEY` y
   `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` de lo que construya el release. Perderlas significa que
   ningún host instalado podrá volver a actualizarse nunca — no hay recuperación, solo reinstalar a
   mano.

2. **La versión.** `0.1.29` es donde quedó la numeración de v1, heredada por el port con `git
   subtree`. v2 debería empezar por su propio número, y el updater compara versiones, así que hay
   que decidirlo antes de publicar nada.

Hasta que las dos estén hechas el updater simplemente no encuentra nada, que es el fallo seguro. El
endpoint apuntaba al feed de v1 — ese **no** era seguro: la firma coincidía, así que un host de v2
podía haberse instalado v1 encima.

## Stack

Tauri 2 + Rust, UI del editor en Svelte 5 + Vite. Windows primero; el código específico de
plataforma se aísla en `platform/` desde el día uno, para que macOS y Linux puedan venir después,
cada uno en su propio repo, sin descoserlo.

## Convenciones

Todo el código va en inglés — identificadores, comentarios, commits y logs. Las cadenas que ve el
usuario pasan por `i18n`, nunca van en línea. Los documentos van en inglés y español
(`NOMBRE.md` / `NOMBRE.es.md`). Ver
[`CONTRIBUTING.es.md`](https://github.com/maxia-cl/KiBoard-protocol/blob/main/CONTRIBUTING.es.md).
