use std::sync::atomic::{AtomicBool, Ordering};

use windows::Win32::Globalization::GetUserDefaultUILanguage;

static EN: AtomicBool = AtomicBool::new(false);

pub const LANGUAGES: [(&str, &str); 3] = [("auto", "Otomatik"), ("tr", "Türkçe"), ("en", "English")];

pub fn system_is_turkish() -> bool {
    unsafe { GetUserDefaultUILanguage() & 0x3FF == 0x1F }
}

pub fn set(lang: &str) {
    let en = match lang {
        "en" => true,
        "tr" => false,
        _ => !system_is_turkish(),
    };
    EN.store(en, Ordering::SeqCst);
}

pub fn en() -> bool {
    EN.load(Ordering::SeqCst)
}

pub fn tr(s: &str) -> &str {
    if !en() {
        return s;
    }
    english(s).unwrap_or(s)
}

fn english(s: &str) -> Option<&'static str> {
    Some(match s {
        "Gizli simgeler" => "Hidden icons",
        "Yeni pencere" => "New window",
        "Dosya konumunu aç" => "Open file location",
        "Görev çubuğunda gösterme" => "Hide from taskbar",
        "Geri yükle" => "Restore",
        "Simge durumuna küçült" => "Minimize",
        "Ekranı kapla" => "Maximize",
        "Pencereyi kapat" => "Close window",
        "Görevi sonlandır" => "End task",
        "Tümünü küçült" => "Minimize all",
        "Bu widget'ı gizle" => "Hide this widget",
        "Görev Yöneticisi" => "Task Manager",
        "Ayarlar" => "Settings",
        "Yeni ekle" => "Add new",
        "Widget'lar" => "Widgets",
        "Tema" => "Theme",
        "Kapalı" => "Off",
        "Çubuğun yanlarında arazi" => "Ground beside the bar",
        "Evcil hayvan" => "Pet",
        "Görünüm" => "Layout",
        "Çentik (ekranın üstü)" => "Notch (top of screen)",
        "Özel görev çubuğu" => "Custom taskbar",
        "Çalan medyayı göster" => "Show playing media",
        "Widget'ları kilitle" => "Lock widgets",
        "Ayarları düzenle (config.toml)" => "Edit settings (config.toml)",
        "Yeniden yükle" => "Reload",
        "Windows ile başlat" => "Start with Windows",
        "Çıkış (Windows görev çubuğunu geri getir)" => "Exit (restore Windows taskbar)",
        "Zamanlayıcı" => "Timer",
        "Süre doldu" => "Time's up",
        "Dil / Language" => "Language / Dil",
        "Otomatik" => "Automatic",

        "Saat / takvim" => "Clock / calendar",
        "Hava durumu" => "Weather",
        "Medya" => "Media",
        "Sistem monitörü" => "System monitor",
        "Yuvarlak" => "Rounded",
        "Modüler" => "Modular",
        "Dock" => "Dock",
        "Tam genişlik" => "Full width",
        "Her zaman" => "Always",
        "Medya çalarken" => "While media plays",
        "Üzerine gelince" => "On hover",
        "Bukalemun" => "Chameleon",
        "Ateş böceği" => "Firefly",
        "Tilki" => "Fox",
        "Kirpi" => "Hedgehog",
        "Köpek" => "Puppy",
        "Pil azaldı" => "Battery low",
        "Ses" => "Sound",
        "Ekran süresi" => "Screen time",
        "Renk seç" => "Pick color",
        "Yeni masaüstü" => "New desktop",
        "Sistem sesleri" => "System sounds",
        "Uygulama sesleri" => "App volumes",
        "Ses çalan uygulama yok" => "No apps playing sound",
        "Ses çıkaran uygulamalar burada listelenir" => "Apps that play sound are listed here",
        "Henüz veri yok" => "No data yet",
        "Bugün kullandığın uygulamalar burada birikir" => "Apps you use today add up here",
        "Bugün ekran başında" => "Screen time today",
        "Renk kopyalandı" => "Color copied",
        "Son kullanılanlar" => "Recent",
        "Cihaz pili azaldı" => "Device battery low",
        "Günün özeti" => "Daily summary",
        "En çok:" => "Most:",
        "Ajanda" => "Agenda",
        "Geçmiş" => "History",
        "Odak" => "Focus",
        "Şimdi" => "Now",
        "Takvim bağlı değil" => "No calendar connected",
        "Ayarlar > Çentik'ten ICS bağlantısı ekle" => "Add an ICS link in Settings > Notch",
        "Yaklaşan etkinlik yok" => "No upcoming events",
        "Önümüzdeki 7 gün boş görünüyor" => "The next 7 days look free",
        "Bugün" => "Today",
        "Yarın" => "Tomorrow",
        "Tüm gün" => "All day",
        "GitHub kapalı" => "GitHub is off",
        "Ayarlar > Çentik'ten açabilirsin" => "Turn it on in Settings > Notch",
        "GitHub CLI bulunamadı" => "GitHub CLI not found",
        "gh kurulunca PR ve CI durumları burada görünür" => "Install gh to see PR and CI status here",
        "GitHub'a bağlanılamadı" => "Couldn't reach GitHub",
        "Terminalde 'gh auth login' çalıştır" => "Run 'gh auth login' in a terminal",
        "Yükleniyor…" => "Loading…",
        "Açık PR yok" => "No open PRs",
        "Açtığın pull request'ler burada görünür" => "Pull requests you open show up here",
        "Açık pull request'lerin" => "Your open pull requests",
        "geçti" => "passed",
        "başarısız" => "failed",
        "çalışıyor" => "running",
        "Bildirim yok" => "No notifications",
        "Gelen bildirimler burada birikir" => "Notifications collect here",
        "Odak modunda gelenler dahil" => "Including ones held during focus",
        "Son bildirimler" => "Recent notifications",
        "Uygulama ara" => "Search apps",
        "Sık kullanılanlar" => "Frequently used",
        "Uygulamalar" => "Apps",
        "Uygulamalar yükleniyor…" => "Loading apps…",
        "En iyi eşleşmeler" => "Best matches",
        "Oturumu kapat" => "Sign out",
        "Uyku" => "Sleep",
        "Yeniden başlat" => "Restart",
        "Bilgisayarı kapat" => "Shut down",
        "Sabitlemeyi kaldır" => "Unpin from taskbar",
        "Görev çubuğuna sabitle" => "Pin to taskbar",
        "Aç" => "Open",
        "Odak bitti" => "Focus ended",
        "Bildirimler yeniden açık" => "Notifications are back on",
        "CI geçti" => "CI passed",
        "CI başarısız" => "CI failed",
        "Evcil hayvanı kapat" => "Turn off the pet",

        "Gece mavisi" => "Midnight blue",
        "Aydınlık" => "Light",
        "Aurora" => "Aurora",
        "Buzlu cam" => "Frosted glass",
        "Neon" => "Neon",
        "Orman" => "Forest",
        "Fırçalı metal" => "Brushed metal",
        "Retro piksel" => "Retro pixel",
        "Pastel" => "Pastel",
        "Minimal" => "Minimal",
        "Gün batımı" => "Sunset",
        "Siyah (AMOLED)" => "Black (AMOLED)",

        "Agent" => "Agent",
        "Süre" => "Timer",
        "Pano" => "Clipboard",
        "Raf" => "Shelf",
        "Hızlı" => "Quick",
        "Onay bekliyor" => "Needs approval",
        "Çalışan agent yok" => "No agents running",
        "Claude Code çalışınca burada ne yaptığı görünür" => "When Claude Code runs, its activity shows here",
        "şimdi" => "now",
        "Çalışıyor" => "Running",
        "Duraklatıldı" => "Paused",
        "Süre seç" => "Pick a time",
        "Hazır süreler" => "Presets",
        "Duraklat" => "Pause",
        "Başlat" => "Start",
        "Sıfırla" => "Reset",
        "+1 dk" => "+1 min",
        "Temizle" => "Clear",
        "Kopyaladığın metinler burada görünür" => "Text you copy shows up here",
        "Birine tıklayınca yeniden kopyalanır" => "Click one to copy it again",
        "Son kopyalananlar" => "Recently copied",
        "Kopyalandı" => "Copied",
        "Dosyaları çentiğe sürükleyip bırak" => "Drag and drop files onto the notch",
        "Burada bekler; sonra istediğin yere sürükle" => "They wait here; drag them anywhere later",
        "Sürükleyerek istediğin yere bırak" => "Drag to drop them anywhere",
        "Koyu tema" => "Dark mode",
        "Ses kapalı" => "Muted",
        "Mikrofon kapalı" => "Mic off",
        "Mikrofon" => "Microphone",
        "Bluetooth" => "Bluetooth",
        "Wi-Fi" => "Wi-Fi",
        "Ekran alıntısı" => "Snip",
        "Kilitle" => "Lock",

        "Bildirim" => "Notification",
        "Senden yanıt bekliyor" => "Waiting for your reply",
        "Görev bitti" => "Task finished",
        "Düşünüyor" => "Thinking",
        "Araç" => "Tool",
        "Web'e bakıyor" => "Browsing the web",
        "Alt agent çalıştırıyor" => "Running a subagent",

        "Açık" => "Clear",
        "Az bulutlu" => "Mostly clear",
        "Parçalı bulutlu" => "Partly cloudy",
        "Bulutlu" => "Overcast",
        "Sisli" => "Foggy",
        "Çisenti" => "Drizzle",
        "Dondurucu çisenti" => "Freezing drizzle",
        "Yağmurlu" => "Rain",
        "Kuvvetli yağmur" => "Heavy rain",
        "Dondurucu yağmur" => "Freezing rain",
        "Karlı" => "Snow",
        "Yoğun kar" => "Heavy snow",
        "Kar taneleri" => "Snow grains",
        "Sağanak" => "Showers",
        "Kar sağanağı" => "Snow showers",
        "Gök gürültülü fırtına" => "Thunderstorm",
        "Dolu ve fırtına" => "Thunderstorm with hail",
        "Bilinmiyor" => "Unknown",

        "Hava durumu yükleniyor…" => "Loading weather…",
        "Çalan bir şey yok" => "Nothing playing",
        "Spotify, YouTube vb. başlatın" => "Start Spotify, YouTube, etc.",
        "İşlemci" => "CPU",
        "Bellek" => "Memory",
        "Sistem" => "System",
        _ => return None,
    })
}

pub const MONTHS_TR: [&str; 12] =
    ["Ocak", "Şubat", "Mart", "Nisan", "Mayıs", "Haziran", "Temmuz", "Ağustos", "Eylül", "Ekim", "Kasım", "Aralık"];
const MONTHS_EN: [&str; 12] =
    ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
pub const DAYS_TR: [&str; 7] = ["Pazar", "Pazartesi", "Salı", "Çarşamba", "Perşembe", "Cuma", "Cumartesi"];
const DAYS_EN: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
pub const DAYS_SHORT_TR: [&str; 7] = ["Pt", "Sa", "Ça", "Pe", "Cu", "Ct", "Pz"];
const DAYS_SHORT_EN: [&str; 7] = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];

pub fn month(m: u16) -> &'static str {
    let i = (m as usize).saturating_sub(1).min(11);
    if en() { MONTHS_EN[i] } else { MONTHS_TR[i] }
}

pub fn month_short(m: u16) -> String {
    month(m).chars().take(3).collect()
}

pub fn day(dow: u16) -> &'static str {
    let i = dow as usize % 7;
    if en() { DAYS_EN[i] } else { DAYS_TR[i] }
}

pub fn days_short() -> &'static [&'static str; 7] {
    if en() { &DAYS_SHORT_EN } else { &DAYS_SHORT_TR }
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_ui_string_has_english() {
        let src = [
            include_str!("app.rs"),
            include_str!("notch.rs"),
            include_str!("notify.rs"),
            include_str!("weather.rs"),
            include_str!("widgets.rs"),
            include_str!("config.rs"),
        ]
        .join("\n");
        let mut missing = Vec::new();
        for (i, _) in src.match_indices("tr(\"") {
            let rest = &src[i + 4..];
            let end = rest.find('"').unwrap();
            let s = &rest[..end];
            if super::english(s).is_none() {
                missing.push(s.to_string());
            }
        }
        assert!(missing.is_empty(), "çevirisi olmayan metinler: {missing:?}");
    }
}
