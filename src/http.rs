use std::ffi::c_void;

use windows::core::{w, PCWSTR, PWSTR};
use windows::Win32::Networking::WinHttp::*;

struct Handle(*mut c_void);

impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                let _ = WinHttpCloseHandle(self.0);
            }
        }
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

pub fn get(url: &str) -> Option<String> {
    fetch(url).ok()
}

fn fetch(url: &str) -> Result<String, String> {
    let url = url.trim();
    let url = match url.strip_prefix("webcal://") {
        Some(rest) => format!("https://{rest}"),
        None => url.to_string(),
    };
    let wurl: Vec<u16> = url.encode_utf16().collect();
    let mut host = vec![0u16; 512];
    let mut path = vec![0u16; 8192];
    let mut extra = vec![0u16; 8192];
    let mut uc = URL_COMPONENTS {
        dwStructSize: std::mem::size_of::<URL_COMPONENTS>() as u32,
        lpszHostName: PWSTR(host.as_mut_ptr()),
        dwHostNameLength: host.len() as u32,
        lpszUrlPath: PWSTR(path.as_mut_ptr()),
        dwUrlPathLength: path.len() as u32,
        lpszExtraInfo: PWSTR(extra.as_mut_ptr()),
        dwExtraInfoLength: extra.len() as u32,
        ..Default::default()
    };
    unsafe {
        WinHttpCrackUrl(&wurl, 0, &mut uc).map_err(|e| format!("crack {e}"))?;
        let host = String::from_utf16_lossy(&host[..uc.dwHostNameLength as usize]);
        let target = String::from_utf16_lossy(&path[..uc.dwUrlPathLength as usize]) + &String::from_utf16_lossy(&extra[..uc.dwExtraInfoLength as usize]);
        let secure = url.to_ascii_lowercase().starts_with("https://");

        let session = Handle(WinHttpOpen(w!("Taskbaroque"), WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, PCWSTR::null(), PCWSTR::null(), 0));
        if session.0.is_null() {
            return Err(format!("open {:?}", windows::Win32::Foundation::GetLastError()));
        }
        let _ = WinHttpSetTimeouts(session.0, 10_000, 10_000, 15_000, 20_000);
        let conn = Handle(WinHttpConnect(session.0, PCWSTR(wide(&host).as_ptr()), uc.nPort, 0));
        if conn.0.is_null() {
            return Err(format!("connect {:?}", windows::Win32::Foundation::GetLastError()));
        }
        let req = Handle(WinHttpOpenRequest(
            conn.0,
            w!("GET"),
            PCWSTR(wide(&target).as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            std::ptr::null(),
            if secure { WINHTTP_FLAG_SECURE } else { WINHTTP_OPEN_REQUEST_FLAGS(0) },
        ));
        if req.0.is_null() {
            return Err(format!("request {:?}", windows::Win32::Foundation::GetLastError()));
        }
        WinHttpSendRequest(req.0, None, None, 0, 0, 0).map_err(|e| format!("send {e}"))?;
        WinHttpReceiveResponse(req.0, std::ptr::null_mut()).map_err(|e| format!("receive {e}"))?;

        let mut status = 0u32;
        let mut len = 4u32;
        WinHttpQueryHeaders(req.0, WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER, PCWSTR::null(), Some(&mut status as *mut u32 as _), &mut len, std::ptr::null_mut()).map_err(|e| format!("status {e}"))?;
        if !(200..300).contains(&status) {
            return Err(format!("http {status}"));
        }

        let mut body = Vec::new();
        let mut buf = vec![0u8; 16 * 1024];
        loop {
            let mut read = 0u32;
            WinHttpReadData(req.0, buf.as_mut_ptr() as _, buf.len() as u32, &mut read).map_err(|e| format!("read {e} after {} bytes", body.len()))?;
            if read == 0 {
                break;
            }
            body.extend_from_slice(&buf[..read as usize]);
        }
        Ok(String::from_utf8_lossy(&body).into_owned())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore = "ağ gerektirir"]
    fn get_live() {
        let r = super::fetch("https://api.open-meteo.com/v1/forecast?latitude=41&longitude=29&current=temperature_2m");
        println!("{r:?}");
        assert!(r.is_ok());
    }
}
