use std::collections::VecDeque;
use std::time::Instant;

use windows::core::w;
use windows::Win32::Foundation::FILETIME;
use windows::Win32::NetworkManagement::IpHelper::{FreeMibTable, GetIfTable2, MIB_IF_TABLE2};
use windows::Win32::NetworkManagement::Ndis::IfOperStatusUp;
use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows::Win32::System::Threading::GetSystemTimes;

pub const HISTORY: usize = 60;

#[derive(Default)]
pub struct SysInfo {
    pub cpu: f32,
    pub ram: f32,
    pub ram_used_gb: f32,
    pub ram_total_gb: f32,
    pub disk: f32,
    pub disk_free_gb: f32,
    pub net_down: f64,
    pub net_up: f64,
    pub cpu_hist: VecDeque<f32>,
    pub ram_hist: VecDeque<f32>,
    last_cpu: Option<(u64, u64)>,
    last_net: Option<(u64, u64, Instant)>,
}

fn ft(f: FILETIME) -> u64 {
    ((f.dwHighDateTime as u64) << 32) | f.dwLowDateTime as u64
}

impl SysInfo {
    pub fn update(&mut self, with_net: bool) {
        unsafe {
            let (mut idle, mut kernel, mut user) = (FILETIME::default(), FILETIME::default(), FILETIME::default());
            if GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user)).is_ok() {
                let total = ft(kernel) + ft(user);
                let idle = ft(idle);
                if let Some((lt, li)) = self.last_cpu {
                    let dt = total.saturating_sub(lt);
                    if dt > 0 {
                        self.cpu = (1.0 - idle.saturating_sub(li) as f32 / dt as f32).clamp(0.0, 1.0) * 100.0;
                    }
                }
                self.last_cpu = Some((total, idle));
            }

            let mut mem = MEMORYSTATUSEX { dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
            if GlobalMemoryStatusEx(&mut mem).is_ok() {
                let gb = 1024.0 * 1024.0 * 1024.0;
                self.ram_total_gb = (mem.ullTotalPhys as f64 / gb) as f32;
                self.ram_used_gb = ((mem.ullTotalPhys - mem.ullAvailPhys) as f64 / gb) as f32;
                self.ram = mem.dwMemoryLoad as f32;
            }

            let (mut avail, mut total, mut free) = (0u64, 0u64, 0u64);
            if GetDiskFreeSpaceExW(w!("C:\\"), Some(&mut avail), Some(&mut total), Some(&mut free)).is_ok() && total > 0 {
                self.disk = (1.0 - free as f64 / total as f64) as f32 * 100.0;
                self.disk_free_gb = (avail as f64 / (1024.0 * 1024.0 * 1024.0)) as f32;
            }
        }

        if with_net {
            self.update_net();
        }

        for (hist, v) in [(&mut self.cpu_hist, self.cpu), (&mut self.ram_hist, self.ram)] {
            if hist.len() >= HISTORY {
                hist.pop_front();
            }
            hist.push_back(v);
        }
    }

    fn update_net(&mut self) {
        let (mut rx, mut tx) = (0u64, 0u64);
        unsafe {
            let mut table: *mut MIB_IF_TABLE2 = std::ptr::null_mut();
            if GetIfTable2(&mut table).is_err() || table.is_null() {
                return;
            }
            let n = (*table).NumEntries as usize;
            let rows = std::slice::from_raw_parts((*table).Table.as_ptr(), n);
            for r in rows {
                let hardware = r.InterfaceAndOperStatusFlags._bitfield & 1 != 0;
                if hardware && r.OperStatus == IfOperStatusUp {
                    rx += r.InOctets;
                    tx += r.OutOctets;
                }
            }
            FreeMibTable(table as *const _);
        }
        let now = Instant::now();
        if let Some((lrx, ltx, lt)) = self.last_net {
            let dt = now.duration_since(lt).as_secs_f64().max(0.001);
            self.net_down = rx.saturating_sub(lrx) as f64 / dt;
            self.net_up = tx.saturating_sub(ltx) as f64 / dt;
        }
        self.last_net = Some((rx, tx, now));
    }
}

pub fn fmt_speed(bps: f64) -> String {
    if bps >= 1024.0 * 1024.0 {
        format!("{:.1} MB/s", bps / (1024.0 * 1024.0))
    } else {
        format!("{:.0} KB/s", bps / 1024.0)
    }
}
