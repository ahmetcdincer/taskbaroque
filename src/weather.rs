use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use windows::core::Result;
use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};

use crate::config::WeatherCfg;
use crate::i18n::tr;
use crate::util;

#[derive(Clone, Debug, PartialEq)]
pub struct Weather {
    pub temp: f32,
    pub code: i32,
    pub wind: f32,
    pub tmax: f32,
    pub tmin: f32,
    pub is_day: bool,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Sky {
    Clear,
    PartlyCloudy,
    Cloudy,
    Fog,
    Drizzle,
    Rain,
    Snow,
    Storm,
}

impl Weather {
    pub fn sky(&self) -> Sky {
        match self.code {
            0 => Sky::Clear,
            1 | 2 => Sky::PartlyCloudy,
            3 => Sky::Cloudy,
            45 | 48 => Sky::Fog,
            51..=57 => Sky::Drizzle,
            61..=67 | 80..=82 => Sky::Rain,
            71..=77 | 85 | 86 => Sky::Snow,
            95..=99 => Sky::Storm,
            _ => Sky::Cloudy,
        }
    }

    pub fn description(&self) -> &'static str {
        match self.code {
            0 => tr("Açık"),
            1 => tr("Az bulutlu"),
            2 => tr("Parçalı bulutlu"),
            3 => if crate::i18n::en() { "Overcast" } else { "Kapalı" },
            45 | 48 => tr("Sisli"),
            51..=55 => tr("Çisenti"),
            56 | 57 => tr("Dondurucu çisenti"),
            61 | 63 => tr("Yağmurlu"),
            65 => tr("Kuvvetli yağmur"),
            66 | 67 => tr("Dondurucu yağmur"),
            71 | 73 => tr("Karlı"),
            75 => tr("Yoğun kar"),
            77 => tr("Kar taneleri"),
            80..=82 => tr("Sağanak"),
            85 | 86 => tr("Kar sağanağı"),
            95 => tr("Gök gürültülü fırtına"),
            96..=99 => tr("Dolu ve fırtına"),
            _ => tr("Bilinmiyor"),
        }
    }
}

pub fn spawn(state: Arc<Mutex<Option<Weather>>>, cfg: WeatherCfg, notify: isize) -> Sender<WeatherCfg> {
    let (tx, rx) = channel::<WeatherCfg>();
    std::thread::spawn(move || {
        unsafe {
            let _ = RoInitialize(RO_INIT_MULTITHREADED);
        }
        let mut cfg = cfg;
        loop {
            let wait = match fetch(&cfg) {
                Ok(w) => {
                    *state.lock().unwrap() = Some(w);
                    util::post_data_changed(notify);
                    Duration::from_secs(cfg.refresh_minutes.max(5) * 60)
                }
                Err(_) => Duration::from_secs(120),
            };
            match rx.recv_timeout(wait) {
                Ok(c) => cfg = c,
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
    });
    tx
}

fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") })
        .collect()
}

pub fn find_place(name: &str) -> Option<(String, f64, f64)> {
    let url = format!(
        "https://geocoding-api.open-meteo.com/v1/search?name={}&count=1&language={}&format=json",
        encode(name.trim()),
        if crate::i18n::en() { "en" } else { "tr" }
    );
    let v: serde_json::Value = serde_json::from_str(&crate::http::get(&url)?).ok()?;
    let r = &v["results"][0];
    Some((r["name"].as_str()?.to_string(), r["latitude"].as_f64()?, r["longitude"].as_f64()?))
}

fn fetch(cfg: &WeatherCfg) -> Result<Weather> {
    let url = format!(
        "https://api.open-meteo.com/v1/forecast?latitude={}&longitude={}\
         &current=temperature_2m,weather_code,wind_speed_10m,is_day\
         &daily=temperature_2m_max,temperature_2m_min&timezone=auto&forecast_days=1",
        cfg.latitude, cfg.longitude
    );
    let bad = || windows::core::Error::from(windows::Win32::Foundation::E_FAIL);
    let body = crate::http::get(&url).ok_or_else(bad)?;
    let v: serde_json::Value = serde_json::from_str(&body).map_err(|_| bad())?;
    let cur = &v["current"];
    let num = |x: &serde_json::Value| x.as_f64().map(|f| f as f32);
    Ok(Weather {
        temp: num(&cur["temperature_2m"]).ok_or_else(bad)?,
        code: cur["weather_code"].as_i64().unwrap_or(3) as i32,
        wind: num(&cur["wind_speed_10m"]).unwrap_or(0.0),
        is_day: cur["is_day"].as_i64().unwrap_or(1) == 1,
        tmax: num(&v["daily"]["temperature_2m_max"][0]).unwrap_or(f32::NAN),
        tmin: num(&v["daily"]["temperature_2m_min"][0]).unwrap_or(f32::NAN),
    })
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore = "ağ gerektirir"]
    fn fetch_live() {
        unsafe {
            let _ = super::RoInitialize(super::RO_INIT_MULTITHREADED);
        }
        let w = super::fetch(&super::WeatherCfg::default());
        println!("{w:?}");
        assert!(w.is_ok());
    }

    #[test]
    fn encodes_query() {
        assert_eq!(super::encode("Muğla Bodrum"), "Mu%C4%9Fla%20Bodrum");
    }

    #[test]
    #[ignore = "ağ gerektirir"]
    fn find_place_live() {
        let p = super::find_place("Antalya");
        println!("{p:?}");
        assert!(p.is_some_and(|(n, lat, _)| n == "Antalya" && (lat - 36.9).abs() < 0.2));
    }
}
