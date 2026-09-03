# Política de firma de código

**Español** · [English](CODE_SIGNING.md)

La política de releases públicas de KiBoard es usar **firma de código gratuita proporcionada por
SignPath.io, con certificado de SignPath Foundation**. Ninguna release Windows se considera lista
para producción hasta que su ejecutable y todos sus instaladores pasen la verificación confiable
de Authenticode del repositorio.

## Alcance y procedencia

- El código fuente se publica con licencia MIT en
  [`maxia-cl/KiBoard-windows-host`](https://github.com/maxia-cl/KiBoard-windows-host).
- Los candidatos se construyen desde tags públicos `v*` mediante GitHub Actions. El workflow
  descarga el protocolo público fijado, compila la aplicación Tauri y conserva la release como
  borrador hasta completar la verificación.
- `tool/verify-authenticode.ps1` rechaza cualquier ejecutable, instalador NSIS o MSI cuyo estado
  Authenticode confiable no sea `Valid`.
- Las firmas del actualizador Tauri se aplican después de Authenticode. Protegen el feed de
  actualización, pero no reemplazan la firma de editor de Windows.

## Roles del equipo

- Committers y revisores: miembros de la
  [organización `maxia-cl`](https://github.com/orgs/maxia-cl/people).
- Aprobadores de firma: propietarios de la
  [organización `maxia-cl`](https://github.com/orgs/maxia-cl/people?query=role%3Aowner).

Los mantenedores usan autenticación multifactor. Antes de aprobar una solicitud de firma, un
mantenedor debe revisar el código, el resultado de CI, las notas de release, los hashes de los
artefactos y las declaraciones de privacidad.

## Privacidad y seguridad

La [política de privacidad](PRIVACY.es.md) describe la analítica anónima opcional de interacciones
y los datos intercambiados por red local con un Android emparejado. La analítica puede desactivarse
en la configuración de la aplicación. KiBoard no firma binarios de terceros como propios ni usa su
acceso de firma para proyectos no relacionados.

Si un artefacto firmado no puede reproducirse desde el código público etiquetado, falla el análisis
de malware o no coincide con estas declaraciones, los mantenedores deben rechazar o revocar la
release.

