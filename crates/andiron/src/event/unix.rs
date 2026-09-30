use super::{Event, KeyCode, KeyEvent, KeyModifiers};
use crate::Size;
use std::{
    cell::RefCell,
    collections::VecDeque,
    fs::File,
    io::{self, Read, Write},
    os::fd::AsRawFd,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Default)]
struct Decoder {
    bytes: Vec<u8>,
    events: VecDeque<Event>,
    cursor: Option<(u16, u16)>,
    size: Option<Size>,
    in_band: bool,
    paste: Option<Vec<u8>>,
}
impl Decoder {
    fn key(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        self.events
            .push_back(Event::Key(KeyEvent::new(code, modifiers)));
    }
    fn feed(&mut self, bytes: &[u8]) {
        self.bytes.extend_from_slice(bytes);
        loop {
            if let Some(paste) = &mut self.paste {
                if let Some(end) = self.bytes.windows(6).position(|s| s == b"\x1b[201~") {
                    paste.extend_from_slice(&self.bytes[..end]);
                    self.bytes.drain(..end + 6);
                    self.events
                        .push_back(Event::Paste(String::from_utf8_lossy(paste).into_owned()));
                    self.paste = None;
                    continue;
                }
                let available = self.bytes.len().saturating_sub(5);
                paste.extend(self.bytes.drain(..available));
                break;
            }
            let Some(&byte) = self.bytes.first() else {
                break;
            };
            if self.bytes.starts_with(b"\x1b\x1b") {
                self.bytes.remove(0);
                self.key(KeyCode::Esc, KeyModifiers::NONE);
                continue;
            }
            if self.bytes.starts_with(b"\x1b[") {
                let Some(end) = self
                    .bytes
                    .iter()
                    .enumerate()
                    .skip(2)
                    .find(|(_, b)| (0x40..=0x7e).contains(*b))
                    .map(|(i, _)| i)
                else {
                    break;
                };
                let sequence: Vec<_> = self.bytes.drain(..=end).collect();
                self.csi(&sequence[2..]);
                continue;
            }
            if byte == 27 && self.bytes.len() < 2 {
                break;
            }
            if self.bytes.starts_with(b"\x1bO") {
                if self.bytes.len() < 3 {
                    break;
                }
                let last = self.bytes[2];
                self.bytes.drain(..3);
                self.csi(&[last]);
                continue;
            }
            let alt = byte == 27;
            let start = usize::from(alt);
            let bytes = &self.bytes[start..];
            let count = match bytes[0] {
                0..=0x7f => 1,
                0xc2..=0xdf => 2,
                0xe0..=0xef => 3,
                0xf0..=0xf4 => 4,
                _ => 1,
            };
            if bytes.len() < count {
                break;
            }
            let ch = std::str::from_utf8(&bytes[..count])
                .ok()
                .and_then(|s| s.chars().next())
                .unwrap_or('\u{fffd}');
            self.bytes.drain(..start + count);
            let mut modifiers = if alt {
                KeyModifiers::ALT
            } else {
                KeyModifiers::NONE
            };
            let code = match ch {
                '\r' | '\n' => KeyCode::Enter,
                '\0' => {
                    modifiers |= KeyModifiers::CONTROL;
                    KeyCode::Char(' ')
                }
                '\x1c'..='\x1f' => {
                    modifiers |= KeyModifiers::CONTROL;
                    KeyCode::Char((ch as u8 + 0x40) as char)
                }
                '\t' => KeyCode::Tab,
                '\x08' | '\x7f' => KeyCode::Backspace,
                '\x01'..='\x1a' => {
                    modifiers |= KeyModifiers::CONTROL;
                    KeyCode::Char((ch as u8 + b'a' - 1) as char)
                }
                _ => KeyCode::Char(ch),
            };
            self.key(code, modifiers);
        }
    }
    fn csi(&mut self, sequence: &[u8]) {
        let (&last, params) = sequence.split_last().unwrap();
        let args: Vec<u16> = std::str::from_utf8(params)
            .unwrap_or("")
            .split(';')
            .filter_map(|s| s.parse().ok())
            .collect();
        match (last, args.as_slice()) {
            (b'R', [row, column]) => {
                self.cursor = Some((column.saturating_sub(1), row.saturating_sub(1)));
                return;
            }
            (b't', [kind @ (8 | 48), rows, columns, ..]) if *rows > 0 && *columns > 0 => {
                if *kind == 8 {
                    self.size = Some(Size {
                        columns: *columns,
                        rows: *rows,
                    });
                }
                if *kind == 48 {
                    self.in_band = true;
                    self.events.push_back(Event::Resize(*columns, *rows));
                }
                return;
            }
            (b'~', [200]) => {
                self.paste = Some(Vec::new());
                return;
            }
            _ => {}
        }
        let mut modifiers = KeyModifiers::NONE;
        let bits = args.get(1).copied().unwrap_or(1).saturating_sub(1);
        if bits & 1 != 0 {
            modifiers |= KeyModifiers::SHIFT;
        }
        if bits & 2 != 0 {
            modifiers |= KeyModifiers::ALT;
        }
        if bits & 4 != 0 {
            modifiers |= KeyModifiers::CONTROL;
        }
        let code = match last {
            b'P' => KeyCode::F(1),
            b'Q' => KeyCode::F(2),
            b'R' => KeyCode::F(3),
            b'S' => KeyCode::F(4),
            b'A' => KeyCode::Up,
            b'B' => KeyCode::Down,
            b'C' => KeyCode::Right,
            b'D' => KeyCode::Left,
            b'H' => KeyCode::Home,
            b'F' => KeyCode::End,
            b'Z' => {
                modifiers |= KeyModifiers::SHIFT;
                KeyCode::BackTab
            }
            b'~' => match args.first() {
                Some(1 | 7) => KeyCode::Home,
                Some(4 | 8) => KeyCode::End,
                Some(2) => KeyCode::Insert,
                Some(3) => KeyCode::Delete,
                Some(5) => KeyCode::PageUp,
                Some(6) => KeyCode::PageDown,
                Some(11..=15) => KeyCode::F((args[0] - 10) as u8),
                Some(17..=21) => KeyCode::F((args[0] - 11) as u8),
                Some(23..=24) => KeyCode::F((args[0] - 12) as u8),
                _ => return,
            },
            _ => return,
        };
        self.key(code, modifiers);
    }
}

struct Reader {
    tty: File,
    resized: Arc<AtomicBool>,
    resize_signal: signal_hook::SigId,
    decoder: Decoder,
    pty_size: (u16, u16),
    actual_size: Option<Size>,
    probe_pending: bool,
    last_probe: Instant,
    geometry_active: bool,
}
impl Reader {
    fn new() -> io::Result<Self> {
        let tty = File::open("/dev/tty")?;
        let pty_size = crossterm::terminal::size()?;
        let resized = Arc::new(AtomicBool::new(false));
        let resize_signal =
            signal_hook::flag::register(signal_hook::consts::SIGWINCH, Arc::clone(&resized))?;
        Ok(Self {
            tty,
            resized,
            resize_signal,
            decoder: Decoder::default(),
            pty_size,
            actual_size: None,
            probe_pending: false,
            last_probe: Instant::now(),
            geometry_active: false,
        })
    }
    fn pump(&mut self, timeout: Duration) -> io::Result<()> {
        // Some frontends resize immediately but throttle TIOCSWINSZ. Probe
        // their grid before that notification, without moving the input cursor.
        // Only use this path after the frontend answered the initial size query.
        if !self.geometry_active
            && self.actual_size.is_some()
            && self.last_probe.elapsed() >= Duration::from_millis(16)
            && (!self.probe_pending || self.last_probe.elapsed() >= Duration::from_millis(100))
        {
            self.decoder.size = None;
            let mut output = io::stdout().lock();
            output.write_all(b"\x1b[18t")?;
            output.flush()?;
            self.probe_pending = true;
            self.last_probe = Instant::now();
        }
        let fd = self.tty.as_raw_fd();
        if fd as usize >= libc::FD_SETSIZE {
            return Err(io::Error::other(
                "terminal descriptor exceeds select capacity",
            ));
        }
        // SAFETY: the fd is live, within FD_SETSIZE, and the initialized fd_set
        // and timeval remain valid for this synchronous select call.
        let result = unsafe {
            let mut readfds: libc::fd_set = std::mem::zeroed();
            libc::FD_ZERO(&mut readfds);
            libc::FD_SET(fd, &mut readfds);
            let mut timeout = libc::timeval {
                tv_sec: timeout.as_secs() as _,
                tv_usec: timeout.subsec_micros() as _,
            };
            libc::select(
                fd + 1,
                &mut readfds,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut timeout,
            )
        };
        if result < 0 {
            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::Interrupted {
                return Err(error);
            }
        } else if result > 0 {
            let mut bytes = [0; 4096];
            let count = self.tty.read(&mut bytes)?;
            if count == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "terminal input closed",
                ));
            }
            self.decoder.feed(&bytes[..count]);
            if let Some(size) = self.decoder.size {
                self.probe_pending = false;
                if self.actual_size.is_some_and(|old| old != size) {
                    self.decoder
                        .events
                        .push_back(Event::Resize(size.columns, size.rows));
                }
                self.actual_size = Some(size);
            }
        } else if timeout >= Duration::from_millis(10) && self.decoder.bytes == b"\x1b" {
            self.decoder.bytes.clear();
            self.decoder.key(KeyCode::Esc, KeyModifiers::NONE);
        }
        let size = crossterm::terminal::size()?;
        if self.resized.swap(false, Ordering::Relaxed) || size != self.pty_size {
            self.pty_size = size;
            if !self.decoder.in_band {
                self.decoder.events.push_back(Event::Resize(size.0, size.1));
            }
        }
        Ok(())
    }
}
impl Drop for Reader {
    fn drop(&mut self) {
        signal_hook::low_level::unregister(self.resize_signal);
    }
}
thread_local! { static READER: RefCell<Option<Reader>> = const { RefCell::new(None) }; }
fn with_reader<T>(f: impl FnOnce(&mut Reader) -> io::Result<T>) -> io::Result<T> {
    READER.with(|cell| {
        let mut reader = cell.borrow_mut();
        if reader.is_none() {
            *reader = Some(Reader::new()?);
        }
        f(reader.as_mut().unwrap())
    })
}
pub fn poll(timeout: Duration) -> io::Result<bool> {
    with_reader(|reader| {
        let start = Instant::now();
        loop {
            if !reader.decoder.events.is_empty() {
                return Ok(true);
            }
            reader.pump(
                timeout
                    .saturating_sub(start.elapsed())
                    .min(Duration::from_millis(20)),
            )?;
            if start.elapsed() >= timeout {
                return Ok(!reader.decoder.events.is_empty());
            }
        }
    })
}
pub fn read() -> io::Result<Event> {
    while !poll(Duration::from_millis(20))? {}
    with_reader(|reader| {
        reader.pump(Duration::ZERO)?;
        let mut event = reader.decoder.events.pop_front().unwrap();
        if matches!(event, Event::Resize(..)) {
            while matches!(reader.decoder.events.front(), Some(Event::Resize(..))) {
                event = reader.decoder.events.pop_front().unwrap();
            }
        }
        Ok(event)
    })
}
pub(crate) fn geometry() -> io::Result<(Size, u16, u16)> {
    with_reader(|reader| {
        reader.geometry_active = true;
        let result = query_geometry(reader);
        reader.geometry_active = false;
        result
    })
}
fn query_geometry(reader: &mut Reader) -> io::Result<(Size, u16, u16)> {
    reader.decoder.cursor = None;
    reader.decoder.size = None;
    let mut output = io::stdout().lock();
    output.write_all(b"\x1b[18t\x1b[6n")?;
    output.flush()?;
    drop(output);
    let start = Instant::now();
    loop {
        reader.pump(Duration::from_millis(10))?;
        if let (Some(size), Some((column, row))) = (reader.decoder.size, reader.decoder.cursor) {
            return Ok((size, column, row));
        }
        if reader.decoder.cursor.is_some() || start.elapsed() > Duration::from_secs(1) {
            if let Some((column, row)) = reader.decoder.cursor {
                let (columns, rows) = crossterm::terminal::size()?;
                return Ok((
                    Size {
                        columns: columns.max(1),
                        rows: rows.max(1),
                    },
                    column,
                    row,
                ));
            }
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "terminal did not report cursor position",
            ));
        }
    }
}

pub(crate) fn reset() {
    READER.with(|reader| *reader.borrow_mut() = None);
}

#[cfg(test)]
mod tests {
    use super::*;
    fn key(code: KeyCode, modifiers: KeyModifiers) -> Event {
        Event::Key(KeyEvent::new(code, modifiers))
    }
    #[test]
    fn fragmented_reports_do_not_become_input_or_reorder_keys() {
        let bytes = b"a\x1b[8;40;96t\x1b[16;15Rb";
        for split in 0..=bytes.len() {
            let mut decoder = Decoder::default();
            decoder.feed(&bytes[..split]);
            decoder.feed(&bytes[split..]);
            assert_eq!(
                decoder.size,
                Some(Size {
                    rows: 40,
                    columns: 96
                })
            );
            assert_eq!(decoder.cursor, Some((14, 15)));
            assert_eq!(
                decoder.events,
                [
                    key(KeyCode::Char('a'), KeyModifiers::NONE),
                    key(KeyCode::Char('b'), KeyModifiers::NONE)
                ]
            );
        }
    }
    #[test]
    fn escape_before_report_does_not_consume_the_report() {
        let mut decoder = Decoder::default();
        decoder.feed(b"\x1b\x1b[2;3R");
        assert_eq!(decoder.cursor, Some((2, 1)));
        assert_eq!(decoder.events, [key(KeyCode::Esc, KeyModifiers::NONE)]);
    }
    #[test]
    fn paste_and_utf8_can_arrive_one_byte_at_a_time() {
        let mut decoder = Decoder::default();
        let bytes = "界\x1b[200~a\n界\x1b[6n\x1b[201~".as_bytes();
        for byte in bytes {
            decoder.feed(&[*byte]);
        }
        assert_eq!(
            decoder.events,
            [
                key(KeyCode::Char('界'), KeyModifiers::NONE),
                Event::Paste("a\n界\x1b[6n".into())
            ]
        );
        assert!(decoder.cursor.is_none());
    }
    #[test]
    fn resize_notification_does_not_overwrite_query_response() {
        let mut decoder = Decoder::default();
        decoder.feed(b"\x1b[8;40;96t\x1b[48;40;90;0;0t");
        assert_eq!(
            decoder.size,
            Some(Size {
                rows: 40,
                columns: 96
            })
        );
        assert_eq!(decoder.events, [Event::Resize(90, 40)]);
        assert!(decoder.in_band);
    }
    #[test]
    fn editing_keys_and_modifiers() {
        let mut decoder = Decoder::default();
        decoder.feed(b"\x1b[D\x1b[1;5C\x1b[3~\x1b[Z\x1bx\x04\x1bOP");
        assert_eq!(
            decoder.events,
            [
                key(KeyCode::Left, KeyModifiers::NONE),
                key(KeyCode::Right, KeyModifiers::CONTROL),
                key(KeyCode::Delete, KeyModifiers::NONE),
                key(KeyCode::BackTab, KeyModifiers::SHIFT),
                key(KeyCode::Char('x'), KeyModifiers::ALT),
                key(KeyCode::Char('d'), KeyModifiers::CONTROL),
                key(KeyCode::F(1), KeyModifiers::NONE),
            ]
        );
    }
}
