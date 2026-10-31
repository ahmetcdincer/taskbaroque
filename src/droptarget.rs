use windows::core::{implement, Result};
use windows::Win32::Foundation::*;
use windows::Win32::System::Com::IDataObject;
use windows::Win32::System::Ole::*;
use windows::Win32::System::SystemServices::MODIFIERKEYS_FLAGS;
use windows::Win32::UI::WindowsAndMessaging::*;

pub const WM_APP_DRAG: u32 = WM_APP + 20;

#[implement(IDropTarget)]
struct Target {
    notify: HWND,
}

impl Target {
    fn post(&self, inside: bool, effect: *mut DROPEFFECT) {
        unsafe {
            let _ = PostMessageW(self.notify, WM_APP_DRAG, WPARAM(inside as usize), LPARAM(0));
            if !effect.is_null() {
                *effect = DROPEFFECT_NONE;
            }
        }
    }
}

impl IDropTarget_Impl for Target_Impl {
    fn DragEnter(&self, _: Option<&IDataObject>, _: MODIFIERKEYS_FLAGS, _: &POINTL, effect: *mut DROPEFFECT) -> Result<()> {
        self.post(true, effect);
        Ok(())
    }

    fn DragOver(&self, _: MODIFIERKEYS_FLAGS, _: &POINTL, effect: *mut DROPEFFECT) -> Result<()> {
        self.post(true, effect);
        Ok(())
    }

    fn DragLeave(&self) -> Result<()> {
        self.post(false, std::ptr::null_mut());
        Ok(())
    }

    fn Drop(&self, _: Option<&IDataObject>, _: MODIFIERKEYS_FLAGS, _: &POINTL, effect: *mut DROPEFFECT) -> Result<()> {
        self.post(false, effect);
        Ok(())
    }
}

pub fn register(hwnd: HWND, notify: HWND) {
    let t: IDropTarget = Target { notify }.into();
    unsafe {
        let _ = RegisterDragDrop(hwnd, &t);
    }
}

pub fn revoke(hwnd: HWND) {
    unsafe {
        let _ = RevokeDragDrop(hwnd);
    }
}
