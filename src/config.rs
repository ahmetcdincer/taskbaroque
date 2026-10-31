use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::SystemTime;

use serde::Deserialize;

use crate::gfx::Color;

pub const DEFAULT_CONFIG: &str = r##"# Taskbaroque ayarları — dosyayı kaydettiğinizde değişiklikler hemen uygulanır.
# Renkler: "#RRGGBB" veya saydamlıkla "#RRGGBBAA".

[general]
locked = false            # true: widget'lar sürüklenemez
language = "auto"         # "auto" (Windows dili) | "tr" | "en"
wallpaper_accent = false  # true: vurgu rengi duvar kağıdından alınır

[theme]
# Hazır temalar: gece, aydinlik, aurora, cam, neon, orman, metal, retro, pastel, minimal,
# gunbatimi, amoled (sağ tık menüsü > Tema ile de değiştirilebilir)
# ya da aşağıdaki [themes.*] adlarından biri.
preset = "gece"
# Seçili temanın tek tek ayarlarını ezmek için açın:
# font = "Segoe UI Variable Text"
# accent = "#4CC2FF"
# text = "#FFFFFF"
# text_dim = "#FFFFFF99"
# background = "#1B1B1FD9"      # widget arka planı
# bar_background = "#18181CE6"  # görev çubuğu arka planı
# radius = 14
# Görünüm ayrıntıları (hazır temalar bunları kullanır):
# bar_gradient = ["#16123AEB", "#0A2128EB"]  # çubuk zemininde renk geçişi
# gradient_vertical = false
# sheen = "#FFFFFF26"          # üstten sönen parlaklık
# glows = ["#8B5CF666", "#10B98166"]  # alt kenarda renkli ışıklar
# border = ["#22D3EE", "#E040FB"]     # kenarlık (birden çok renk: geçiş)
# border_vertical = true
# border_width = 2
# glow = 6                     # kenarlık renginde dış parıltı
# edge = "#C8835A"             # alt kenarda ince çizgi
# tiles = "glass"              # simge kutucukları: none | glass | solid | outline
# indicator = "dot"            # açık uygulama işareti: pill | dot | dash
# start = "circle"             # Başlat düğmesi: plain | circle | square
# start_colors = ["#60A5FA", "#A78BFA", "#818CF8", "#C084FC"]
# separators = true
# brackets = true              # köşe süsleri (retro)

# Kendi temanız: menüde "Tema" altında görünür. Eksik alanlar `base` temadan alınır.
# [themes.benim]
# name = "Benim temam"
# base = "gece"
# accent = "#FFB020"
# background = "#201A10E0"

[bar]
enabled = true
monitors = "all"          # "all" (her ekranda bir çubuk) | "primary" (yalnızca ana ekran)
windows_per_monitor = false # true: her çubuk yalnızca kendi ekranındaki pencereleri gösterir
position = "bottom"       # "bottom" | "top"
style = "floating"        # görünüm: "floating" (yuvarlak, ortada) | "modular" (ayrı paneller) | "dock" | "full" (tam genişlik)
# Masaüstü düzeni bozulmasın diye çubuk Windows görev çubuğunun ayırdığı şeride oturur.
height = 44               # çubuğun yüksekliği (şerit daha alçaksa ona sığdırılır)
margin = 8                # Windows çubuğu yoksa ayrılan şeritte üst/alt boşluk
align = "center"          # "center" | "left"
icon_size = 24
show_media = true
show_weather = true
show_sysinfo = true
show_clock = true
show_tray = true          # bildirim alanı (arka planda çalışan programların simgeleri)
terrain = true            # çubuğun iki yanında pet'in dolaştığı arazi
terrain_style = "road"    # "road" (tünelli yol) | "grass" (çimenli arazi)
car = true                # yolda dolaşan araba
hotkeys = true            # Win+1…9 çubuktaki uygulamaları açar, Alt+Boşluk başlatıcıyı açar
start_icon = ""           # Başlat düğmesi için görsel (png/jpg/ico yolu); boşsa Windows logosu
clock_seconds = false
# Görev çubuğunda gösterilmeyecek programlar (exe adı). Programa sağ tık > "Görev çubuğunda gösterme" ile de eklenir.
hidden_apps = []

[notch]
# Ekranın üstünde ortada: ses çubukları, saat, albüm kapağı; üzerine gelince medya kontrolleri ve takvim
mode = "always"           # "always" (her zaman) | "media" (medya çalarken) | "hover" (üzerine gelince) | "off"
monitor = 1

[pet]
enabled = true
kind = "chameleon"        # "chameleon" (bukalemun) | "firefly" (ateş böceği) | "fox" (tilki) | "hedgehog" (kirpi)
monitor = 1               # hangi ekranda başlasın (sürükleyerek başka ekrana taşınabilir)
size = 1.0

[weather]
city = "İstanbul"
latitude = 41.01
longitude = 28.98
refresh_minutes = 30

# Widget'lar. Sağ tık menüsünden açılıp kapatılabilir, sürükleyerek taşınabilir.
#   enabled = true/false   açık / kapalı
#   monitor = 1            hangi ekranda (1 = ana ekran, 2 = ikinci ekran…)
#   x / y                  ekranın sol üst köşesine göre; negatifse sağdan/alttan ölçülür
#   layer = "desktop" (pencerelerin altında) | "top" (her zaman üstte)

[[widget]]
type = "clock"
x = -320
y = 40
seconds = true
calendar = true

[[widget]]
type = "weather"
x = -320
y = 400

[[widget]]
type = "media"
x = -380
y = 570

[[widget]]
type = "sysmon"
x = 40
y = 40
"##;

pub const DEFAULT_CONFIG_EN: &str = r##"# Taskbaroque settings — changes apply as soon as you save this file.
# Colors: "#RRGGBB" or with transparency "#RRGGBBAA".

[general]
locked = false            # true: widgets can't be dragged
language = "auto"         # "auto" (Windows language) | "en" | "tr"
wallpaper_accent = false  # true: the accent color is taken from the wallpaper

[theme]
# Built-in themes: gece, aydinlik, aurora, cam, neon, orman, metal, retro, pastel, minimal,
# gunbatimi, amoled (also from right-click menu > Theme)
# or one of the [themes.*] names below.
preset = "gece"
# Uncomment to override single settings of the selected theme:
# font = "Segoe UI Variable Text"
# accent = "#4CC2FF"
# text = "#FFFFFF"
# text_dim = "#FFFFFF99"
# background = "#1B1B1FD9"      # widget background
# bar_background = "#18181CE6"  # taskbar background
# radius = 14
# Look details (built-in themes use these):
# bar_gradient = ["#16123AEB", "#0A2128EB"]  # gradient on the bar
# gradient_vertical = false
# sheen = "#FFFFFF26"          # highlight fading from the top
# glows = ["#8B5CF666", "#10B98166"]  # colored lights along the bottom edge
# border = ["#22D3EE", "#E040FB"]     # border (several colors: gradient)
# border_vertical = true
# border_width = 2
# glow = 6                     # outer glow in the border color
# edge = "#C8835A"             # thin line on the bottom edge
# tiles = "glass"              # icon tiles: none | glass | solid | outline
# indicator = "dot"            # running app marker: pill | dot | dash
# start = "circle"             # Start button: plain | circle | square
# start_colors = ["#60A5FA", "#A78BFA", "#818CF8", "#C084FC"]
# separators = true
# brackets = true              # corner ornaments (retro)

# Your own theme: shows up under "Theme" in the menu. Missing fields come from the `base` theme.
# [themes.mine]
# name = "My theme"
# base = "gece"
# accent = "#FFB020"
# background = "#201A10E0"

[bar]
enabled = true
monitors = "all"          # "all" (a bar on every screen) | "primary" (main screen only)
windows_per_monitor = false # true: each bar shows only the windows on its own screen
position = "bottom"       # "bottom" | "top"
style = "floating"        # layout: "floating" (rounded, centered) | "modular" (separate panels) | "dock" | "full" (full width)
# So the desktop layout doesn't move, the bar sits in the strip Windows reserves for its taskbar.
height = 44               # bar height (fitted into the strip if that is lower)
margin = 8                # top/bottom gap in the reserved strip when there is no Windows taskbar
align = "center"          # "center" | "left"
icon_size = 24
show_media = true
show_weather = true
show_sysinfo = true
show_clock = true
show_tray = true          # notification area (icons of programs running in the background)
terrain = true            # ground on both sides of the bar where the pet walks
terrain_style = "road"    # "road" (road with tunnels) | "grass" (grassy ground)
car = true                # car driving on the road
hotkeys = true            # Win+1…9 opens bar apps, Alt+Space opens the launcher
start_icon = ""           # image for the Start button (png/jpg/ico path); empty = Windows logo
clock_seconds = false
# Programs not to show on the taskbar (exe name). Also added via right-click on a program > "Hide from taskbar".
hidden_apps = []

[notch]
# Top center of the screen: audio bars, clock, album art; on hover media controls, calendar and more
mode = "always"           # "always" | "media" (while media plays) | "hover" (on hover) | "off"
monitor = 1

[pet]
enabled = true
kind = "chameleon"        # "chameleon" | "firefly" | "fox" | "hedgehog"
monitor = 1               # screen to start on (can be dragged to another screen)
size = 1.0

[weather]
city = "Istanbul"
latitude = 41.01
longitude = 28.98
refresh_minutes = 30

# Widgets. Can be turned on/off from the right-click menu and moved by dragging.
#   enabled = true/false   on / off
#   monitor = 1            which screen (1 = main screen, 2 = second screen…)
#   x / y                  from the top-left corner of the screen; negative values measure from the right/bottom
#   layer = "desktop" (below windows) | "top" (always on top)

[[widget]]
type = "clock"
x = -320
y = 40
seconds = true
calendar = true

[[widget]]
type = "weather"
x = -320
y = 400

[[widget]]
type = "media"
x = -380
y = 570

[[widget]]
type = "sysmon"
x = 40
y = 40
"##;

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Config {
    pub general: General,
    pub theme: Theme,
    pub bar: BarCfg,
    pub weather: WeatherCfg,
    pub themes: BTreeMap<String, ThemeColors>,
    #[serde(rename = "widget")]
    pub widgets: Vec<WidgetCfg>,
    pub pet: PetCfg,
    pub notch: NotchCfg,
    pub calendar: CalendarCfg,
    pub github: GithubCfg,
}

#[derive(Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct CalendarCfg {
    pub ics: Vec<String>,
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct GithubCfg {
    pub enabled: bool,
}

impl Default for GithubCfg {
    fn default() -> Self {
        GithubCfg { enabled: true }
    }
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct General {
    pub locked: bool,
    pub language: String,
    pub wallpaper_accent: bool,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Theme {
    pub preset: String,
    #[serde(flatten)]
    pub overrides: ThemeColors,
}

impl Default for Theme {
    fn default() -> Self {
        Theme { preset: "gece".into(), overrides: ThemeColors::default() }
    }
}

macro_rules! theme_fields {
    ($($f:ident: $t:ty),* $(,)?) => {
        #[derive(Deserialize, Clone, Debug, Default)]
        #[serde(default)]
        pub struct ThemeColors {
            pub name: Option<String>,
            pub base: Option<String>,
            $(pub $f: Option<$t>,)*
        }

        impl ThemeColors {
            fn layer(&mut self, over: &ThemeColors) {
                $(if over.$f.is_some() {
                    self.$f = over.$f.clone();
                })*
            }
        }

        const OVERRIDE_KEYS: &[&str] = &[$(stringify!($f)),*];
    };
}

theme_fields! {
    font: String,
    accent: String,
    text: String,
    text_dim: String,
    background: String,
    bar_background: String,
    radius: f32,
    bar_gradient: Vec<String>,
    gradient_vertical: bool,
    sheen: String,
    glows: Vec<String>,
    border: Vec<String>,
    border_vertical: bool,
    border_width: f32,
    glow: f32,
    edge: String,
    tiles: String,
    indicator: String,
    start: String,
    start_colors: Vec<String>,
    separators: bool,
    brackets: bool,
}

pub const PRESET_KEYS: [&str; 12] = [
    "gece", "aydinlik", "aurora", "cam", "neon", "orman", "metal", "retro", "pastel", "minimal",
    "gunbatimi", "amoled",
];

const PRESETS_TOML: &str = r##"
[gece]
name = "Gece mavisi"
accent = "#4CC2FF"
text = "#FFFFFF"
text_dim = "#FFFFFF99"
background = "#1B1B1FD9"
bar_background = "#18181CE6"
radius = 14

[aydinlik]
name = "Aydınlık"
accent = "#0067C0"
text = "#1B1B1F"
text_dim = "#1B1B1F99"
background = "#F5F5F7E6"
bar_background = "#F3F3F5EE"
radius = 12

[aurora]
name = "Aurora"
accent = "#7DD3FC"
text = "#FFFFFF"
text_dim = "#FFFFFFA6"
background = "#11132AE8"
bar_background = "#10122AEB"
bar_gradient = ["#16123AEB", "#0D1730EB", "#0A2128EB"]
glows = ["#8B5CF666", "#3B82F655", "#10B98166"]
border = ["#8B5CF6B3", "#3B82F6B3", "#34D399B3"]
glow = 4
radius = 24
tiles = "glass"
indicator = "dot"
start = "circle"
start_colors = ["#60A5FA", "#A78BFA", "#818CF8", "#C084FC"]
separators = true

[cam]
name = "Buzlu cam"
accent = "#7DD3FC"
text = "#FFFFFF"
text_dim = "#FFFFFFBF"
background = "#3A4E5CC8"
bar_background = "#3A4E5CC0"
bar_gradient = ["#5B7586B8", "#34495AC0"]
gradient_vertical = true
sheen = "#FFFFFF26"
glows = ["#38BDF84D", "#38BDF800", "#38BDF833"]
border = ["#A5E8FFB3"]
glow = 4
radius = 26
tiles = "glass"
indicator = "dot"
start = "circle"
start_colors = ["#BAE6FD"]
separators = true

[neon]
name = "Neon"
accent = "#22D3EE"
text = "#FFFFFF"
text_dim = "#FFFFFFA6"
background = "#07060DF0"
bar_background = "#06060AF5"
border = ["#22D3EE", "#E040FB"]
border_vertical = true
border_width = 2
glow = 6
radius = 30
indicator = "dash"
start = "circle"
start_colors = ["#22D3EE", "#E879F9", "#E879F9", "#22D3EE"]
separators = true

[orman]
name = "Orman"
accent = "#6FD08C"
text = "#EAF5EC"
text_dim = "#EAF5ECA6"
background = "#122019E6"
bar_background = "#13241AEB"
bar_gradient = ["#16301FEB", "#0F2016EB", "#1C3A24EB"]
glows = ["#4ADE8033", "#4ADE8000", "#4ADE8040"]
border = ["#6EE7A0B3"]
glow = 3
radius = 24
tiles = "glass"
indicator = "dash"
start = "circle"
start_colors = ["#86EFAC"]
separators = true

[metal]
name = "Fırçalı metal"
accent = "#E39A6B"
text = "#F2F2F2"
text_dim = "#FFFFFF99"
background = "#262628F2"
bar_background = "#262628F5"
bar_gradient = ["#38383BF5", "#26262AF5", "#1C1C1FF5"]
gradient_vertical = true
glows = ["#FFFFFF00", "#FFFFFF00", "#FFFFFF14", "#FFFFFF00"]
border = ["#FFFFFF1F"]
edge = "#C8835A"
radius = 12
tiles = "solid"
indicator = "dot"
start = "circle"
start_colors = ["#F0B48A"]
separators = true

[retro]
name = "Retro piksel"
font = "Cascadia Mono"
accent = "#F5B041"
text = "#F5D7A8"
text_dim = "#C9A27AB3"
background = "#2B1224F2"
bar_background = "#2B1224F5"
border = ["#5A2A48"]
border_width = 2
radius = 3
tiles = "outline"
indicator = "dash"
start = "square"
start_colors = ["#F5B85A"]
separators = true
brackets = true

[pastel]
name = "Pastel"
accent = "#C4A1F5"
text = "#FFFFFF"
text_dim = "#FFFFFFC7"
background = "#A996D6D9"
bar_background = "#B9A6E3D0"
bar_gradient = ["#B4A3E3D0", "#C6B3E8D0", "#D8B5DCD0"]
sheen = "#FFFFFF2E"
border = ["#FFFFFF66"]
radius = 30
tiles = "glass"
indicator = "dot"
start = "circle"
start_colors = ["#FFFFFF"]
separators = true

[minimal]
name = "Minimal"
accent = "#FFFFFF"
text = "#FFFFFF"
text_dim = "#FFFFFF99"
background = "#141414F0"
bar_background = "#141414F5"
border = ["#FFFFFF10"]
radius = 8
indicator = "dot"
start_colors = ["#FFFFFF"]
separators = true

[gunbatimi]
name = "Gün batımı"
accent = "#FF8A5B"
text = "#FFF4EC"
text_dim = "#FFE3D1A0"
background = "#2A1622E0"
bar_background = "#241320EB"
radius = 14

[amoled]
name = "Siyah (AMOLED)"
accent = "#FFFFFF"
text = "#FFFFFF"
text_dim = "#FFFFFF80"
background = "#000000F2"
bar_background = "#000000FF"
radius = 8
"##;

fn presets() -> &'static BTreeMap<String, ThemeColors> {
    static P: OnceLock<BTreeMap<String, ThemeColors>> = OnceLock::new();
    P.get_or_init(|| toml::from_str(PRESETS_TOML).expect("hazır temalar ayrıştırılamadı"))
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Tiles {
    None,
    Glass,
    Solid,
    Outline,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Indicator {
    Pill,
    Dot,
    Dash,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum StartStyle {
    Plain,
    Circle,
    Square,
}

#[derive(Clone, Debug)]
pub struct Palette {
    pub font: String,
    pub accent: Color,
    pub text: Color,
    pub dim: Color,
    pub bg: Color,
    pub bar_bg: Color,
    pub on_accent: Color,
    pub radius: f32,
    pub bar_gradient: Vec<Color>,
    pub gradient_vertical: bool,
    pub sheen: Option<Color>,
    pub glows: Vec<Color>,
    pub border: Vec<Color>,
    pub border_vertical: bool,
    pub border_width: f32,
    pub glow: f32,
    pub edge: Option<Color>,
    pub tiles: Tiles,
    pub indicator: Indicator,
    pub start: StartStyle,
    pub start_colors: Vec<Color>,
    pub separators: bool,
    pub brackets: bool,
}

impl Palette {
    fn resolve(t: &ThemeColors) -> Palette {
        let col = |v: &Option<String>, d: &str| v.as_deref().and_then(Color::parse).unwrap_or_else(|| Color::parse(d).unwrap());
        let cols = |v: &Option<Vec<String>>| -> Vec<Color> {
            v.iter().flatten().filter_map(|s| Color::parse(s)).collect()
        };
        let opt = |v: &Option<String>| v.as_deref().and_then(Color::parse);
        let accent = col(&t.accent, "#4CC2FF");
        let text = col(&t.text, "#FFFFFF");
        let lum = 0.299 * accent.r + 0.587 * accent.g + 0.114 * accent.b;
        let mut border = cols(&t.border);
        if border.is_empty() {
            border.push(text.alpha(0.08));
        }
        let mut start_colors = cols(&t.start_colors);
        if start_colors.is_empty() {
            start_colors.push(accent);
        }
        Palette {
            font: t.font.clone().unwrap_or_else(|| "Segoe UI Variable Text".into()),
            accent,
            text,
            dim: col(&t.text_dim, "#FFFFFF99"),
            bg: col(&t.background, "#1B1B1FD9"),
            bar_bg: col(&t.bar_background, "#18181CE6"),
            on_accent: if lum > 0.6 { Color::rgba(12, 12, 16, 255) } else { Color::rgba(255, 255, 255, 255) },
            radius: t.radius.unwrap_or(14.0),
            bar_gradient: cols(&t.bar_gradient),
            gradient_vertical: t.gradient_vertical.unwrap_or(false),
            sheen: opt(&t.sheen),
            glows: cols(&t.glows),
            border,
            border_vertical: t.border_vertical.unwrap_or(false),
            border_width: t.border_width.unwrap_or(1.0).clamp(0.5, 4.0),
            glow: t.glow.unwrap_or(0.0).clamp(0.0, 8.0),
            edge: opt(&t.edge),
            tiles: match t.tiles.as_deref() {
                Some("glass") => Tiles::Glass,
                Some("solid") => Tiles::Solid,
                Some("outline") => Tiles::Outline,
                _ => Tiles::None,
            },
            indicator: match t.indicator.as_deref() {
                Some("dot") => Indicator::Dot,
                Some("dash") => Indicator::Dash,
                _ => Indicator::Pill,
            },
            start: match t.start.as_deref() {
                Some("circle") => StartStyle::Circle,
                Some("square") => StartStyle::Square,
                _ => StartStyle::Plain,
            },
            start_colors,
            separators: t.separators.unwrap_or(false),
            brackets: t.brackets.unwrap_or(false),
        }
    }
}

impl Config {
    pub fn palette(&self) -> Palette {
        let presets = presets();
        let custom = self.themes.get(&self.theme.preset);
        let base_key = custom.and_then(|t| t.base.as_deref()).unwrap_or(&self.theme.preset);
        let mut t = presets.get(base_key).or_else(|| presets.get(PRESET_KEYS[0])).cloned().unwrap_or_default();
        if let Some(c) = custom {
            t.layer(c);
        }
        t.layer(&self.theme.overrides);
        if self.general.wallpaper_accent {
            if let Some(c) = WALL_ACCENT.lock().ok().and_then(|g| g.clone()) {
                t.accent = Some(c);
            }
        }
        Palette::resolve(&t)
    }

    pub fn theme_list(&self) -> Vec<(String, String)> {
        let presets = presets();
        let mut v: Vec<(String, String)> = PRESET_KEYS
            .iter()
            .map(|k| {
                let name = presets.get(*k).and_then(|p| p.name.clone()).unwrap_or_else(|| k.to_string());
                (k.to_string(), crate::i18n::tr(&name).to_string())
            })
            .collect();
        for (k, t) in &self.themes {
            v.push((k.clone(), t.name.clone().unwrap_or_else(|| k.clone())));
        }
        v
    }
}

#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub struct BarCfg {
    pub enabled: bool,
    pub position: String,
    pub style: String,
    pub height: f32,
    pub margin: f32,
    pub align: String,
    pub icon_size: f32,
    pub show_media: bool,
    pub show_weather: bool,
    pub show_sysinfo: bool,
    pub show_clock: bool,
    pub show_tray: bool,
    pub terrain: bool,
    pub terrain_style: String,
    pub car: bool,
    pub hotkeys: bool,
    pub start_icon: String,
    pub clock_seconds: bool,
    pub monitors: String,
    pub windows_per_monitor: bool,
    pub hidden_apps: Vec<String>,
    pub pinned: Vec<String>,
    pub start_menu: String,
}

impl Default for BarCfg {
    fn default() -> Self {
        BarCfg {
            enabled: true,
            monitors: "all".into(),
            windows_per_monitor: false,
            hidden_apps: Vec::new(),
            pinned: Vec::new(),
            start_menu: "launcher".into(),
            position: "bottom".into(),
            style: "floating".into(),
            height: 44.0,
            margin: 8.0,
            align: "center".into(),
            icon_size: 24.0,
            show_media: true,
            show_weather: true,
            show_sysinfo: true,
            show_clock: true,
            show_tray: true,
            terrain: true,
            terrain_style: "road".into(),
            car: true,
            hotkeys: true,
            start_icon: String::new(),
            clock_seconds: false,
        }
    }
}

impl BarCfg {
    pub fn floating(&self) -> bool {
        self.style != "full"
    }
    pub fn modular(&self) -> bool {
        self.style == "modular"
    }
    pub fn dock(&self) -> bool {
        self.style == "dock"
    }
    pub fn all_monitors(&self) -> bool {
        self.monitors != "primary"
    }
    pub fn top(&self) -> bool {
        self.position == "top"
    }
    pub fn total_height(&self) -> f32 {
        self.height.clamp(28.0, 96.0) + if self.floating() { self.margin.max(0.0) * 2.0 } else { 0.0 }
    }
}

#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub struct NotchCfg {
    pub mode: String,
    pub monitor: usize,
}

impl Default for NotchCfg {
    fn default() -> Self {
        NotchCfg { mode: "always".into(), monitor: 1 }
    }
}

#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub struct PetCfg {
    pub enabled: bool,
    pub kind: String,
    pub monitor: usize,
    pub size: f32,
}

impl Default for PetCfg {
    fn default() -> Self {
        PetCfg { enabled: true, kind: "chameleon".into(), monitor: 1, size: 1.0 }
    }
}

pub const PET_KINDS: [(&str, &str); 5] = [
    ("chameleon", "Bukalemun"),
    ("firefly", "Ateş böceği"),
    ("fox", "Tilki"),
    ("hedgehog", "Kirpi"),
    ("puppy", "Köpek"),
];

#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct WeatherCfg {
    pub city: String,
    pub latitude: f64,
    pub longitude: f64,
    pub refresh_minutes: u64,
}

impl Default for WeatherCfg {
    fn default() -> Self {
        WeatherCfg { city: "İstanbul".into(), latitude: 41.01, longitude: 28.98, refresh_minutes: 30 }
    }
}

#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub struct WidgetCfg {
    #[serde(rename = "type")]
    pub kind: String,
    pub enabled: bool,
    pub monitor: usize,
    pub x: f32,
    pub y: f32,
    pub layer: String,
    pub seconds: bool,
    pub calendar: bool,
}

impl Default for WidgetCfg {
    fn default() -> Self {
        WidgetCfg { kind: String::new(), enabled: true, monitor: 1, x: 40.0, y: 40.0, layer: "desktop".into(), seconds: true, calendar: true }
    }
}

pub fn dir() -> PathBuf {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    base.join("Taskbaroque")
}

pub fn migrate_dir() {
    let Some(base) = std::env::var_os("APPDATA").map(PathBuf::from) else { return };
    let (old, new) = (base.join("WinTheme"), base.join("Taskbaroque"));
    if old.exists() && !new.exists() {
        let _ = std::fs::rename(&old, &new);
    }
}

pub fn path() -> PathBuf {
    dir().join("config.toml")
}

pub fn mtime() -> Option<SystemTime> {
    std::fs::metadata(path()).and_then(|m| m.modified()).ok()
}

pub fn load() -> Result<Config, String> {
    let p = path();
    if !p.exists() {
        let _ = std::fs::create_dir_all(dir());
        let default = if crate::i18n::en() { DEFAULT_CONFIG_EN } else { DEFAULT_CONFIG };
        std::fs::write(&p, default).map_err(|e| format!("{}: {e}", p.display()))?;
    }
    let text = std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?;
    toml::from_str(&text).map_err(|e| {
        if crate::i18n::en() {
            format!("config.toml has an error:\n{e}")
        } else {
            format!("config.toml hatalı:\n{e}")
        }
    })
}

pub fn edit(f: impl FnOnce(&mut toml_edit::DocumentMut)) {
    let p = path();
    let Ok(text) = std::fs::read_to_string(&p) else { return };
    let Ok(mut doc) = text.parse::<toml_edit::DocumentMut>() else { return };
    f(&mut doc);
    let _ = std::fs::write(&p, doc.to_string());
}

fn edit_widget(index: usize, f: impl FnOnce(&mut toml_edit::Table)) {
    edit(|doc| {
        if let Some(t) = doc.get_mut("widget").and_then(|w| w.as_array_of_tables_mut()).and_then(|a| a.get_mut(index)) {
            f(t);
        }
    });
}

pub fn save_widget_pos(index: usize, monitor: usize, x: f32, y: f32) {
    edit_widget(index, |t| {
        t["monitor"] = toml_edit::value(monitor as i64);
        t["x"] = toml_edit::value(x.round() as i64);
        t["y"] = toml_edit::value(y.round() as i64);
    });
}

pub fn set_widget_enabled(index: usize, on: bool) {
    edit_widget(index, |t| t["enabled"] = toml_edit::value(on));
}

pub fn add_widget(kind: &str, monitor: usize) {
    edit(|doc| {
        if doc.get("widget").and_then(|w| w.as_array_of_tables()).is_none() {
            doc["widget"] = toml_edit::Item::ArrayOfTables(toml_edit::ArrayOfTables::new());
        }
        let mut t = toml_edit::Table::new();
        t["type"] = toml_edit::value(kind);
        t["monitor"] = toml_edit::value(monitor as i64);
        t["x"] = toml_edit::value(120);
        t["y"] = toml_edit::value(120);
        if let Some(a) = doc["widget"].as_array_of_tables_mut() {
            a.push(t);
        }
    });
}

pub fn hide_app(exe: &str) {
    edit(|doc| {
        if !doc.contains_key("bar") {
            doc["bar"] = toml_edit::table();
        }
        let bar = &mut doc["bar"];
        if bar.get("hidden_apps").and_then(|v| v.as_array()).is_none() {
            bar["hidden_apps"] = toml_edit::value(toml_edit::Array::new());
        }
        if let Some(a) = bar["hidden_apps"].as_array_mut() {
            if !a.iter().any(|v| v.as_str().is_some_and(|s| s.eq_ignore_ascii_case(exe))) {
                a.push(exe);
            }
        }
    });
}

pub fn set_pet(kind: Option<&str>) {
    edit(|doc| {
        if !doc.contains_key("pet") {
            doc["pet"] = toml_edit::table();
        }
        doc["pet"]["enabled"] = toml_edit::value(kind.is_some());
        if let Some(k) = kind {
            doc["pet"]["kind"] = toml_edit::value(k);
        }
    });
}

pub fn set_bar_style(style: &str) {
    edit(|doc| {
        if !doc.contains_key("bar") {
            doc["bar"] = toml_edit::table();
        }
        doc["bar"]["style"] = toml_edit::value(style);
    });
}

pub const BAR_STYLES: [(&str, &str); 4] =
    [("floating", "Yuvarlak"), ("modular", "Modüler"), ("dock", "Dock"), ("full", "Tam genişlik")];

pub fn set_notch_mode(mode: &str) {
    edit(|doc| {
        if !doc.contains_key("notch") {
            doc["notch"] = toml_edit::table();
        }
        doc["notch"]["mode"] = toml_edit::value(mode);
    });
}

pub const NOTCH_MODES: [(&str, &str); 4] =
    [("always", "Her zaman"), ("media", "Medya çalarken"), ("hover", "Üzerine gelince"), ("off", "Kapalı")];

pub fn set_bar_flag(key: &str, on: bool) {
    edit(|doc| {
        if !doc.contains_key("bar") {
            doc["bar"] = toml_edit::table();
        }
        doc["bar"][key] = toml_edit::value(on);
    });
}

pub fn set_value(table: &str, key: &str, v: impl Into<toml_edit::Value>) {
    edit(|doc| {
        if !doc.contains_key(table) {
            doc[table] = toml_edit::table();
        }
        doc[table][key] = toml_edit::value(v);
    });
}

pub fn clear_hidden_apps() {
    set_value("bar", "hidden_apps", toml_edit::Array::new());
}

pub fn set_language(lang: &str) {
    edit(|doc| {
        if !doc.contains_key("general") {
            doc["general"] = toml_edit::table();
        }
        doc["general"]["language"] = toml_edit::value(lang);
    });
}

pub fn set_bar_enabled(on: bool) {
    edit(|doc| {
        if !doc.contains_key("bar") {
            doc["bar"] = toml_edit::table();
        }
        doc["bar"]["enabled"] = toml_edit::value(on);
    });
}

pub const WIDGET_KINDS: [(&str, &str); 4] =
    [("clock", "Saat / takvim"), ("weather", "Hava durumu"), ("media", "Medya"), ("sysmon", "Sistem monitörü")];

pub fn widget_label(kind: &str) -> &str {
    WIDGET_KINDS.iter().find(|(k, _)| *k == kind).map(|(_, l)| *l).unwrap_or(kind)
}

pub fn set_theme(key: &str) {
    edit(|doc| {
        if !doc.contains_key("theme") {
            doc["theme"] = toml_edit::table();
        }
        if let Some(t) = doc["theme"].as_table_mut() {
            for k in OVERRIDE_KEYS {
                t.remove(k);
            }
            t["preset"] = toml_edit::value(key);
        }
    });
}

pub fn save_locked(locked: bool) {
    edit(|doc| {
        if !doc.contains_key("general") {
            doc["general"] = toml_edit::table();
        }
        doc["general"]["locked"] = toml_edit::value(locked);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_and_presets_parse() {
        let cfg: Config = toml::from_str(DEFAULT_CONFIG).expect("varsayılan config ayrıştırılmalı");
        assert_eq!(cfg.theme.preset, "gece");
        assert_eq!(cfg.widgets.len(), 4);
        let presets = presets();
        for k in PRESET_KEYS {
            let p = presets.get(k).unwrap_or_else(|| panic!("{k} tanımlı değil"));
            assert!(p.name.is_some(), "{k}");
            let mut all: Vec<&String> = [&p.accent, &p.text, &p.text_dim, &p.background, &p.bar_background, &p.sheen, &p.edge]
                .into_iter()
                .flatten()
                .collect();
            for v in [&p.bar_gradient, &p.glows, &p.border, &p.start_colors].into_iter().flatten() {
                all.extend(v);
            }
            for c in all {
                assert!(Color::parse(c).is_some(), "{k}: {c}");
            }
        }
        assert_eq!(presets.len(), PRESET_KEYS.len());
        let mut cfg = cfg;
        cfg.theme.preset = "neon".into();
        let p = cfg.palette();
        assert_eq!(p.border.len(), 2);
        assert!(p.border_vertical && p.glow > 0.0);
    }

    #[test]
    fn custom_theme_inherits_base_and_overrides() {
        let cfg: Config = toml::from_str(
            r##"
            [theme]
            preset = "benim"
            radius = 3
            [themes.benim]
            name = "Benim"
            base = "aydinlik"
            accent = "#FF0000"
            "##,
        )
        .unwrap();
        let p = cfg.palette();
        assert_eq!(p.accent, Color::parse("#FF0000").unwrap());
        assert_eq!(p.text, Color::parse("#1B1B1F").unwrap());
        assert_eq!(p.radius, 3.0);
        assert_eq!(cfg.theme_list().last().unwrap().1, "Benim");
    }
}

pub fn set_start_icon(src: Option<&std::path::Path>) {
    let old = load().ok().map(|c| c.bar.start_icon).unwrap_or_default();
    let val = match src {
        Some(p) => {
            let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("png").to_lowercase();
            let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
            let dst = dir().join(format!("start_icon_{stamp}.{ext}"));
            if std::fs::copy(p, &dst).is_err() {
                return;
            }
            dst.to_string_lossy().into_owned()
        }
        None => String::new(),
    };
    set_value("bar", "start_icon", val.as_str());
    let old = std::path::PathBuf::from(old);
    if old.parent() == Some(dir().as_path()) && old.file_name().is_some_and(|n| n.to_string_lossy().starts_with("start_icon_")) {
        let _ = std::fs::remove_file(old);
    }
}

pub static WALL_ACCENT: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

fn edit_array(table: &str, key: &str, f: impl FnOnce(&mut toml_edit::Array)) {
    edit(|doc| {
        if !doc.contains_key(table) {
            doc[table] = toml_edit::table();
        }
        let t = &mut doc[table];
        if t.get(key).and_then(|v| v.as_array()).is_none() {
            t[key] = toml_edit::value(toml_edit::Array::new());
        }
        if let Some(a) = t[key].as_array_mut() {
            f(a);
        }
    });
}

pub fn set_pinned(paths: &[String]) {
    edit_array("bar", "pinned", |a| {
        a.clear();
        for p in paths {
            a.push(p.as_str());
        }
    });
}

pub fn add_ics(url: &str) {
    edit_array("calendar", "ics", |a| {
        if !a.iter().any(|v| v.as_str() == Some(url)) {
            a.push(url);
        }
    });
}

pub fn set_weather_place(city: &str, lat: f64, lon: f64) {
    edit(|doc| {
        if !doc.contains_key("weather") {
            doc["weather"] = toml_edit::table();
        }
        doc["weather"]["city"] = toml_edit::value(city);
        doc["weather"]["latitude"] = toml_edit::value((lat * 1000.0).round() / 1000.0);
        doc["weather"]["longitude"] = toml_edit::value((lon * 1000.0).round() / 1000.0);
    });
}

pub fn clear_ics() {
    set_value("calendar", "ics", toml_edit::Array::new());
}
