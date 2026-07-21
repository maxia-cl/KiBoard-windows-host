// i18n del catálogo de perfiles. La etiqueta en ESPAÑOL es la fuente de verdad (así están
// escritas en default_profiles y en los config.json ya guardados); esta tabla la traduce al
// vuelo en layout_for según el locale que el móvil declaró en su "hello". Etiquetas que no
// están en la tabla (perfiles editados por el usuario) se muestran tal cual — texto del
// usuario no se traduce.

use std::sync::atomic::{AtomicU8, Ordering};

const ES: u8 = 0;
const EN: u8 = 1;
const ZH: u8 = 2;

// ponytail: locale GLOBAL del host (el último hello gana) — per-cliente solo si algún día
// conectan a la vez dos teléfonos en idiomas distintos.
static LOCALE: AtomicU8 = AtomicU8::new(ES);

pub fn set_locale(lang: &str) {
    let l = if lang.starts_with("en") {
        EN
    } else if lang.starts_with("zh") {
        ZH
    } else {
        ES
    };
    LOCALE.store(l, Ordering::Relaxed);
}

pub fn tr(label: &str) -> &str {
    match LOCALE.load(Ordering::Relaxed) {
        // ponytail: búsqueda lineal sobre ~190 entradas cada 500ms — irrelevante; HashMap si
        // el catálogo creciera 10x.
        EN => TABLE.iter().find(|e| e.0 == label).map(|e| e.1).unwrap_or(label),
        ZH => TABLE.iter().find(|e| e.0 == label).map(|e| e.2).unwrap_or(label),
        _ => label,
    }
}

/// (es, en, zh-Hans). Cubre todas las etiquetas de default_profiles + las dinámicas de OBS.
/// El test de abajo falla si un perfil nuevo usa una etiqueta sin traducción.
static TABLE: &[(&str, &str, &str)] = &[
    ("+ Volumen", "Vol +", "音量+"),
    ("- Volumen", "Vol -", "音量-"),
    ("Aceptar", "Accept", "接受"),
    ("Adelante", "Forward", "前进"),
    ("Agenda", "Agenda", "日程"),
    ("Agrupar", "Group", "编组"),
    ("Aleatorio", "Shuffle", "随机播放"),
    ("Anterior", "Previous", "上一个"),
    ("Archivar", "Archive", "归档"),
    ("Archivos", "Files", "文件"),
    ("Asignar", "Assign", "分配"),
    ("Asignarme", "Assign me", "分给我"),
    ("Atrás", "Back", "后退"),
    ("Audio", "Audio", "音频"),
    ("Autosuma", "AutoSum", "自动求和"),
    ("Añadir rápido", "Quick add", "快速添加"),
    ("Añadir tarea", "Add task", "添加任务"),
    ("Año", "Year", "年"),
    ("Bajar", "Down", "向下"),
    ("Barra lateral", "Sidebar", "侧边栏"),
    ("Blame", "Blame", "Blame"),
    ("Blanco", "White", "白屏"),
    ("Borrador", "Eraser", "橡皮擦"),
    ("Borrar chat", "Clear chat", "清空聊天"),
    ("Bucle", "Loop", "循环"),
    ("Buffer clips", "Replay buf.", "回放缓存"),
    ("Buscar", "Find", "查找"),
    ("Buscar acción", "Find action", "查找操作"),
    ("Calendario", "Calendar", "日历"),
    ("Cambiador", "Switcher", "切换器"),
    ("Cambiar rama", "Switch branch", "切换分支"),
    ("Cancelar", "Cancel", "取消"),
    ("Canal ant.", "Prev channel", "上个频道"),
    ("Captura", "Screenshot", "截图"),
    ("Cerrar app", "Close app", "关闭应用"),
    ("Ciclar modo", "Cycle mode", "切换模式"),
    ("Cerrar pestaña", "Close tab", "关闭标签页"),
    ("Chat", "Chat", "聊天"),
    ("Chat ant.", "Prev chat", "上个聊天"),
    ("Clip", "Clip", "剪辑"),
    ("Clip 30s", "30s clip", "剪辑30秒"),
    ("Colgar", "Hang up", "挂断"),
    ("Colores", "Colors", "颜色"),
    ("Comentar", "Comment", "注释"),
    ("Compartir", "Share", "分享"),
    ("Copiar", "Copy", "复制"),
    ("Cortar", "Cut", "剪切"),
    ("Cortar directo", "End stream", "结束直播"),
    ("Crear", "Create", "创建"),
    ("Cuadrícula", "Grid", "网格"),
    ("Cursiva", "Italic", "斜体"),
    ("Cámara", "Camera", "摄像头"),
    ("Círculo", "Circle", "圆形"),
    ("Descargas", "Downloads", "下载"),
    ("Desde actual", "From current", "从当前"),
    ("Deshacer", "Undo", "撤销"),
    ("Destacar", "Star", "加星"),
    ("Detener grab.", "Stop rec.", "停止录制"),
    ("Directo", "Stream", "直播"),
    ("Dónde estoy", "Where am I", "当前路径"),
    ("Dividir", "Split", "分割"),
    ("Duplicar", "Duplicate", "创建副本"),
    ("Duplicar línea", "Dup. line", "复制行"),
    ("Día", "Day", "日"),
    ("Editar", "Edit", "编辑"),
    ("Editar último", "Edit last", "编辑上条"),
    ("Editar/Ver", "Edit/View", "编辑/查看"),
    ("Editor web", "Web editor", "网页编辑器"),
    ("Ejecutar", "Run", "运行"),
    ("Eliminar", "Delete", "删除"),
    ("Elipse", "Ellipse", "椭圆"),
    ("Enfocar entrada", "Focus input", "聚焦输入"),
    ("Enlace", "Link", "链接"),
    ("Entrada", "In", "入点"),
    ("Enviar", "Send", "发送"),
    ("Escena actual", "Current scene", "当前场景"),
    ("Esfuerzo", "Effort", "推理强度"),
    ("Estado", "Status", "状态"),
    ("Etiqueta", "Label", "标签"),
    ("Etiquetas", "Labels", "标签"),
    ("Exportar", "Export", "导出"),
    ("Favorito", "Favorite", "收藏"),
    ("Fijar", "Pin", "置顶"),
    ("Filtrar", "Filter", "筛选"),
    ("Filtro", "Filter", "筛选"),
    ("Formato", "Format", "格式化"),
    ("Game Bar", "Game Bar", "Game Bar"),
    ("Girar dcha.", "Rotate right", "向右旋转"),
    ("Girar izq.", "Rotate left", "向左旋转"),
    ("Git estado", "Git status", "Git 状态"),
    ("Grabar", "Record", "录制"),
    ("Grabar juego", "Record game", "录制游戏"),
    ("Guardados", "Saved", "收藏夹"),
    ("Guardar", "Save", "保存"),
    ("Guardar todo", "Save all", "全部保存"),
    ("Guardar/Compilar", "Save/Build", "保存/编译"),
    ("Hilos", "Threads", "消息串"),
    ("Historial", "History", "历史记录"),
    ("Hoy", "Today", "今天"),
    ("Importar", "Import", "导入"),
    ("Imprimir", "Print", "打印"),
    ("Incógnito", "Incognito", "无痕窗口"),
    ("Ir a", "Go to", "转到"),
    ("Ir a línea", "Go to line", "转到行"),
    ("Ir a página", "Go to page", "转到页"),
    ("Levantar mano", "Raise hand", "举手"),
    ("Limpiar", "Clear", "清屏"),
    ("Listar", "List", "列出文件"),
    ("Lista nueva", "New list", "新建列表"),
    ("Lupa", "Loupe", "放大视图"),
    ("Lápiz", "Pencil", "铅笔"),
    ("Línea", "Line", "直线"),
    ("Mano", "Hand", "抓手"),
    ("Marcador", "Marker", "标记"),
    ("Marcar", "Pick", "留用"),
    ("Marcar leído", "Mark read", "标为已读"),
    ("Marco", "Frame", "框架"),
    ("Me gusta", "Like", "点赞"),
    ("Mes", "Month", "月"),
    ("Mic", "Mic", "麦克风"),
    ("Miembros", "Members", "成员"),
    ("Mis tarjetas", "My cards", "我的卡片"),
    ("Modelo", "Model", "模型"),
    ("Modo", "Mode", "模式"),
    ("Mover", "Move", "移动"),
    ("Negrita", "Bold", "粗体"),
    ("Negro", "Black", "黑屏"),
    ("Nota", "Note", "便签"),
    ("Nueva carpeta", "New folder", "新建文件夹"),
    ("Nueva diap.", "New slide", "新幻灯片"),
    ("Nueva línea", "New line", "换行"),
    ("Nueva nota", "New note", "新建笔记"),
    ("Nueva pestaña", "New tab", "新标签页"),
    ("Nueva página", "New page", "新建页面"),
    ("Nueva ventana", "New window", "新窗口"),
    ("Nuevo", "New", "新建"),
    ("Nuevo chat", "New chat", "新聊天"),
    ("Nuevo correo", "New email", "新邮件"),
    ("Nuevo post", "New post", "发帖"),
    ("Organizar imports", "Fix imports", "整理导入"),
    ("Overlay", "Overlay", "悬浮界面"),
    ("Paleta", "Palette", "命令面板"),
    ("Panel sgte", "Next pane", "下个面板"),
    ("Pant. completa", "Full screen", "全屏"),
    ("Pantalla completa", "Full screen", "全屏"),
    ("Participantes", "Participants", "参会者"),
    ("Pegar", "Paste", "粘贴"),
    ("Pincel", "Brush", "画笔"),
    ("Planificar respuesta", "Plan response", "规划回复"),
    ("Play/Pausa", "Play/Pause", "播放/暂停"),
    ("Pluma", "Pen", "钢笔"),
    ("Presentar", "Present", "放映"),
    ("Prioridad", "Priority", "优先级"),
    ("Propiedades", "Properties", "属性"),
    ("Reabrir pestaña", "Reopen tab", "恢复标签页"),
    ("Recargar", "Reload", "重新加载"),
    ("Rechazar", "Reject", "排除"),
    ("Recortar", "Crop", "裁剪"),
    ("Rectángulo", "Rectangle", "矩形"),
    ("Redactar", "Compose", "写邮件"),
    ("Reemplazar", "Replace", "替换"),
    ("Reenviar", "Forward", "转发"),
    ("Reformatear", "Reformat", "重新格式化"),
    ("Rehacer", "Redo", "重做"),
    ("Rellenar", "Fill", "填充"),
    ("Relleno", "Fill", "填充"),
    ("Renombrar", "Rename", "重命名"),
    ("Repetir", "Repeat", "重复"),
    ("Repost", "Repost", "转发"),
    ("Reproducir", "Play", "播放"),
    ("Resaltar", "Highlight", "高亮"),
    ("Resp. todos", "Reply all", "回复全部"),
    ("Responder", "Reply", "回复"),
    ("Revelar", "Develop", "修改照片"),
    ("Rotar", "Rotate", "旋转"),
    ("Salida", "Out", "出点"),
    ("Salir", "Exit", "退出"),
    ("Saltar a", "Jump to", "跳转"),
    ("Sel. todo", "Select all", "全选"),
    ("Seleccionar", "Select", "选择"),
    ("Selección", "Select", "选择"),
    ("Selector color", "Color picker", "取色器"),
    ("Semana", "Week", "周"),
    ("Sgte canal", "Next channel", "下个频道"),
    ("Sgte chat", "Next chat", "下个聊天"),
    ("Siguiente", "Next", "下一个"),
    ("Silenciar", "Mute", "静音"),
    ("Silencio", "Mute", "静音"),
    ("Sincronizar", "Sync", "同步"),
    ("Subir", "Up", "向上"),
    ("Subir archivo", "Upload", "上传"),
    ("Subir nivel", "Up level", "上级目录"),
    ("Subrayado", "Underline", "下划线"),
    ("Subtítulos", "Subtitles", "字幕"),
    ("Tableros", "Boards", "看板"),
    ("Teatro", "Theater", "剧场模式"),
    ("Terminal", "Terminal", "终端"),
    ("Texto", "Text", "文本"),
    ("Vencimiento", "Due date", "截止日期"),
    ("Vista previa", "Preview", "预览"),
    ("Vídeo", "Video", "视频"),
    ("Zoom", "Zoom", "缩放"),
    ("Zoom +", "Zoom in", "放大"),
    ("Zoom -", "Zoom out", "缩小"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tabla_sin_duplicados() {
        let mut keys: Vec<&str> = TABLE.iter().map(|e| e.0).collect();
        keys.sort();
        let n = keys.len();
        keys.dedup();
        assert_eq!(n, keys.len(), "clave ES duplicada en TABLE");
    }

    #[test]
    fn catalogo_completo() {
        // Toda etiqueta del catálogo integrado debe tener traducción (si añades un perfil
        // nuevo y esto falla, agrega la etiqueta a TABLE).
        let faltan: Vec<String> = crate::default_profiles()
            .iter()
            .flat_map(|p| p.buttons.iter().map(|b| b.label.clone()))
            .filter(|l| !TABLE.iter().any(|e| e.0 == l))
            .collect();
        assert!(faltan.is_empty(), "etiquetas sin traducción: {faltan:?}");
    }

    #[test]
    fn tr_cambia_por_locale() {
        set_locale("en");
        assert_eq!(tr("Copiar"), "Copy");
        assert_eq!(tr("etiqueta custom del usuario"), "etiqueta custom del usuario");
        set_locale("zh-Hans");
        assert_eq!(tr("Copiar"), "复制");
        set_locale("es");
        assert_eq!(tr("Copiar"), "Copiar");
    }
}
