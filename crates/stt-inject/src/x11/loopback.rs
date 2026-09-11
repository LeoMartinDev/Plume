use std::time::{Duration, Instant};

use stt_core::TextInjector;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    ConfigureWindowAux, ConnectionExt as XProtoExt, CreateWindowAux, EventMask, GrabMode,
    GrabStatus, InputFocus, StackMode, WindowClass,
};
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;
use x11rb::{COPY_DEPTH_FROM_PARENT, CURRENT_TIME};

use super::X11Injector;

const INSERT_TEXT: &str = "hi";
const REPLACE_NEW: &str = "ho";
const TIMEOUT: Duration = Duration::from_secs(2);
const XK_BACKSPACE: u32 = 0xff08;

struct Scratch {
    conn: RustConnection,
    win: u32,
    grabbed: bool,
}

impl Drop for Scratch {
    fn drop(&mut self) {
        if self.grabbed {
            let _ = self.conn.ungrab_keyboard(CURRENT_TIME);
        }
        let _ = self.conn.destroy_window(self.win);
        let _ = self.conn.flush();
    }
}

impl Scratch {
    fn open() -> Result<Self, Box<dyn std::error::Error>> {
        let (conn, screen_num) = x11rb::connect(None)?;
        let screen = &conn.setup().roots[screen_num];
        let win = conn.generate_id()?;
        let aux = CreateWindowAux::new()
            .event_mask(
                EventMask::KEY_PRESS
                    | EventMask::KEY_RELEASE
                    | EventMask::STRUCTURE_NOTIFY
                    | EventMask::FOCUS_CHANGE,
            )
            .override_redirect(1)
            .background_pixel(screen.black_pixel);
        conn.create_window(
            COPY_DEPTH_FROM_PARENT,
            win,
            screen.root,
            0,
            0,
            16,
            16,
            0,
            WindowClass::INPUT_OUTPUT,
            0,
            &aux,
        )?;
        conn.map_window(win)?;
        conn.configure_window(win, &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE))?;
        conn.sync()?;
        wait_map(&conn, win)?;
        conn.set_input_focus(InputFocus::PARENT, win, CURRENT_TIME)?;
        conn.sync()?;
        // XTEST fake keys go to the keyboard grab, so this client sees them even
        // if a window manager keeps focus on another window.
        let grab = conn
            .grab_keyboard(false, win, CURRENT_TIME, GrabMode::ASYNC, GrabMode::ASYNC)?
            .reply()?;
        if grab.status != GrabStatus::SUCCESS {
            return Err(format!("grab_keyboard failed: {:?}", grab.status).into());
        }
        Ok(Self {
            conn,
            win,
            grabbed: true,
        })
    }
}

fn wait_map(conn: &RustConnection, win: u32) -> Result<(), Box<dyn std::error::Error>> {
    let deadline = Instant::now() + TIMEOUT;
    while Instant::now() < deadline {
        while let Some(event) = conn.poll_for_event()? {
            if let Event::MapNotify(map) = event {
                if map.window == win {
                    return Ok(());
                }
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    Err("timed out waiting for MapNotify on the scratch window".into())
}

fn wait_keys(
    conn: &RustConnection,
    min: usize,
) -> Result<Vec<(bool, u8)>, Box<dyn std::error::Error>> {
    let deadline = Instant::now() + TIMEOUT;
    let mut out = Vec::new();
    while out.len() < min && Instant::now() < deadline {
        while let Some(event) = conn.poll_for_event()? {
            match event {
                Event::KeyPress(press) => out.push((true, press.detail)),
                Event::KeyRelease(release) => out.push((false, release.detail)),
                _ => {}
            }
        }
        if out.len() >= min {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    Ok(out)
}

fn modifier_keycodes(conn: &RustConnection) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let map = conn.get_modifier_mapping()?.reply()?;
    Ok(map.keycodes.into_iter().filter(|code| *code != 0).collect())
}

fn press_keysyms(
    conn: &RustConnection,
    events: &[(bool, u8)],
    modifiers: &[u8],
) -> Result<Vec<u32>, Box<dyn std::error::Error>> {
    let setup = conn.setup();
    let min = setup.min_keycode;
    let count = setup.max_keycode.saturating_sub(min).saturating_add(1);
    let mapping = conn.get_keyboard_mapping(min, count)?.reply()?;
    let per = usize::from(mapping.keysyms_per_keycode);
    let mut keysyms = Vec::new();
    for &(down, keycode) in events {
        if !down || modifiers.contains(&keycode) {
            continue;
        }
        let idx = usize::from(keycode.saturating_sub(min)) * per;
        keysyms.push(mapping.keysyms.get(idx).copied().unwrap_or(0));
    }
    Ok(keysyms)
}

#[test]
fn xtest_loopback_insert_then_replace_last() {
    if std::env::var_os("DISPLAY").is_none() {
        eprintln!("x11 loopback SKIPPED: DISPLAY is unset");
        return;
    }

    let scratch = Scratch::open().expect("scratch X11 window");
    let modifiers = modifier_keycodes(&scratch.conn).expect("modifier map");
    let mut injector = X11Injector::connect().expect("X11 injector");

    injector.insert(INSERT_TEXT).expect("insert via XTEST");
    let insert_events = wait_keys(&scratch.conn, INSERT_TEXT.len() * 2).expect("insert key events");
    let insert_syms =
        press_keysyms(&scratch.conn, &insert_events, &modifiers).expect("decode insert");
    println!(
        "x11 loopback insert {INSERT_TEXT:?} events={insert_events:?} keysyms={insert_syms:#x?}"
    );
    assert_eq!(
        insert_syms,
        vec![u32::from(b'h'), u32::from(b'i')],
        "insert() must deliver h then i through XTEST"
    );

    injector
        .replace_last(INSERT_TEXT, REPLACE_NEW)
        .expect("replace_last via XTEST");
    let replace_events = wait_keys(&scratch.conn, INSERT_TEXT.len() * 2 + REPLACE_NEW.len() * 2)
        .expect("replace_last key events");
    let replace_syms =
        press_keysyms(&scratch.conn, &replace_events, &modifiers).expect("decode replace");
    println!(
        "x11 loopback replace_last {INSERT_TEXT:?}->{REPLACE_NEW:?} events={replace_events:?} keysyms={replace_syms:#x?}"
    );
    assert_eq!(
        replace_syms,
        vec![XK_BACKSPACE, XK_BACKSPACE, u32::from(b'h'), u32::from(b'o')],
        "replace_last() must replay backspaces then retype"
    );
}
