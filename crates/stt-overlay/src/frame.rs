pub(crate) fn hide_server_frame(title: &str) {
    #[cfg(target_os = "linux")]
    linux::hide(title);
    #[cfg(not(target_os = "linux"))]
    let _ = title;
}

#[cfg(target_os = "linux")]
mod linux {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{
        Atom, AtomEnum, ConfigureWindowAux, ConnectionExt as _, PropMode, Window,
    };
    use x11rb::rust_connection::RustConnection;
    use x11rb::wrapper::ConnectionExt as _;

    const MWM_HINTS_DECORATIONS: u32 = 1 << 1;

    pub(crate) fn hide(title: &str) {
        let Ok((conn, screen)) = x11rb::connect(None) else {
            return;
        };
        let root = conn.setup().roots[screen].root;
        let Ok(wm_name) = intern(&conn, b"WM_NAME") else {
            return;
        };
        let Ok(net_name) = intern(&conn, b"_NET_WM_NAME") else {
            return;
        };
        let Ok(motif) = intern(&conn, b"_MOTIF_WM_HINTS") else {
            return;
        };
        let Ok(pid_atom) = intern(&conn, b"_NET_WM_PID") else {
            return;
        };
        let mut matches = Vec::new();
        collect(&conn, root, wm_name, net_name, title, 0, &mut matches);
        let own_pid = std::process::id();
        for window in matches {
            if window_pid(&conn, window, pid_atom) != Some(own_pid) {
                continue;
            }
            if decorations_cleared(&conn, window, motif) {
                continue;
            }
            strip_frame(&conn, root, window, motif);
        }
    }

    fn decorations_cleared(conn: &RustConnection, window: Window, motif: Atom) -> bool {
        let Ok(cookie) = conn.get_property(false, window, motif, motif, 0, 5) else {
            return false;
        };
        let Ok(reply) = cookie.reply() else {
            return false;
        };
        let mut fields = [0u32; 5];
        for (index, chunk) in reply.value.chunks_exact(4).take(5).enumerate() {
            fields[index] = u32::from_ne_bytes(chunk.try_into().unwrap_or([0; 4]));
        }
        fields[0] & MWM_HINTS_DECORATIONS != 0 && fields[2] == 0
    }

    // Weston applies `_MOTIF_WM_HINTS` at map time. Clearing the hint on a
    // window that is already mapped leaves the title bar in place.
    fn strip_frame(conn: &RustConnection, root: Window, window: Window, motif: Atom) {
        let place = conn
            .translate_coordinates(window, root, 0, 0)
            .ok()
            .and_then(|cookie| cookie.reply().ok());
        let geom = conn
            .get_geometry(window)
            .ok()
            .and_then(|cookie| cookie.reply().ok());
        let _ = conn.unmap_window(window);
        let _ = conn.change_property32(
            PropMode::REPLACE,
            window,
            motif,
            motif,
            &[MWM_HINTS_DECORATIONS, 0, 0, 0, 0],
        );
        let _ = conn.map_window(window);
        if let (Some(place), Some(geom)) = (place, geom) {
            let _ = conn.configure_window(
                window,
                &ConfigureWindowAux::new()
                    .x(i32::from(place.dst_x))
                    .y(i32::from(place.dst_y))
                    .width(u32::from(geom.width))
                    .height(u32::from(geom.height))
                    .border_width(0),
            );
        }
        let _ = conn.flush();
    }

    fn window_pid(conn: &RustConnection, window: Window, atom: Atom) -> Option<u32> {
        let reply = conn
            .get_property(false, window, atom, AtomEnum::CARDINAL, 0, 1)
            .ok()?
            .reply()
            .ok()?;
        let bytes: [u8; 4] = reply.value.get(..4)?.try_into().ok()?;
        Some(u32::from_ne_bytes(bytes))
    }

    fn intern(conn: &RustConnection, name: &[u8]) -> Result<Atom, ()> {
        conn.intern_atom(false, name)
            .map_err(|_| ())?
            .reply()
            .map(|reply| reply.atom)
            .map_err(|_| ())
    }

    fn collect(
        conn: &RustConnection,
        window: Window,
        wm_name: Atom,
        net_name: Atom,
        title: &str,
        depth: u8,
        matches: &mut Vec<Window>,
    ) {
        if depth > 8 {
            return;
        }
        if window_title(conn, window, wm_name)
            .or_else(|| window_title(conn, window, net_name))
            .is_some_and(|name| name == title)
        {
            matches.push(window);
        }
        let Ok(tree) = conn
            .query_tree(window)
            .map_err(|_| ())
            .and_then(|cookie| cookie.reply().map_err(|_| ()))
        else {
            return;
        };
        for child in tree.children {
            collect(conn, child, wm_name, net_name, title, depth + 1, matches);
        }
    }

    fn window_title(conn: &RustConnection, window: Window, atom: Atom) -> Option<String> {
        let reply = conn
            .get_property(false, window, atom, AtomEnum::ANY, 0, 256)
            .ok()?
            .reply()
            .ok()?;
        if reply.value.is_empty() {
            return None;
        }
        let name = String::from_utf8_lossy(&reply.value);
        Some(name.trim_end_matches('\u{0}').to_string())
    }
}
