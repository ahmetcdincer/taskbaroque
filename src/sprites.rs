use std::collections::HashMap;

use serde::Deserialize;

#[derive(Deserialize, Clone, Copy, Debug)]
pub struct Anim {
    pub row: u32,
    pub frames: u32,
}

#[derive(Deserialize, Clone, Debug)]
pub struct PetSheet {
    pub cell: f32,
    pub base: f32,
    pub anims: HashMap<String, Anim>,
    #[serde(default)]
    pub stand: Option<Stand>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Stand {
    pub anim: String,
    pub frame: u32,
}

/// Pixel art sürüm (varsa). Kediler ve kurt köpeği yalnızca pixel olarak var.
fn pet_pixel(kind: &str) -> Option<(&'static [u8], &'static str)> {
    Some(match kind {
        "chameleon" => (include_bytes!("../assets/pets/pixel/chameleon.png"), include_str!("../assets/pets/pixel/chameleon.json")),
        "firefly" => (include_bytes!("../assets/pets/pixel/firefly.png"), include_str!("../assets/pets/pixel/firefly.json")),
        "fox" => (include_bytes!("../assets/pets/pixel/fox.png"), include_str!("../assets/pets/pixel/fox.json")),
        "hedgehog" => (include_bytes!("../assets/pets/pixel/hedgehog.png"), include_str!("../assets/pets/pixel/hedgehog.json")),
        "puppy" => (include_bytes!("../assets/pets/pixel/puppy.png"), include_str!("../assets/pets/pixel/puppy.json")),
        "cat_orange" => (include_bytes!("../assets/pets/pixel/cat_orange.png"), include_str!("../assets/pets/pixel/cat_orange.json")),
        "cat_black" => (include_bytes!("../assets/pets/pixel/cat_black.png"), include_str!("../assets/pets/pixel/cat_black.json")),
        "cat_grey" => (include_bytes!("../assets/pets/pixel/cat_grey.png"), include_str!("../assets/pets/pixel/cat_grey.json")),
        "wolfdog" => (include_bytes!("../assets/pets/pixel/wolfdog.png"), include_str!("../assets/pets/pixel/wolfdog.json")),
        _ => return None,
    })
}

pub fn pet(kind: &str, pixel: bool) -> Option<(&'static [u8], PetSheet)> {
    if let Some((png, json)) = pet_pixel(kind).filter(|_| pixel || classic(kind).is_none()) {
        return Some((png, serde_json::from_str(json).ok()?));
    }
    let (png, json) = classic(kind)?;
    Some((png, serde_json::from_str(json).ok()?))
}

fn classic(kind: &str) -> Option<(&'static [u8], &'static str)> {
    let (png, json): (&'static [u8], &'static str) = match kind {
        "chameleon" => (include_bytes!("../assets/pets/chameleon.png"), include_str!("../assets/pets/chameleon.json")),
        "firefly" => (include_bytes!("../assets/pets/firefly.png"), include_str!("../assets/pets/firefly.json")),
        "fox" => (include_bytes!("../assets/pets/fox.png"), include_str!("../assets/pets/fox.json")),
        "hedgehog" => (include_bytes!("../assets/pets/hedgehog.png"), include_str!("../assets/pets/hedgehog.json")),
        "puppy" => (include_bytes!("../assets/pets/puppy.png"), include_str!("../assets/pets/puppy.json")),
        _ => return None,
    };
    Some((png, json))
}

#[derive(Deserialize, Clone, Copy, Debug)]
pub struct Part {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Deserialize, Clone, Debug)]
pub struct GroundMeta {
    pub ground_end_caps: Vec<Part>,
    pub props: Vec<Part>,
    pub night_props: Vec<Part>,
}

pub const GROUND_STRIP: &[u8] = include_bytes!("../assets/ground/ground_strip.png");
pub const GROUND_SNOW: &[u8] = include_bytes!("../assets/ground/ground_snow.png");
pub const GROUND_CAPS: &[u8] = include_bytes!("../assets/ground/ground_end_caps.png");
pub const GROUND_PROPS: &[u8] = include_bytes!("../assets/ground/props.png");
pub const GROUND_NIGHT: &[u8] = include_bytes!("../assets/ground/night_props.png");

pub fn ground_meta() -> Option<GroundMeta> {
    serde_json::from_str(include_str!("../assets/ground/ground.json")).ok()
}

#[derive(Deserialize, Clone, Copy, Debug)]
pub struct Size {
    pub w: f32,
    pub h: f32,
}

#[derive(Deserialize, Clone, Copy, Debug)]
pub struct Lines {
    pub back: f32,
    pub road: f32,
    pub walk: f32,
}

#[derive(Deserialize, Clone, Copy, Debug)]
pub struct TunnelMeta {
    pub piece: Part,
    pub open: [f32; 2],
    pub open_y: [f32; 2],
}

#[derive(Deserialize, Clone, Debug)]
pub struct RoadMeta {
    pub zoom: f32,
    pub strip_day: Size,
    pub lines: Lines,
    pub tunnels: HashMap<String, TunnelMeta>,
    pub props: HashMap<String, Part>,
    pub car: HashMap<String, Vec<Part>>,
    pub car_body_w: f32,
}

pub const ROAD_DAY: &[u8] = include_bytes!("../assets/road/strip_day.png");
pub const ROAD_NIGHT: &[u8] = include_bytes!("../assets/road/strip_night.png");
pub const ROAD_SNOW: &[u8] = include_bytes!("../assets/road/strip_snow.png");
pub const ROAD_TUNNELS: &[u8] = include_bytes!("../assets/road/tunnels.png");
pub const ROAD_PROPS: &[u8] = include_bytes!("../assets/road/props.png");
pub const ROAD_CAR: &[u8] = include_bytes!("../assets/road/car.png");

pub fn road_meta() -> Option<RoadMeta> {
    serde_json::from_str(include_str!("../assets/road/road.json")).ok()
}

pub const PROP_PUDDLE: usize = 9;
pub const PROP_SIGN: usize = 10;
pub const NIGHT_FIRST_USABLE: usize = 3;

#[cfg(test)]
mod tests {
    #[test]
    fn embedded_assets_parse() {
        for pixel in [false, true] {
            for (k, _) in crate::config::PET_KINDS {
                let (png, sheet) = super::pet(k, pixel).unwrap();
                assert!(png.len() > 1000);
                for (a, n) in [("idle", 4), ("walk", 8), ("run", 6), ("sleep", 4), ("jump", 4), ("happy", 4), ("dangle", 4), ("special", 6)] {
                    assert_eq!(sheet.anims.get(a).map(|x| x.frames), Some(n), "{k} {a}");
                }
            }
        }
        let g = super::ground_meta().unwrap();
        assert_eq!(g.props.len(), 11);
        assert_eq!(g.ground_end_caps.len(), 2);
        let r = super::road_meta().unwrap();
        assert_eq!(r.tunnels.len(), 3);
        assert_eq!(r.props.len(), 12);
        assert_eq!(r.car["green"].len(), 4);
    }
}
