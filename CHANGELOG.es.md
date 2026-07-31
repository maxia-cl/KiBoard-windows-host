# Changelog

[English](CHANGELOG.md) · **Español**

Cambios destacables del host de KiBoard. La app del teléfono tiene el suyo; el contrato de mensajes
se versiona aparte en [`KiBoard-protocol`](https://github.com/maxia-cl/KiBoard-protocol).

Todavía no se ha publicado nada — v2 nunca salió, así que lo de abajo son las fases de
implementación, no tags de release. El primer release necesita además su propia clave de firma y su
propio número de versión (ver "Publicación y actualizaciones" en el README).

## Sin publicar — protocolo `v0.3.0-f7`

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
