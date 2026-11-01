//! Window text: English, Chinese, Brazilian Portuguese, Spanish and French.

use crate::protocol::{Mode, Param, MODES};

pub struct Text {
    /// In the order of `MODES`.
    pub mode_names: [&'static str; 7],
    /// In the order of `PARAM_KEYS`.
    pub param_labels: [&'static str; 5],
    pub effects: &'static str,
    pub color: &'static str,
    pub settings: &'static str,
    pub output: &'static str,
    pub red: &'static str,
    pub green: &'static str,
    pub blue: &'static str,
    pub hex: &'static str,
    pub brightness: &'static str,
    pub direction: &'static str,
    pub left: &'static str,
    pub right: &'static str,
    pub lighting: &'static str,
    pub bypass: &'static str,
    pub bypass_hint: &'static str,
    pub keep: &'static str,
    pub keep_hint: &'static str,
    pub keep_no_tray: &'static str,
    pub appearance: &'static str,
    pub system: &'static str,
    pub light: &'static str,
    pub dark: &'static str,
    pub reset: &'static str,
    pub reset_title: &'static str,
    pub reset_body: &'static str,
    pub reset_confirm: &'static str,
    pub cancel: &'static str,
    pub profiles: &'static str,
    pub save_profile: &'static str,
    pub save_profile_hint: &'static str,
    pub profile_name: &'static str,
    pub save: &'static str,
    pub delete: &'static str,
    pub delete_profile: &'static str,
    pub about: &'static str,
    pub quit: &'static str,
    pub show: &'static str,
    pub retry: &'static str,
    pub about_text: &'static str,
    pub connecting: &'static str,
    pub connected: &'static str,
    pub dry_run: &'static str,
    pub disconnected: &'static str,
    pub gpu_temperature: &'static str,
    pub vram_temperature: &'static str,
    pub gpu_usage: &'static str,
    pub vram_used: &'static str,
    pub usage_hint: &'static str,
    pub celsius: &'static str,
    pub fahrenheit: &'static str,
    pub load_short: &'static str,
    pub temp_short: &'static str,
    pub used_short: &'static str,
}

static ENGLISH: Text = Text {
    mode_names: ["Rainbow", "Chase", "Gradient", "Sequential", "Stacking", "Breathing", "Custom"],
    param_labels: ["Response", "Speed", "Interval", "Both directions", "Tempo"],
    effects: "Effects",
    color: "Color",
    settings: "Settings",
    output: "Output",
    red: "Red",
    green: "Green",
    blue: "Blue",
    hex: "Hex",
    brightness: "Brightness",
    direction: "Direction",
    left: "Left",
    right: "Right",
    lighting: "Lighting",
    bypass: "Motherboard Control",
    bypass_hint: "Let the motherboard ARGB header drive the LEDs",
    keep: "Keep Running in Tray",
    keep_hint: "Closing the window leaves the app in the system tray",
    keep_no_tray: "No system tray is available on this desktop",
    appearance: "Appearance",
    system: "Follow System",
    light: "Light",
    dark: "Dark",
    reset: "Restore Defaults",
    reset_title: "Restore Default Lighting?",
    reset_body: "All effect settings and saved colors will be lost, and the card will switch to Rainbow. This can't be undone.",
    reset_confirm: "Restore",
    cancel: "Cancel",
    profiles: "Profiles",
    save_profile: "Save Current Lighting",
    save_profile_hint: "Saves the current effect and its settings. Using an existing name replaces that profile.",
    profile_name: "Profile Name",
    save: "Save",
    delete: "Delete",
    delete_profile: "Delete This Profile?",
    about: "About",
    quit: "Quit",
    show: "Show Window",
    retry: "Retry",
    about_text: "ARGB lighting control for graphics cards",
    connecting: "Connecting…",
    connected: "Connected on i2c-",
    dry_run: "Dry run: nothing is sent to the card",
    disconnected: "Not connected",
    celsius: "Celsius",
    fahrenheit: "Fahrenheit",
    load_short: "Load",
    temp_short: "Temp",
    used_short: "Used",
    gpu_temperature: "GPU temperature",
    vram_temperature: "VRAM temperature",
    gpu_usage: "GPU usage",
    vram_used: "VRAM used",
    usage_hint: "Counts your own apps. The desktop compositor may be left out, so the real figure can be higher.",
};

static CHINESE: Text = Text {
    mode_names: ["彩虹", "追逐", "渐变", "顺序", "堆叠", "呼吸", "自定义"],
    param_labels: ["响应", "速度", "间隔", "双向", "节奏"],
    effects: "灯效",
    color: "颜色",
    settings: "设置",
    output: "输出",
    red: "红",
    green: "绿",
    blue: "蓝",
    hex: "十六进制",
    brightness: "亮度",
    direction: "方向",
    left: "左",
    right: "右",
    lighting: "灯光",
    bypass: "主板控制",
    bypass_hint: "由主板 ARGB 接口控制灯光",
    keep: "保持在托盘运行",
    keep_hint: "关闭窗口后继续在系统托盘运行",
    keep_no_tray: "此桌面没有可用的系统托盘",
    appearance: "外观",
    system: "跟随系统",
    light: "浅色",
    dark: "深色",
    reset: "恢复默认",
    reset_title: "恢复默认灯光?",
    reset_body: "所有灯效设置和已保存的颜色都将丢失,显卡将切换为彩虹灯效。此操作无法撤销。",
    reset_confirm: "恢复",
    cancel: "取消",
    profiles: "配置文件",
    save_profile: "保存当前灯光",
    save_profile_hint: "保存当前灯效及其设置。使用已有名称将替换该配置。",
    profile_name: "配置名称",
    save: "保存",
    delete: "删除",
    delete_profile: "删除此配置?",
    about: "关于",
    quit: "退出",
    show: "显示窗口",
    retry: "重试",
    about_text: "显卡 ARGB 灯光控制",
    connecting: "正在连接…",
    connected: "已连接 i2c-",
    dry_run: "演示模式:不会发送到显卡",
    disconnected: "未连接",
    celsius: "摄氏度",
    fahrenheit: "华氏度",
    load_short: "负载",
    temp_short: "温度",
    used_short: "已用",
    gpu_temperature: "GPU 温度",
    vram_temperature: "显存温度",
    gpu_usage: "GPU 使用率",
    vram_used: "已用显存",
    usage_hint: "仅统计你自己的应用。桌面合成器可能未计入，实际数值可能更高。",
};

static PORTUGUESE: Text = Text {
    mode_names: ["Arco-íris", "Corrida", "Gradiente", "Sequencial", "Empilhamento", "Pulsação", "Personalizado"],
    param_labels: ["Resposta", "Velocidade", "Intervalo", "Ambas as direções", "Ritmo"],
    effects: "Efeitos",
    color: "Cor",
    settings: "Ajustes",
    output: "Saída",
    red: "Vermelho",
    green: "Verde",
    blue: "Azul",
    hex: "Hexadecimal",
    brightness: "Brilho",
    direction: "Direção",
    left: "Esquerda",
    right: "Direita",
    lighting: "Iluminação",
    bypass: "Controle pela placa-mãe",
    bypass_hint: "Deixa o conector ARGB da placa-mãe controlar os LEDs",
    keep: "Manter na bandeja do sistema",
    keep_hint: "Fechar a janela mantém o app na bandeja do sistema",
    keep_no_tray: "Este desktop não tem bandeja do sistema",
    appearance: "Aparência",
    system: "Seguir o sistema",
    light: "Claro",
    dark: "Escuro",
    reset: "Restaurar padrões",
    reset_title: "Restaurar a iluminação padrão?",
    reset_body: "Todos os ajustes de efeitos e cores salvas serão perdidos, e a placa mudará para Arco-íris. Isso não pode ser desfeito.",
    reset_confirm: "Restaurar",
    cancel: "Cancelar",
    profiles: "Perfis",
    save_profile: "Salvar iluminação atual",
    save_profile_hint: "Salva o efeito atual e seus ajustes. Usar um nome existente substitui esse perfil.",
    profile_name: "Nome do perfil",
    save: "Salvar",
    delete: "Excluir",
    delete_profile: "Excluir este perfil?",
    about: "Sobre",
    quit: "Sair",
    show: "Mostrar janela",
    retry: "Tentar novamente",
    about_text: "Controle de iluminação ARGB para placas de vídeo",
    connecting: "Conectando…",
    connected: "Conectado em i2c-",
    dry_run: "Simulação: nada é enviado à placa",
    disconnected: "Não conectado",
    celsius: "Celsius",
    fahrenheit: "Fahrenheit",
    load_short: "Uso",
    temp_short: "Temp",
    used_short: "Usada",
    gpu_temperature: "Temperatura da GPU",
    vram_temperature: "Temperatura da VRAM",
    gpu_usage: "Uso da GPU",
    vram_used: "VRAM usada",
    usage_hint: "Conta os seus próprios aplicativos. O compositor da área de trabalho pode ficar de fora, então o valor real pode ser maior.",
};

static SPANISH: Text = Text {
    mode_names: ["Arcoíris", "Persecución", "Degradado", "Secuencial", "Apilado", "Respiración", "Personalizado"],
    param_labels: ["Respuesta", "Velocidad", "Intervalo", "Ambas direcciones", "Ritmo"],
    effects: "Efectos",
    color: "Color",
    settings: "Ajustes",
    output: "Salida",
    red: "Rojo",
    green: "Verde",
    blue: "Azul",
    hex: "Hexadecimal",
    brightness: "Brillo",
    direction: "Dirección",
    left: "Izquierda",
    right: "Derecha",
    lighting: "Iluminación",
    bypass: "Control por la placa base",
    bypass_hint: "Permite que el conector ARGB de la placa base controle los LED",
    keep: "Mantener en la bandeja del sistema",
    keep_hint: "Al cerrar la ventana, la aplicación sigue en la bandeja del sistema",
    keep_no_tray: "Este escritorio no tiene bandeja del sistema",
    appearance: "Apariencia",
    system: "Seguir al sistema",
    light: "Claro",
    dark: "Oscuro",
    reset: "Restaurar valores predeterminados",
    reset_title: "¿Restaurar la iluminación predeterminada?",
    reset_body: "Se perderán todos los ajustes de efectos y los colores guardados, y la tarjeta cambiará a Arcoíris. Esta acción no se puede deshacer.",
    reset_confirm: "Restaurar",
    cancel: "Cancelar",
    profiles: "Perfiles",
    save_profile: "Guardar iluminación actual",
    save_profile_hint: "Guarda el efecto actual y sus ajustes. Usar un nombre existente reemplaza ese perfil.",
    profile_name: "Nombre del perfil",
    save: "Guardar",
    delete: "Eliminar",
    delete_profile: "¿Eliminar este perfil?",
    about: "Acerca de",
    quit: "Salir",
    show: "Mostrar ventana",
    retry: "Reintentar",
    about_text: "Control de iluminación ARGB para tarjetas gráficas",
    connecting: "Conectando…",
    connected: "Conectado en i2c-",
    dry_run: "Simulación: no se envía nada a la tarjeta",
    disconnected: "Sin conexión",
    celsius: "Celsius",
    fahrenheit: "Fahrenheit",
    load_short: "Uso",
    temp_short: "Temp",
    used_short: "Usada",
    gpu_temperature: "Temperatura de la GPU",
    vram_temperature: "Temperatura de la VRAM",
    gpu_usage: "Uso de la GPU",
    vram_used: "VRAM usada",
    usage_hint: "Cuenta tus propias aplicaciones. El compositor del escritorio puede quedar fuera, así que el valor real puede ser mayor.",
};

static FRENCH: Text = Text {
    mode_names: ["Arc-en-ciel", "Poursuite", "Dégradé", "Séquentiel", "Empilement", "Respiration", "Personnalisé"],
    param_labels: ["Réponse", "Vitesse", "Intervalle", "Deux directions", "Rythme"],
    effects: "Effets",
    color: "Couleur",
    settings: "Réglages",
    output: "Sortie",
    red: "Rouge",
    green: "Vert",
    blue: "Bleu",
    hex: "Hexadécimal",
    brightness: "Luminosité",
    direction: "Direction",
    left: "Gauche",
    right: "Droite",
    lighting: "Éclairage",
    bypass: "Contrôle par la carte mère",
    bypass_hint: "Laisse le connecteur ARGB de la carte mère piloter les LED",
    keep: "Garder dans la zone de notification",
    keep_hint: "Fermer la fenêtre laisse l'application dans la zone de notification",
    keep_no_tray: "Ce bureau n'a pas de zone de notification",
    appearance: "Apparence",
    system: "Suivre le système",
    light: "Clair",
    dark: "Sombre",
    reset: "Restaurer les valeurs par défaut",
    reset_title: "Restaurer l'éclairage par défaut ?",
    reset_body: "Tous les réglages d'effets et les couleurs enregistrées seront perdus, et la carte passera en Arc-en-ciel. Cette action est irréversible.",
    reset_confirm: "Restaurer",
    cancel: "Annuler",
    profiles: "Profils",
    save_profile: "Enregistrer l'éclairage actuel",
    save_profile_hint: "Enregistre l'effet actuel et ses réglages. Un nom existant remplace ce profil.",
    profile_name: "Nom du profil",
    save: "Enregistrer",
    delete: "Supprimer",
    delete_profile: "Supprimer ce profil ?",
    about: "À propos",
    quit: "Quitter",
    show: "Afficher la fenêtre",
    retry: "Réessayer",
    about_text: "Contrôle de l'éclairage ARGB pour cartes graphiques",
    connecting: "Connexion…",
    connected: "Connecté sur i2c-",
    dry_run: "Simulation : rien n'est envoyé à la carte",
    disconnected: "Non connecté",
    celsius: "Celsius",
    fahrenheit: "Fahrenheit",
    load_short: "Util.",
    temp_short: "Temp",
    used_short: "Util.",
    gpu_temperature: "Température du GPU",
    vram_temperature: "Température de la VRAM",
    gpu_usage: "Utilisation du GPU",
    vram_used: "VRAM utilisée",
    usage_hint: "Compte vos propres applications. Le compositeur du bureau peut être exclu, la valeur réelle peut donc être plus élevée.",
};

const PARAM_KEYS: [&str; 5] = ["response", "speed", "interval", "bidirectional", "tempo"];

impl Text {
    pub fn mode_name(&self, mode: &Mode) -> &'static str {
        let index = MODES.iter().position(|item| item.key == mode.key);
        self.mode_names[index.expect("mode comes from MODES")]
    }

    pub fn param_label(&self, param: &Param) -> &'static str {
        let index = PARAM_KEYS.iter().position(|key| *key == param.key);
        self.param_labels[index.expect("every setting has a label")]
    }
}

/// Picks by the language part alone, so `es_MX`, `es_AR` and `es_ES` all share
/// one Spanish, and `fr_CA` and `pt_PT` fall to their language's translation.
fn for_locale(locale: &str) -> &'static Text {
    match locale
        .split(['_', '.', '@', '-'])
        .next()
        .unwrap_or_default()
    {
        "zh" => &CHINESE,
        "pt" => &PORTUGUESE,
        "es" => &SPANISH,
        "fr" => &FRENCH,
        _ => &ENGLISH,
    }
}

fn system_locale() -> String {
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|name| std::env::var(name).ok())
        .find(|value| !value.is_empty())
        .unwrap_or_default()
}

pub fn text(language: &str) -> &'static Text {
    match language {
        "auto" => for_locale(&system_locale()),
        chosen => for_locale(chosen),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regional_variants_share_their_language() {
        for locale in ["es_MX.UTF-8", "es_AR.UTF-8", "es_ES.UTF-8", "es"] {
            assert_eq!(for_locale(locale).cancel, "Cancelar", "{locale}");
            assert_eq!(for_locale(locale).retry, "Reintentar", "{locale}");
        }
        for locale in ["pt_BR.UTF-8", "pt_PT.UTF-8"] {
            assert_eq!(for_locale(locale).retry, "Tentar novamente", "{locale}");
        }
        for locale in ["fr_FR.UTF-8", "fr_CA.UTF-8", "fr_BE@euro"] {
            assert_eq!(for_locale(locale).retry, "Réessayer", "{locale}");
        }
        assert_eq!(for_locale("zh_TW.UTF-8").retry, "重试");
    }

    #[test]
    fn unknown_locales_fall_back_to_english() {
        for locale in ["", "C", "POSIX", "de_DE.UTF-8", "C.UTF-8"] {
            assert_eq!(for_locale(locale).retry, "Retry", "{locale}");
        }
    }

    #[test]
    fn every_language_names_every_mode_and_setting() {
        for language in ["en", "zh", "pt", "es", "fr"] {
            let text = text(language);
            for mode in &MODES {
                assert!(!text.mode_name(mode).is_empty());
                for param in mode.params {
                    assert!(!text.param_label(param).is_empty());
                }
            }
        }
    }

    #[test]
    fn english_table_matches_the_protocol_names() {
        for mode in &MODES {
            assert_eq!(ENGLISH.mode_name(mode), mode.name);
            for param in mode.params {
                assert_eq!(ENGLISH.param_label(param), param.label);
            }
        }
    }
}
