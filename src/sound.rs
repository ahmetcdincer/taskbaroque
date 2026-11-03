//! Bildirim sesleri. Varsayılan temalar uygulamanın kendi ürettiği kısa melodilerdir; Windows ses
//! şemasına bağlı kalmaz (şemada ses tanımlı değilse hiç çalmazdı). Her durum ayrı tınlar:
//! bitti, dikkat (onay/yanıt bekliyor), hata. Ayrıca Windows sistem sesleri ya da kullanıcının
//! seçtiği bir .wav dosyası çalınabilir.

use std::collections::HashMap;
use std::sync::Mutex;

use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Media::Audio::{PlaySoundW, SND_ALIAS, SND_ASYNC, SND_FILENAME, SND_MEMORY, SND_NODEFAULT};

use crate::config::SoundCfg;

const RATE: u32 = 44_100;

/// (ayar anahtarı, görünen ad)
pub const THEMES: [(&str, &str); 6] = [
    ("chime", "Melodi"),
    ("soft", "Yumuşak"),
    ("bell", "Zil"),
    ("retro", "Retro (8-bit)"),
    ("windows", "Windows sesleri"),
    ("custom", "Kendi ses dosyam"),
];

#[derive(Clone, Copy)]
enum Wave {
    /// Zil benzeri: temel ton + üst harmonikler
    Chime,
    /// Saf sinüs, yavaş sönüm
    Soft,
    /// Çan: uyumsuz kısmi tonlar, uzun sönüm
    Bell,
    /// Kare dalga, kısa: eski oyun konsolu
    Square,
}

/// (frekans Hz, başlangıç sn, süre sn)
fn render(notes: &[(f32, f32, f32)], wave: Wave, volume: f32) -> Vec<u8> {
    let total = notes.iter().map(|n| n.1 + n.2).fold(0.0f32, f32::max) + 0.05;
    let n = (total * RATE as f32) as usize;
    let mut buf = vec![0f32; n];
    let decay = match wave {
        Wave::Chime => 7.5,
        Wave::Soft => 4.5,
        Wave::Bell => 3.2,
        Wave::Square => 12.0,
    };
    for &(f, start, dur) in notes {
        let s0 = (start * RATE as f32) as usize;
        let len = (dur * RATE as f32) as usize;
        for i in 0..len {
            let Some(slot) = buf.get_mut(s0 + i) else { break };
            let t = i as f32 / RATE as f32;
            let attack = (t / if matches!(wave, Wave::Soft) { 0.02 } else { 0.004 }).min(1.0);
            let env = attack * (-t * decay).exp();
            let w = std::f32::consts::TAU * f * t;
            let v = match wave {
                Wave::Chime => w.sin() + 0.28 * (2.0 * w).sin() + 0.08 * (3.0 * w).sin(),
                Wave::Soft => w.sin(),
                Wave::Bell => w.sin() + 0.5 * (2.76 * w).sin() + 0.25 * (5.4 * w).sin() + 0.12 * (8.93 * w).sin(),
                Wave::Square => {
                    if w.sin() >= 0.0 {
                        0.6
                    } else {
                        -0.6
                    }
                }
            };
            *slot += env * v;
        }
    }
    let peak = buf.iter().fold(0.0f32, |m, v| m.max(v.abs())).max(1e-6);
    let k = volume.clamp(0.0, 1.0) * 0.9 / peak;
    let data_len = (n * 2) as u32;
    let mut out = Vec::with_capacity(44 + n * 2);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&RATE.to_le_bytes());
    out.extend_from_slice(&(RATE * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for v in buf {
        out.extend_from_slice(&((v * k * i16::MAX as f32) as i16).to_le_bytes());
    }
    out
}

fn melody(theme: &str, kind: &str) -> (Vec<(f32, f32, f32)>, Wave) {
    match (theme, kind) {
        // Melodi: Do-Mi-Sol yükselen / iki kısa Re / Sol-Re alçalan
        ("chime", "done") => (vec![(1046.5, 0.0, 0.5), (1318.5, 0.11, 0.5), (1568.0, 0.22, 0.7)], Wave::Chime),
        ("chime", "attention") => (vec![(1174.7, 0.0, 0.3), (1174.7, 0.17, 0.45)], Wave::Chime),
        ("chime", "error") => (vec![(784.0, 0.0, 0.4), (587.3, 0.16, 0.7)], Wave::Chime),
        ("chime", _) => (vec![(1046.5, 0.0, 0.6)], Wave::Chime),
        // Yumuşak: bir oktav aşağıda, saf ve sakin
        ("soft", "done") => (vec![(523.3, 0.0, 0.7), (659.3, 0.16, 0.7), (784.0, 0.32, 0.9)], Wave::Soft),
        ("soft", "attention") => (vec![(587.3, 0.0, 0.5), (740.0, 0.22, 0.7)], Wave::Soft),
        ("soft", "error") => (vec![(440.0, 0.0, 0.6), (349.2, 0.22, 0.9)], Wave::Soft),
        ("soft", _) => (vec![(523.3, 0.0, 0.8)], Wave::Soft),
        // Zil: çan tınısı
        ("bell", "done") => (vec![(880.0, 0.0, 1.4)], Wave::Bell),
        ("bell", "attention") => (vec![(988.0, 0.0, 0.7), (988.0, 0.3, 1.0)], Wave::Bell),
        ("bell", "error") => (vec![(440.0, 0.0, 1.6)], Wave::Bell),
        ("bell", _) => (vec![(880.0, 0.0, 1.0)], Wave::Bell),
        // Retro: hızlı arpej ve bip'ler
        ("retro", "done") => (vec![(1046.5, 0.0, 0.09), (1318.5, 0.08, 0.09), (1568.0, 0.16, 0.09), (2093.0, 0.24, 0.22)], Wave::Square),
        ("retro", "attention") => (vec![(1568.0, 0.0, 0.08), (1568.0, 0.14, 0.12)], Wave::Square),
        ("retro", "error") => (vec![(392.0, 0.0, 0.12), (311.1, 0.12, 0.12), (261.6, 0.24, 0.3)], Wave::Square),
        ("retro", _) => (vec![(1318.5, 0.0, 0.12)], Wave::Square),
        _ => melody("chime", kind),
    }
}

/// Üretilen sesler; PlaySound eşzamansız çalarken bellek geçerli kalsın diye hiç silinmez.
static CACHE: Mutex<Option<HashMap<(String, String, u32), &'static [u8]>>> = Mutex::new(None);

fn clip(theme: &str, kind: &str, volume: f32) -> &'static [u8] {
    let vol = (volume.clamp(0.0, 1.0) * 100.0).round() as u32;
    let key = (theme.to_string(), kind.to_string(), vol);
    let mut cache = CACHE.lock().unwrap();
    let map = cache.get_or_insert_with(HashMap::new);
    if let Some(b) = map.get(&key) {
        return b;
    }
    let (notes, wave) = melody(theme, kind);
    let wav: &'static [u8] = Box::leak(render(&notes, wave, vol as f32 / 100.0).into_boxed_slice());
    map.insert(key, wav);
    wav
}

/// Bu tür bildirimde ses çalınsın mı?
fn wanted(cfg: &SoundCfg, kind: &str) -> bool {
    cfg.enabled
        && match kind {
            "done" => cfg.done,
            "attention" => cfg.attention,
            "error" => cfg.error,
            _ => true,
        }
}

/// Bildirim sesi çal (ayarlara göre; kapalıysa hiçbir şey yapmaz).
pub fn notify(cfg: &SoundCfg, kind: &str) {
    if wanted(cfg, kind) {
        play(cfg, kind);
    }
}

/// Ayarlardan bağımsız çal (Ayarlar'daki "Dinle" düğmesi için).
pub fn play(cfg: &SoundCfg, kind: &str) {
    unsafe {
        match cfg.theme.as_str() {
            "windows" => {
                let alias = match kind {
                    "done" => "SystemAsterisk",
                    "attention" => "SystemExclamation",
                    "error" => "SystemHand",
                    _ => "SystemNotification",
                };
                let _ = PlaySoundW(&HSTRING::from(alias), None, SND_ALIAS | SND_ASYNC | SND_NODEFAULT);
            }
            "custom" if std::path::Path::new(&cfg.file).is_file() => {
                let _ = PlaySoundW(&HSTRING::from(cfg.file.as_str()), None, SND_FILENAME | SND_ASYNC | SND_NODEFAULT);
            }
            theme => {
                let theme = if theme == "custom" { "chime" } else { theme };
                let wav = clip(theme, kind, cfg.volume);
                let _ = PlaySoundW(PCWSTR(wav.as_ptr() as *const u16), None, SND_MEMORY | SND_ASYNC | SND_NODEFAULT);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn wav_header() {
        for wave in [super::Wave::Chime, super::Wave::Soft, super::Wave::Bell, super::Wave::Square] {
            let w = super::render(&[(440.0, 0.0, 0.1)], wave, 0.5);
            assert_eq!(&w[0..4], b"RIFF");
            assert_eq!(&w[8..16], b"WAVEfmt ");
            let data = u32::from_le_bytes([w[40], w[41], w[42], w[43]]) as usize;
            assert_eq!(w.len(), 44 + data);
        }
        // Önbellek aynı ayara aynı belleği döndürür
        let a = super::clip("retro", "done", 0.7).as_ptr();
        assert_eq!(a, super::clip("retro", "done", 0.7).as_ptr());
    }
}
