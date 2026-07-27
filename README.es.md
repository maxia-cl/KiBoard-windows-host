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

**F0 lista.** El host de v1 (`src-tauri/`) se portó desde `ricardomendezv/Kiboard` con un
`git subtree split` (historial completo preservado) y se modularizó en `config.rs`, `net/`,
`engine/`, `platform/`, `integrations/obs.rs` sin cambiar el comportamiento — verificado contra un
handshake WebSocket real usando el protocolo v1. `KiBoard-protocol` está fijado como submódulo de
git en `KiBoard-protocol/` (tag `v0.1.0-fp`); `npm run dev`/`build` regeneran `src/tokens.g.css`
desde ahí automáticamente.

La **maqueta del editor de la fase FP** —el dispositivo dibujado, el catálogo con búsqueda, el
inspector de teclas y las ocho operaciones de arrastrar y soltar, incluyendo deshacer/rehacer y el
camino accesible de doble clic/flechas— sigue corriendo contra un `MockBridge` en memoria, ahora
leyendo los fixtures directo desde el submódulo en vez de una copia local. Verificado
interactivamente en una sesión de `vite dev` corriendo. El cableado real de Tauri (`TauriBridge`
reemplazando a `MockBridge`) es F5. El plan de implementación está en `KiBoard-protocol`.

## Stack

Tauri 2 + Rust, UI del editor en Svelte 5 + Vite. Windows primero; el código específico de
plataforma se aísla en `platform/` desde el día uno, para que macOS y Linux puedan venir después,
cada uno en su propio repo, sin descoserlo.

## Convenciones

Todo el código va en inglés — identificadores, comentarios, commits y logs. Las cadenas que ve el
usuario pasan por `i18n`, nunca van en línea. Los documentos van en inglés y español
(`NOMBRE.md` / `NOMBRE.es.md`). Ver
[`CONTRIBUTING.es.md`](https://github.com/maxia-cl/KiBoard-protocol/blob/main/CONTRIBUTING.es.md).
