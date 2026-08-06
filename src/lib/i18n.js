// The editor window's own strings. The PC speaks WINDOWS' language — the host reads it once from
// `GetUserDefaultUILanguage` and hands it over through `host_lang`, the same value its tray menu
// uses. The phone's `hello` language never reaches here: it governs the KEY LABELS that travel to
// that phone, not the window somebody is sitting in front of.
//
// Deliberately NOT here: the catalogue's item labels (Windows, Trackpad, Record, an OBS scene, an
// app's name). Those are written INTO the deck when a key is created and then travel to every
// phone, where `i18n::tr` translates them from Spanish. Translating them at creation time would
// store, say, Chinese in the file and leave every other phone unable to translate it back.

import { invoke } from "./bridge.js";

// ponytail: a module-level `let`, not a store — the language cannot change while the window is
// open (Windows needs a sign-out to change it), so nothing ever has to re-render because of it.
let lang = "es";

/** Resolved before the app mounts, so the first frame is already in the right language. */
export async function initLang() {
  try {
    lang = await invoke("host_lang");
  } catch {
    // `vite dev` in a plain browser has no host. The browser's own language is close enough for
    // something only a developer ever sees.
    lang = (navigator?.language ?? "es").slice(0, 2);
  }
  if (!["es", "en", "zh"].includes(lang)) lang = "es";
}

/** `t("save")`, or `t("assigned", 5)` for the ones with a `{0}` in them. */
export function t(key, ...args) {
  const row = STRINGS[key];
  if (!row) return key; // a typo shows itself instead of showing nothing
  const s = row[{ es: 0, en: 1, zh: 2 }[lang]];
  return args.length ? s.replace(/\{(\d)\}/g, (_, i) => args[i]) : s;
}

/** [es, en, zh-Hans] */
const STRINGS = {
  // App.svelte
  "tab.auto": ["Auto", "Auto", "自动"],
  "tab.manual": ["Manual", "Manual", "手动"],
  "deck.name": ["Nombre del deck", "Deck name", "Deck 名称"],
  "deck.duplicate": ["Duplicar deck", "Duplicate deck", "复制 Deck"],
  "deck.export": ["Exportar el deck a un archivo", "Export deck to a file", "导出 Deck 到文件"],
  "deck.import": ["Importar un archivo de deck", "Import a deck file", "导入 Deck 文件"],
  "save.changes": ["Guardar cambios", "Save changes", "保存更改"],
  "save.done": ["Guardado", "Saved", "已保存"],
  connected: ["{0} conectados", "{0} connected", "已连接 {0} 台"],
  "pairing.title": ["Vinculación y dispositivos", "Pairing & devices", "配对与设备"],
  "load.error": ["No se pudieron leer los decks: {0}", "Could not read the decks: {0}", "无法读取 Deck：{0}"],
  "load.reading": ["Leyendo tus decks y aplicaciones…", "Reading your decks and apps…", "正在读取你的 Deck 和应用…"],
  "load.empty": ["Todavía no hay decks.", "No decks yet.", "还没有 Deck。"],
  "import.notjson": ["Ese archivo no es JSON válido", "That file is not valid JSON", "该文件不是有效的 JSON"],

  // PairingPanel.svelte
  "pair.firstrun": ["Conecta tu teléfono", "Connect your phone", "连接你的手机"],
  "pair.nohost": [
    "Solo disponible dentro de la app KiBoard real (no en vite dev en un navegador).",
    "Only available inside the real KiBoard app (not vite dev in a browser).",
    "仅在真实的 KiBoard 应用内可用（浏览器中的 vite dev 不行）。",
  ],
  "pair.step1": [
    "Instala <b>KiBoard</b> en el teléfono y ábrelo.",
    "Install <b>KiBoard</b> on the phone and open it.",
    "在手机上安装 <b>KiBoard</b> 并打开。",
  ],
  "pair.step2": [
    "Lista este PC por sí solo. Si la lista queda vacía — WiFi de invitados, algunos routers de ISP — escribe la dirección de abajo.",
    "It lists this PC by itself. If the list stays empty — guest WiFi, some ISP routers — type the address below instead.",
    "它会自动列出这台电脑。如果列表一直是空的（访客 WiFi、某些运营商路由器），请改为输入下面的地址。",
  ],
  "pair.step3": [
    "El teléfono muestra un código de seis dígitos. También aparece aquí; tienen que coincidir.",
    "The phone shows a six-digit code. It appears here too; they have to match.",
    "手机会显示一个六位数验证码，这里也会出现，两者必须一致。",
  ],
  "pair.firewall": [
    "Windows puede preguntar si KiBoard puede usar la red la primera vez. Di que <b>sí</b>, para redes privadas — si no, el teléfono no puede encontrar este PC.",
    "Windows may ask whether KiBoard can use the network the first time. Say <b>yes</b>, for private networks — the phone cannot find this PC otherwise.",
    "首次运行时 Windows 可能会询问是否允许 KiBoard 使用网络。请对专用网络选择<b>允许</b>，否则手机找不到这台电脑。",
  ],
  loading: ["Cargando…", "Loading…", "加载中…"],
  "pair.hostid": ["Id del host", "Host id", "主机 ID"],
  "pair.accept": ["Aceptar nuevas vinculaciones", "Accept new pairings", "接受新的配对"],
  "pair.address": ["Dirección", "Address", "地址"],
  "pair.addresshint": [
    "Escribe esto en el teléfono si no logra encontrar este PC solo.",
    "Type this on the phone if it cannot find this PC by itself.",
    "如果手机无法自动找到这台电脑，请在手机上输入这个地址。",
  ],
  "pair.certificate": ["Certificado", "Certificate", "证书"],
  "pair.wants": [
    '"{0}" quiere conectarse — caduca en {1}s',
    '"{0}" wants to connect — expires in {1}s',
    "“{0}”请求连接 — {1} 秒后过期",
  ],
  "pair.devices": ["Dispositivos vinculados", "Paired devices", "已配对设备"],
  "pair.nodevices": ["Todavía no hay dispositivos vinculados.", "No devices paired yet.", "还没有配对任何设备。"],
  "pair.revoke": ["Revocar", "Revoke", "撤销"],

  // Catalogue.svelte / AssignPopover.svelte
  search: ["🔍 buscar…", "🔍 search…", "🔍 搜索…"],
  "search.plain": ["Buscar…", "Search…", "搜索…"],
  "catalogue.enter": [
    "Enter lo asigna a la tecla seleccionada",
    "Enter assigns this to the selected key",
    "按 Enter 将其分配给选中的按键",
  ],
  "cat.apps": ["Aplicaciones", "Apps", "应用"],
  "cat.system": ["Sistema", "System", "系统"],
  "cat.obs": ["OBS", "OBS", "OBS"],
  "cat.pages": ["Páginas", "Pages", "页面"],

  // AutoTab.svelte
  "auto.search": ["🔍 buscar ~100 perfiles…", "🔍 search ~100 profiles…", "🔍 搜索约 100 个配置…"],
  "auto.hint": [
    "Vista previa de solo lectura. Los perfiles del modo automático cambian solos cuando cambia la app en primer plano — no se configuran arrastrando aquí, solo se miran.",
    "Read-only preview. Auto-mode profiles switch by themselves when the foreground app changes — they aren't configured by dragging here, only browsed.",
    "只读预览。自动模式的配置会随前台应用切换而自动切换 — 这里只能浏览，不能拖拽配置。",
  ],

  // Device.svelte
  "device.deck": ["Deck: {0}", "Deck: {0}", "Deck：{0}"],
  "device.screen": ["pantalla {0}/{1}", "screen {0}/{1}", "第 {0}/{1} 屏"],
  "device.back": ["← Atrás", "← Back", "← 返回"],
  "device.page": ["Página", "Page", "页面"],
  "device.screenN": ["Pantalla {0}", "Screen {0}", "第 {0} 屏"],

  // Inspector.svelte
  "insp.label": ["Etiqueta", "Label", "标签"],
  "insp.icon": ["Icono", "Icon", "图标"],
  "insp.change": ["cambiar…", "change…", "更改…"],
  "insp.image": ["Imagen", "Image", "图片"],
  "insp.backtoicon": ["Volver al icono", "Back to the icon", "恢复为图标"],
  "insp.colour": ["Color", "Colour", "颜色"],
  "insp.short": ["Pulsación corta", "Short press", "短按"],
  "insp.removestep": ["Quitar paso", "Remove step", "删除步骤"],
  "insp.addstep": ["+ Añadir paso", "+ Add step", "+ 添加步骤"],
  "insp.long": ["Larga", "Long", "长按"],
  "insp.double": ["Doble", "Double", "双击"],
  "insp.second": ["Segundo estado", "Second state", "第二状态"],
  "insp.whenon": ["Cuando está activo", "When on", "开启时"],
  "insp.facehint": [
    "Una pulsación corta cambia a esta cara. Los campos vacíos conservan lo de arriba.",
    "A short press swaps to this face. Empty fields keep what is above.",
    "短按会切换到这一面。留空的字段沿用上面的设置。",
  ],
  "insp.faceicon": ["icono…", "icon…", "图标…"],
  "insp.faceaction": [
    "Acción (vacío = la misma)",
    "Action (blank = the same one)",
    "动作（留空 = 与上面相同）",
  ],
  "insp.confirm": ["Pedir confirmación", "Ask to confirm", "需要确认"],
  "insp.test": ["▶ Probar", "▶ Test", "▶ 测试"],
  "insp.folder": [
    "Carpeta — haz doble clic en el dispositivo para abrirla y configurar su contenido.",
    "Folder — double-click it on the device to open and configure its contents.",
    "文件夹 — 在设备上双击可打开并配置其内容。",
  ],
  "insp.empty": ["Selecciona una tecla para editarla.", "Select a key to edit it.", "选择一个按键进行编辑。"],

  // IconPicker.svelte
  "icons.pick": ["Elige un icono", "Pick an icon", "选择图标"],

  // store.svelte.js — the toasts
  "toast.notsaved": ["No se guardó: {0}", "Not saved: {0}", "未保存：{0}"],
  "toast.saved": ["Guardado — teléfonos actualizados", "Saved — phones updated", "已保存 — 手机已更新"],
  "toast.assigned": ["Asignado a la tecla {0}", "Assigned to key {0}", "已分配到按键 {0}"],
  "toast.replaced": ["Reemplazada la tecla {0}", "Replaced key {0}", "已替换按键 {0}"],
  "toast.fullselect": [
    "Esta pantalla está llena — selecciona una tecla para reemplazarla",
    "This screen is full — select a key to replace",
    "这一屏已满 — 请选择一个按键进行替换",
  ],
  "toast.moved": ["Movida a la tecla {0}", "Moved to key {0}", "已移动到按键 {0}"],
  "toast.copied": ["Copiada a la tecla {0}", "Copied to key {0}", "已复制到按键 {0}"],
  "toast.swapped": ["Intercambiadas las teclas {0} y {1}", "Swapped keys {0} and {1}", "已交换按键 {0} 和 {1}"],
  "toast.cleared": ["Vaciada la tecla {0}", "Cleared key {0}", "已清空按键 {0}"],
  "toast.screenfull": ["Esa pantalla está llena", "That screen is full", "那一屏已满"],
  "toast.movedscreen": ["Movida a la pantalla {0}", "Moved to screen {0}", "已移动到第 {0} 屏"],
  "toast.duplicated": ['Duplicado como "{0}"', 'Duplicated as "{0}"', "已复制为“{0}”"],
  "toast.savefirst": [
    "Guarda primero — el archivo se escribe desde lo almacenado",
    "Save first — the file is written from what is stored",
    "请先保存 — 文件是根据已存储的内容写出的",
  ],
  "toast.exported": ["Exportado a {0}", "Exported to {0}", "已导出到 {0}"],
  "toast.exportfailed": ["Falló la exportación: {0}", "Export failed: {0}", "导出失败：{0}"],
  "toast.notadeck": ["Ese archivo no es un deck", "That file is not a deck", "该文件不是一个 Deck"],
  "toast.imported": [
    'Importado "{0}" — revísalo y luego guarda',
    'Imported "{0}" — check it, then save',
    "已导入“{0}” — 检查后再保存",
  ],
  "toast.testfailed": ["▶ falló: {0}", "▶ failed: {0}", "▶ 失败：{0}"],
};

// ponytail: one runnable check instead of a test file — the frontend has no test runner.
// `node src/lib/i18n.js` fails loudly if a string is missing a language or a placeholder.
if (typeof process !== "undefined" && process.argv?.[1]?.endsWith("i18n.js")) {
  for (const [key, row] of Object.entries(STRINGS)) {
    console.assert(row.length === 3 && row.every((s) => s), `${key}: needs es, en and zh`);
    const slots = (s) => (s.match(/\{\d\}/g) ?? []).sort().join();
    console.assert(
      new Set(row.map(slots)).size === 1,
      `${key}: the three languages disagree on placeholders`
    );
  }
  console.log(`${Object.keys(STRINGS).length} strings checked`);
}
