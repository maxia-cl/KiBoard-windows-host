# Changelog

[English](CHANGELOG.md) · **Español**

Cambios destacables del host de KiBoard. La app del teléfono tiene el suyo; el contrato de mensajes
se versiona aparte en [`KiBoard-protocol`](https://github.com/maxia-cl/KiBoard-protocol).

## 2.0.1 (2026-09-03) — protocolo `v0.5.0`

### Añadido

- Analítica anónima y opcional de interacciones con vocabulario fijo seguro y sin contenido del
  usuario.
- Una vista previa del tablero automático de Windows que replica el layout Android en vivo.
- Íconos expresivos compartidos e imágenes de aplicaciones en alta resolución.

### Cambiado

- El modo Manual edita sólo tableros personalizados de KiBoard; el Launcher generado sigue siendo
  automático.
- Launcher conserva las apps visuales usadas durante los últimos 30 días y ordena primero las más
  recientes.
- Elegir o abrir una app desde Launcher vuelve directamente a su tablero automático.
- La configuración Manual queda limitada a las acciones que ocupan usuarios comunes.

### Corregido

- Una instalación de release ya no puede arrancar un binario que apunte al servidor de desarrollo.
- El manifiesto del actualizador usa la preferencia NSIS compatible de Tauri.

## 2.0.0 (2026-09-02) — protocolo `v0.3.0-f7`

El primer número de versión de KiBoard 2. `0.1.29` era donde quedó la numeración de v1 y cruzó en el
port con subtree; el host y la app del teléfono comparten `2.0.0` porque con `wss://` hay que
instalarlos juntos de todos modos.

### Rompe compatibilidad

- **El transporte es `wss://`.** Todo cruzaba la LAN en texto plano, incluido el token de `hello`,
  que es toda la autoridad de un dispositivo. Ahora el host genera un certificado autofirmado por
  instalación en `cert.der`/`key.der` junto a `config.json` y sirve TLS con él; mDNS anuncia
  `tls = 1`. **No hay respaldo en texto plano**, y un teléfono viejo no puede hablar con un host
  nuevo ni al revés — hay que recompilar e instalar los dos juntos.
- El certificado es estado por instalación, como `host_id`. Regenerarlo deja fuera a todos los
  dispositivos emparejados y a cada uno le parece un ataque.

### Añadido

- **El primer arranque se explica solo** (B1): sin nada emparejado, el panel de emparejamiento se
  abre por su cuenta y dice los tres pasos, incluido el diálogo de red de Windows que te deja sin
  mDNS si lo cierras.
- **El PC dice dónde está** (R1): el panel y `pairing_status` muestran `ip:port`, para poder decirle
  a un teléfono qué escribir en una red que no deja pasar multicast.
- **En las teclas de OBS manda OBS**: una tecla de escena se enciende solo mientras está al aire, y
  el deck se repinta cuando OBS cambia sin que nadie pulse nada.
- Teclas de dos estados, cadenas de acciones, imágenes propias y compartir por archivo (paridad con
  Elgato).
- El catálogo de apps vía `Get-StartApps`, un deck **Launcher** generado, y `launch:` / `focus:` /
  `kill:`.
- El editor sobre datos reales, con vista previa en vivo al teléfono de decks todavía sin guardar.

### Arreglado

- KiBoard ya no reescribe su entrada de inicio de Windows en cada ejecución, y ahora informa los
  errores de registro en vez de ignorarlos silenciosamente.
- El deck Launcher nunca llegó a quien ya tenía un config: el sembrado estaba guardado por
  `decks.is_empty()`, que se salta en silencio a todos los usuarios existentes. Ahora se rellena, y
  se registra aunque no añada nada, para que un Launcher borrado siga borrado.
- Las teclas `danger` se pintaban en rojo pero nunca preguntaban: un toque mal dado cerraba la app
  en primer plano.
- 81 de los 93 nombres de icono que usaban los perfiles por defecto no estaban dibujados y caían en
  un cuadrado en blanco. Ahora los dos vocabularios tienen 105 nombres y un test falla si vuelven a
  separarse.
- `cargo test` reescribía el config real de quien desarrollaba.

### Cambiado

- El endpoint del updater apuntaba al feed de releases de **v1**, cuya firma habría validado — un
  host de v2 podía haberse instalado v1 encima. Ahora apunta a
  `maxia-cl/KiBoard-windows-host-releases`.
