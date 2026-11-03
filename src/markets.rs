//! Piyasalar widget'ı için döviz, altın ve kripto fiyatları (anahtar gerektirmeyen ücretsiz kaynaklar).
//!
//! - Kripto ve altın: CoinGecko `simple/price` (altın = PAX Gold, 1 token ≈ 1 ons altın → gram)
//! - Döviz: Frankfurter (Avrupa Merkez Bankası referans kurları, iş günlerinde güncellenir)

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::i18n;

const TROY_OUNCE_G: f64 = 31.1034768;

#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    Fiat(String),
    Crypto { id: &'static str, symbol: &'static str, name: &'static str },
    Gold,
}

const CRYPTO: [(&str, &str, &str); 12] = [
    ("BTC", "bitcoin", "Bitcoin"),
    ("ETH", "ethereum", "Ethereum"),
    ("SOL", "solana", "Solana"),
    ("BNB", "binancecoin", "BNB"),
    ("XRP", "ripple", "XRP"),
    ("DOGE", "dogecoin", "Dogecoin"),
    ("ADA", "cardano", "Cardano"),
    ("AVAX", "avalanche-2", "Avalanche"),
    ("TON", "the-open-network", "Toncoin"),
    ("TRX", "tron", "TRON"),
    ("DOT", "polkadot", "Polkadot"),
    ("USDT", "tether", "Tether"),
];

pub const DEFAULT_ITEMS: [&str; 5] = ["USD", "EUR", "GOLD", "BTC", "ETH"];

pub fn parse(s: &str) -> Option<Item> {
    let u = s.trim().to_uppercase();
    if matches!(u.as_str(), "GOLD" | "ALTIN" | "XAU") {
        return Some(Item::Gold);
    }
    if let Some((sym, id, name)) = CRYPTO.iter().find(|(sym, _, _)| *sym == u) {
        return Some(Item::Crypto { id, symbol: sym, name });
    }
    (u.len() == 3 && u.chars().all(|c| c.is_ascii_alphabetic()) && u != "TRY").then_some(Item::Fiat(u))
}

#[derive(Clone, Debug, PartialEq)]
pub struct Quote {
    pub label: String,
    pub code: String,
    /// Biçimlendirilmiş fiyat, ör. "34,21 ₺" ya da "$67.430"
    pub price: String,
    /// Yüzde değişim (kripto/altın: 24 saat, döviz: önceki iş günü)
    pub change: Option<f64>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Data {
    pub quotes: Vec<Quote>,
    /// Son başarılı güncelleme "SS:DD"
    pub updated: Option<String>,
    pub error: bool,
}

fn fiat_name(code: &str) -> String {
    let tr = !i18n::en();
    match (code, tr) {
        ("USD", true) => "Dolar".into(),
        ("EUR", true) => "Euro".into(),
        ("GBP", true) => "Sterlin".into(),
        ("CHF", true) => "İsviçre frangı".into(),
        ("JPY", true) => "Japon yeni".into(),
        ("USD", false) => "US dollar".into(),
        ("EUR", false) => "Euro".into(),
        ("GBP", false) => "British pound".into(),
        _ => code.to_string(),
    }
}

/// Türkçe sayı biçimi: binlik nokta, ondalık virgül.
pub fn fmt_num(v: f64, decimals: usize) -> String {
    let s = format!("{:.*}", decimals, v.abs());
    let (int, frac) = s.split_once('.').map_or((s.as_str(), ""), |(a, b)| (a, b));
    let mut out = String::new();
    for (i, ch) in int.chars().enumerate() {
        if i > 0 && (int.len() - i) % 3 == 0 {
            out.push('.');
        }
        out.push(ch);
    }
    if !frac.is_empty() {
        out.push(',');
        out.push_str(frac);
    }
    if v < 0.0 {
        out.insert(0, '-');
    }
    out
}

fn decimals_for(v: f64) -> usize {
    if v >= 1000.0 {
        0
    } else if v >= 1.0 {
        2
    } else {
        4
    }
}

fn get_json(url: &str) -> Option<serde_json::Value> {
    serde_json::from_str(&crate::http::get(url)?).ok()
}

fn date_days_ago(days: i64) -> String {
    let today = crate::feeds::local_now().div_euclid(86400);
    let (y, m, d) = crate::feeds::civil_from_days(today - days);
    format!("{y:04}-{m:02}-{d:02}")
}

/// (son kur, önceki iş gününe göre % değişim)
fn fiat_rate(code: &str) -> Option<(f64, Option<f64>)> {
    let url = format!("https://api.frankfurter.app/{}..?from={code}&to=TRY", date_days_ago(8));
    let v = get_json(&url)?;
    let rates = v["rates"].as_object()?;
    let mut days: Vec<(&String, f64)> = rates.iter().filter_map(|(d, r)| Some((d, r["TRY"].as_f64()?))).collect();
    days.sort_by(|a, b| a.0.cmp(b.0));
    let last = days.last()?.1;
    let change = days.len().checked_sub(2).map(|i| days[i].1).filter(|p| *p > 0.0).map(|p| (last - p) / p * 100.0);
    Some((last, change))
}

fn fetch(items: &[Item]) -> Option<Vec<Quote>> {
    let mut ids: Vec<&str> = items
        .iter()
        .filter_map(|i| match i {
            Item::Crypto { id, .. } => Some(*id),
            Item::Gold => Some("pax-gold"),
            Item::Fiat(_) => None,
        })
        .collect();
    let need_fiat = items.iter().any(|i| matches!(i, Item::Fiat(_)));
    if need_fiat {
        ids.push("tether"); // Frankfurter çalışmazsa dolar kuru için yedek
    }
    ids.sort();
    ids.dedup();
    let cg = if ids.is_empty() {
        None
    } else {
        get_json(&format!(
            "https://api.coingecko.com/api/v3/simple/price?ids={}&vs_currencies=usd,try&include_24hr_change=true",
            ids.join(",")
        ))
    };

    let mut out = Vec::new();
    let mut ok = false;
    for it in items {
        let q = match it {
            Item::Crypto { id, symbol, name } => cg.as_ref().and_then(|v| {
                let p = v[*id]["usd"].as_f64()?;
                Some(Quote {
                    label: name.to_string(),
                    code: symbol.to_string(),
                    price: format!("${}", fmt_num(p, decimals_for(p))),
                    change: v[*id]["usd_24h_change"].as_f64(),
                })
            }),
            Item::Gold => cg.as_ref().and_then(|v| {
                let p = v["pax-gold"]["try"].as_f64()? / TROY_OUNCE_G;
                Some(Quote {
                    label: if i18n::en() { "Gold (gram)".into() } else { "Gram altın".into() },
                    code: "XAU".into(),
                    price: format!("≈{} ₺", fmt_num(p, 0)),
                    change: v["pax-gold"]["try_24h_change"].as_f64(),
                })
            }),
            Item::Fiat(code) => fiat_rate(code)
                .or_else(|| {
                    let t = cg.as_ref()?["tether"]["try"].as_f64()?;
                    (code == "USD").then_some((t, cg.as_ref()?["tether"]["try_24h_change"].as_f64()))
                })
                .map(|(r, change)| Quote { label: fiat_name(code), code: code.clone(), price: format!("{} ₺", fmt_num(r, 2)), change }),
        };
        if let Some(q) = q {
            ok = true;
            out.push(q);
        }
    }
    ok.then_some(out)
}

/// Arka planda her 5 dakikada bir günceller; `stop` true olunca biter.
pub fn spawn(items: Vec<Item>, state: Arc<Mutex<Data>>, stop: Arc<AtomicBool>, notify: isize) {
    std::thread::spawn(move || loop {
        let res = fetch(&items);
        {
            let mut d = state.lock().unwrap();
            match res {
                Some(q) => {
                    let t = crate::util::now();
                    *d = Data { quotes: q, updated: Some(format!("{:02}:{:02}", t.hour, t.min)), error: false };
                }
                None => d.error = true,
            }
        }
        crate::util::post_data_changed(notify);
        for _ in 0..300 {
            if stop.load(Ordering::SeqCst) {
                return;
            }
            std::thread::sleep(Duration::from_secs(1));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn number_format() {
        assert_eq!(fmt_num(34.2105, 2), "34,21");
        assert_eq!(fmt_num(67430.4, 0), "67.430");
        assert_eq!(fmt_num(1234567.891, 2), "1.234.567,89");
        assert_eq!(fmt_num(-5.5, 1), "-5,5");
    }

    #[test]
    fn parse_items() {
        assert_eq!(parse("usd"), Some(Item::Fiat("USD".into())));
        assert_eq!(parse("altın"), Some(Item::Gold));
        assert!(matches!(parse("BTC"), Some(Item::Crypto { id: "bitcoin", .. })));
        assert_eq!(parse("TRY"), None);
        assert_eq!(parse("hello"), None);
    }
}
