use std::sync::{Arc, Mutex};
use std::time::Duration;

use windows::core::GUID;
use windows::Devices::Bluetooth::GenericAttributeProfile::GattCommunicationStatus;
use windows::Devices::Bluetooth::{BluetoothCacheMode, BluetoothConnectionStatus, BluetoothLEDevice, BluetoothUuidHelper};
use windows::Devices::Enumeration::DeviceInformation;
use windows::Storage::Streams::DataReader;
use windows::Win32::Devices::DeviceAndDriverInstallation::*;
use windows::Win32::Devices::Properties::*;
use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};

use crate::util;

#[derive(Clone, Debug, PartialEq)]
pub struct BtDevice {
    pub name: String,
    pub percent: u8,
}

const BATTERY_KEY: DEVPROPKEY = DEVPROPKEY { fmtid: GUID::from_u128(0x104ea319_6ee2_4701_bd47_8ddbf425bbe5), pid: 2 };

fn clean(name: &str) -> String {
    let mut n = name.trim().to_string();
    for s in [" Hands-Free AG Audio", " Hands-Free AG", " Hands-Free", " Stereo", " Avrcp Transport", " Audio"] {
        if let Some(x) = n.strip_suffix(s) {
            n = x.trim().to_string();
        }
    }
    n
}

fn classic() -> Vec<BtDevice> {
    let mut out = Vec::new();
    unsafe {
        let Ok(set) = SetupDiGetClassDevsW(None, windows::core::PCWSTR::null(), None, DIGCF_PRESENT | DIGCF_ALLCLASSES) else { return out };
        let mut i = 0;
        loop {
            let mut info = SP_DEVINFO_DATA { cbSize: std::mem::size_of::<SP_DEVINFO_DATA>() as u32, ..Default::default() };
            if SetupDiEnumDeviceInfo(set, i, &mut info).is_err() {
                break;
            }
            i += 1;
            let mut ty = DEVPROPTYPE::default();
            let mut b = [0u8; 4];
            if SetupDiGetDevicePropertyW(set, &info, &BATTERY_KEY, &mut ty, Some(&mut b), None, 0).is_err() {
                continue;
            }
            let mut name = [0u8; 512];
            let mut n = 0u32;
            if SetupDiGetDevicePropertyW(set, &info, &DEVPKEY_Device_FriendlyName, &mut ty, Some(&mut name), Some(&mut n), 0).is_err() {
                continue;
            }
            let wide: Vec<u16> = name[..n as usize].chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).take_while(|c| *c != 0).collect();
            let name = clean(&String::from_utf16_lossy(&wide));
            if !name.is_empty() && b[0] <= 100 {
                out.push(BtDevice { name, percent: b[0] });
            }
        }
        let _ = SetupDiDestroyDeviceInfoList(set);
    }
    out
}

fn ble() -> windows::core::Result<Vec<BtDevice>> {
    let mut out = Vec::new();
    let sel = BluetoothLEDevice::GetDeviceSelectorFromConnectionStatus(BluetoothConnectionStatus::Connected)?;
    let list = DeviceInformation::FindAllAsyncAqsFilter(&sel)?.get()?;
    let service = BluetoothUuidHelper::FromShortId(0x180F)?;
    let level = BluetoothUuidHelper::FromShortId(0x2A19)?;
    for info in list {
        let Ok(device) = BluetoothLEDevice::FromIdAsync(&info.Id()?).and_then(|op| op.get()) else { continue };
        let name = device.Name().map(|n| n.to_string()).unwrap_or_default();
        let read = (|| -> windows::core::Result<Option<u8>> {
            let svcs = device.GetGattServicesForUuidWithCacheModeAsync(service, BluetoothCacheMode::Cached)?.get()?;
            if svcs.Status()? != GattCommunicationStatus::Success {
                return Ok(None);
            }
            let Ok(svc) = svcs.Services()?.GetAt(0) else { return Ok(None) };
            let res = svc.GetCharacteristicsForUuidWithCacheModeAsync(level, BluetoothCacheMode::Cached)?.get()?;
            let ch = res.Characteristics()?.GetAt(0);
            let v = match (res.Status()?, ch) {
                (GattCommunicationStatus::Success, Ok(ch)) => ch.ReadValueWithCacheModeAsync(BluetoothCacheMode::Cached)?.get()?,
                _ => return Ok(None),
            };
            let _ = svc.Close();
            if v.Status()? != GattCommunicationStatus::Success {
                return Ok(None);
            }
            let r = DataReader::FromBuffer(&v.Value()?)?;
            Ok(Some(r.ReadByte()?))
        })();
        let _ = device.Close();
        if let Ok(Some(p)) = read {
            if p <= 100 && !name.is_empty() {
                out.push(BtDevice { name: clean(&name), percent: p });
            }
        }
    }
    Ok(out)
}

pub fn spawn(state: Arc<Mutex<Vec<BtDevice>>>, notify: isize) {
    std::thread::spawn(move || {
        unsafe {
            let _ = RoInitialize(RO_INIT_MULTITHREADED);
        }
        loop {
            let mut all = classic();
            for d in ble().unwrap_or_default() {
                if !all.iter().any(|a| a.name.eq_ignore_ascii_case(&d.name)) {
                    all.push(d);
                }
            }
            let mut merged: Vec<BtDevice> = Vec::new();
            for d in all {
                match merged.iter_mut().find(|m| m.name.eq_ignore_ascii_case(&d.name)) {
                    Some(m) => m.percent = m.percent.min(d.percent),
                    None => merged.push(d),
                }
            }
            merged.sort_by(|a, b| a.name.cmp(&b.name));
            let changed = {
                let mut s = state.lock().unwrap();
                let c = *s != merged;
                *s = merged;
                c
            };
            if changed {
                util::post_data_changed(notify);
            }
            std::thread::sleep(Duration::from_secs(90));
        }
    });
}
