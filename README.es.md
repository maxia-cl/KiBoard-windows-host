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
| [`KiBoard-protocol`](https://github.com/maxia-cl/KiBoard-protocol) | Contrato de mensajes, tokens visuales, fixtures, documentación | Público |
| [`KiBoard-app`](https://github.com/maxia-cl/KiBoard-app) | App móvil | Público |

`KiBoard-protocol` es la **fuente de verdad**. Primero se cambia ahí, después aquí.
Las compilaciones firmadas y su feed de actualización se publican en Releases de este repositorio.

## Estado

**FP hasta F6 listas, F7 a medias.** `KiBoard-protocol` está fijado como submódulo de git en
`KiBoard-protocol/`, tag `v0.5.0`; `npm run dev`/`build` regeneran `src/tokens.g.css` desde ahí
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

Las compilaciones se publican en Releases de este repositorio. El workflow corre con un tag `v*`,
firma los artefactos del updater, escribe `latest.json` y abre el release como **borrador** para
revisión. Se niega a publicar si falta `TAURI_SIGNING_PRIVATE_KEY`. El script
`tool/setup-windows-updater-signing.ps1` se ejecuta sólo una vez en la máquina de publicación y la
clave con su contraseña deben respaldarse fuera de Git.

La firma del actualizador y la firma de editor de Windows son controles distintos. Antes de
publicar un borrador, `tool/verify-authenticode.ps1` exige una firma Authenticode confiable en el
ejecutable y en cada instalador NSIS/MSI. Un certificado autofirmado sirve sólo para desarrollo
local y no satisface esta verificación de release.

**La versión es `2.0.1`** en `Cargo.toml` y en `tauri.conf.json`, y la app del teléfono va igual.
`0.1.29` era donde quedó la numeración de v1 y cruzó en el port con subtree; host y teléfono
comparten número porque con `wss://` hay que instalarlos juntos de todos modos.

La clave pública del updater queda en `tauri.conf.json`; su mitad privada existe únicamente como
secreto de GitHub Actions y en el respaldo protegido del responsable de publicación.

Una release debe instalarse desde el paquete NSIS/MSI producido por `npx tauri build` o descargado
desde GitHub Releases. Nunca se debe copiar `src-tauri/target/release/desktop.exe` después de un
build directo de Cargo: sin la feature `custom-protocol` de Tauri, ese ejecutable deja la ventana
apuntando al servidor de desarrollo en `localhost`. Para pruebas locales se debe ejecutar
`tool/install-built-release.ps1`: sólo acepta el paquete NSIS de Tauri y reinicia la aplicación
instalada.

## Stack

Tauri 2 + Rust, UI del editor en Svelte 5 + Vite. Windows primero; el código específico de
plataforma se aísla en `platform/` desde el día uno, para que macOS y Linux puedan venir después,
cada uno en su propio repo, sin descoserlo.

## Convenciones

Todo el código va en inglés — identificadores, comentarios, commits y logs. Las cadenas que ve el
usuario pasan por `i18n`, nunca van en línea. Los documentos van en inglés y español
(`NOMBRE.md` / `NOMBRE.es.md`). Ver
[`CONTRIBUTING.es.md`](https://github.com/maxia-cl/KiBoard-protocol/blob/main/CONTRIBUTING.es.md).

## Licencia

KiBoard es código abierto bajo la [licencia MIT](LICENSE).
