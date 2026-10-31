use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use windows::core::Result;
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session,
    GlobalSystemMediaTransportControlsSessionManager as Manager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
};
use windows::Storage::Streams::DataReader;
use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};

use crate::util;

#[derive(Clone, Default, PartialEq)]
pub struct MediaInfo {
    pub active: bool,
    pub title: String,
    pub artist: String,
    pub playing: bool,
    pub thumb: Option<Arc<Vec<u8>>>,
}

impl MediaInfo {
    pub fn track_key(&self) -> String {
        format!("{}\u{1}{}", self.title, self.artist)
    }
}

pub enum MediaCmd {
    Toggle,
    Next,
    Prev,
}

pub fn spawn(state: Arc<Mutex<MediaInfo>>, notify: isize) -> Sender<MediaCmd> {
    let (tx, rx) = channel::<MediaCmd>();
    std::thread::spawn(move || {
        unsafe {
            let _ = RoInitialize(RO_INIT_MULTITHREADED);
        }
        let Ok(manager) = Manager::RequestAsync().and_then(|op| op.get()) else { return };
        loop {
            match rx.recv_timeout(Duration::from_millis(1000)) {
                Ok(cmd) => {
                    if let Ok(s) = manager.GetCurrentSession() {
                        let _ = run_cmd(&s, cmd);
                    }
                    std::thread::sleep(Duration::from_millis(150));
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
            let prev = state.lock().unwrap().clone();
            let next = read(&manager, &prev).unwrap_or_default();
            if next != prev {
                *state.lock().unwrap() = next;
                util::post_data_changed(notify);
            }
        }
    });
    tx
}

fn run_cmd(s: &Session, cmd: MediaCmd) -> Result<()> {
    let op = match cmd {
        MediaCmd::Toggle => s.TryTogglePlayPauseAsync()?,
        MediaCmd::Next => s.TrySkipNextAsync()?,
        MediaCmd::Prev => s.TrySkipPreviousAsync()?,
    };
    op.get()?;
    Ok(())
}

fn read(manager: &Manager, prev: &MediaInfo) -> Result<MediaInfo> {
    let s = manager.GetCurrentSession()?;
    let props = s.TryGetMediaPropertiesAsync()?.get()?;
    let mut info = MediaInfo {
        active: true,
        title: props.Title()?.to_string(),
        artist: props.Artist()?.to_string(),
        playing: s.GetPlaybackInfo()?.PlaybackStatus()? == Status::Playing,
        thumb: None,
    };
    if info.title.is_empty() {
        return Ok(MediaInfo::default());
    }
    info.thumb = if prev.active && prev.track_key() == info.track_key() && prev.thumb.is_some() {
        prev.thumb.clone()
    } else {
        read_thumb(&props).ok().map(Arc::new)
    };
    Ok(info)
}

fn read_thumb(props: &windows::Media::Control::GlobalSystemMediaTransportControlsSessionMediaProperties) -> Result<Vec<u8>> {
    let stream = props.Thumbnail()?.OpenReadAsync()?.get()?;
    let size = stream.Size()?.min(4 * 1024 * 1024) as u32;
    let reader = DataReader::CreateDataReader(&stream)?;
    reader.LoadAsync(size)?.get()?;
    let mut buf = vec![0u8; size as usize];
    reader.ReadBytes(&mut buf)?;
    Ok(buf)
}
