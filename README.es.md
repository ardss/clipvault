# ClipVault

[![Release](https://img.shields.io/github/v/release/ardss/clipvault)](../../releases)
[![CI](https://img.shields.io/github/actions/workflow/status/ardss/clipvault/ci.yml?label=CI)](../../actions/workflows/ci.yml)
[![License](https://img.shields.io/github/license/ardss/clipvault)](LICENSE)

**Español** | [English](README.md) | [简体中文](README.zh-CN.md) | [日本語](README.ja.md) | [한국어](README.ko.md) | [Français](README.fr.md) | [Deutsch](README.de.md)

El Win+V mejorado: un gestor de historial del portapapeles de Windows limpio y minimalista.

Pulsa `Alt+V` para invocar el panel → busca → haz clic para pegar de vuelta donde estabas. 100% local: sin red, sin cuenta, sin telemetría.

## Características

- **Historial duradero**: texto, imágenes, archivos, texto enriquecido (formatos web/Word), enlaces — sobrevive a los reinicios, retención configurable
- **Búsqueda**: filtrado instantáneo por palabra clave (chino e inglés)
- **Fijar**: mantén las entradas frecuentes arriba; excluidas de la limpieza
- **Filtros**: todo / texto / imágenes / enlaces / archivos / fijados
- **Pegar de vuelta**: el panel nunca roba el foco de tu trabajo; haz clic en una entrada y cae donde estabas
- **Vista previa al pasar el ratón**: el texto truncado y las miniaturas se expanden en una vista lateral (desplazable, seleccionable, copiable en parte)
- **Primero el teclado**: `↑↓` navegar, `Enter` pegar, `Supr` eliminar, `Esc` cerrar
- **Tema oscuro y claro, interfaz en 7 idiomas** (es/en/zh/ja/ko/fr/de): se cambia dentro del panel y se recuerda
- **Estadísticas**: totales / categorías / tendencia de 7 días / top 5 pegados
- **Autoinicio opcional**, icono en la bandeja, altura del panel arrastrable que se recuerda
- **Pausar captura** (un clic en la bandeja, reanudación automática opcional) y **borrar todo el historial** con un clic cada uno
- **Interceptación de privacidad, todo opcional**: filtro de palabras sensibles (con interruptor), exclusión de apps, interruptor de captura de imágenes; las copias marcadas por gestores de contraseñas nunca se registran (estándar de Windows)
- **Atajo de invocación configurable** (re-registro seguro ante conflictos)
- **Texto oversized** (>256 KB) se guarda en un archivo local con vista previa en línea — nada se pierde en silencio

## Privacidad

- 100% local. Los datos viven en `%APPDATA%\com.clipvault.app\` (SQLite WAL + archivos PNG)
- Las copias de gestores de contraseñas nunca se registran (se respetan los marcadores sensibles del portapapeles)
- Filtro de palabras sensibles definido por el usuario
- Sin código de red en el binario. Detalles en [SECURITY.md](SECURITY.md)

## Instalación

Descarga un instalador de [Releases](../../releases), o compila desde el código:

```bash
npm install
npx tauri build        # salida en src-tauri/target/release/bundle/
```

## Compilación

Tauri 2 + Vue 3 + Rust (rusqlite / capa Win32 escrita a mano).

> **Nota:** compila siempre las versiones con `npx tauri build`. Un `cargo build --release`
> desnudo deja la configuración de desarrollo activa y la ventana intentará cargar la UI
> desde el servidor de desarrollo de vite (ver [docs/postmortem-2026-09-11.md](docs/postmortem-2026-09-11.md)).

Modo desarrollo:

```bash
npx vite --port 5173        # terminal 1
npx tauri dev               # terminal 2
```

Pruebas: `cd src-tauri && cargo test` (ida y vuelta del portapapeles / decodificación DIB / análisis DROPFILES)

## Pruebas y aceptación

Ver [docs/TESTING.md](docs/TESTING.md) y [docs/RELEASE_CHECKLIST.md](docs/RELEASE_CHECKLIST.md).

## Notas técnicas

- Compatibilidad de escritura del portapapeles: las imágenes se ofrecen tanto como CF_DIB estándar (bottom-up) como en el formato registrado "PNG" — las apps basadas en Chromium (Chrome/VS Code/Electron) y las apps GDI las leen correctamente; la decodificación DIB soporta máscaras de canal BI_BITFIELDS (herramientas de captura)
- La tecla de pegado la emite un subproceso inyector bajo demanda (auto-reparable) (en algunas configuraciones las teclas sintéticas en el propio proceso se tragan)
- Vigilante de latidos del IPC de WebView: el panel detecta un canal IPC muerto y se recarga (chequeo JS cada 5 s, respaldo nativo en 60 s)
- Todos los datos viven en `%APPDATA%\com.clipvault.app\`

## Licencia

[MIT](LICENSE)
