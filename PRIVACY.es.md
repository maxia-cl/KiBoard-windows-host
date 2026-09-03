# Política de privacidad de KiBoard para Windows

Vigente desde: 3 de septiembre de 2026

KiBoard Host conecta un teléfono o tablet Android con un PC Windows y utiliza analítica anónima
para entender qué funciones se usan y detectar flujos que necesitan mejoras. KiBoard no muestra
publicidad, no crea cuentas y no vende datos.

## Analítica de uso

El host envía eventos a Aptabase cuando se inicia la aplicación y cuando el usuario interactúa con
KiBoard. Los eventos pueden indicar:

- el tipo de interacción, como pulsar una tecla del tablero, cambiar página o modo, abrir el
  Launcher, usar el trackpad, dictar, emparejar un dispositivo o editar un tablero;
- contexto funcional no identificable, como modo automático/manual, posición de la tecla, tipo de
  pulsación, categoría de la acción, identificador de una superficie incorporada, orientación,
  tamaño de la cuadrícula y resultado aceptado o rechazado; los perfiles personalizados se agrupan
  como una sola categoría;
- versión de KiBoard, idioma de Windows, sistema operativo, compilación de desarrollo/producción y
  un identificador aleatorio que dura sólo durante la ejecución actual del host.

KiBoard **no envía** a Aptabase nombres de aplicaciones o ventanas, nombres de dispositivos,
nombres de tableros o perfiles, etiquetas o acciones personalizadas, texto escrito o dictado,
audio, direcciones locales, códigos o tokens de emparejamiento ni certificados.

El endpoint configurado usa la región de Estados Unidos de Aptabase. Aptabase indica que no usa
identificadores permanentes del dispositivo y que genera en el servidor un identificador diario a
partir de la dirección IP, el agente de usuario y una sal rotativa. También indica que conserva los
eventos hasta por cinco años. Consulte <https://aptabase.com/legal/privacy>.

## Datos locales y conexión

Los dispositivos emparejados, tableros, perfiles, configuración y tokens se guardan localmente en
el PC. Las órdenes del teléfono viajan cifradas por la red local al host para ejecutar la acción
solicitada. Esta información no forma parte de los eventos de analítica descritos arriba.

## Contacto y cambios

Las consultas de privacidad pueden abrirse en
<https://github.com/maxia-cl/KiBoard-windows-host/issues>. Los cambios importantes se publicarán en
este repositorio con una nueva fecha de vigencia.
