use std::sync::OnceLock;
use wasmi::{
    Engine, Instance, Linker, Memory, Module, Store, StoreLimits, StoreLimitsBuilder, WasmParams,
    WasmResults,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Cell {
    pub text: String,
    pub foreground: [u8; 3],
    pub background: [u8; 3],
    pub bold: bool,
    pub underline: bool,
    pub width: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Frame {
    pub lines: Vec<Vec<Cell>>,
    pub cursor: Option<(u16, u16)>,
    pub background: [u8; 3],
}

pub(super) struct Ghostty {
    store: Store<StoreLimits>,
    instance: Instance,
    memory: Memory,
    scratch: u32,
    terminal: u32,
    render: u32,
    iterator: u32,
    cells: u32,
    encoder: u32,
    event: u32,
    cols: u16,
    rows: u16,
    query: Vec<u8>,
}

fn module() -> Result<&'static (Engine, Module), String> {
    static MODULE: OnceLock<Result<(Engine, Module), String>> = OnceLock::new();
    MODULE
        .get_or_init(|| {
            let mut config = wasmi::Config::default();
            config.consume_fuel(true);
            let engine = Engine::new(&config);
            let module = Module::new(&engine, include_bytes!("vendor/ghostty-vt.wasm"))
                .map_err(|e| e.to_string())?;
            Ok((engine, module))
        })
        .as_ref()
        .map_err(Clone::clone)
}

impl Ghostty {
    pub(super) fn new(cols: u16, rows: u16) -> Result<Self, String> {
        let (engine, module) = module()?;
        let mut store = Store::new(
            engine,
            StoreLimitsBuilder::new()
                .memory_size(128 * 1024 * 1024)
                .build(),
        );
        store.limiter(|limits| limits);
        store.set_fuel(100_000_000).map_err(|e| e.to_string())?;
        let mut linker = Linker::new(engine);
        for import in module.imports() {
            if import.module() != "env" || import.name() != "log" {
                return Err("Unexpected libghostty import".into());
            }
            let wasmi::ExternType::Func(ty) = import.ty() else {
                return Err("Unexpected libghostty import type".into());
            };
            linker
                .func_new("env", "log", ty.clone(), |_, _, _| Ok(()))
                .map_err(|e| e.to_string())?;
        }
        let instance = linker
            .instantiate_and_start(&mut store, module)
            .map_err(|e| e.to_string())?;
        let memory = instance
            .get_memory(&store, "memory")
            .ok_or("libghostty memory missing")?;
        let mut vt = Self {
            store,
            instance,
            memory,
            scratch: 0,
            terminal: 0,
            render: 0,
            iterator: 0,
            cells: 0,
            encoder: 0,
            event: 0,
            cols,
            rows,
            query: Vec::new(),
        };
        vt.scratch = vt.call("ghostty_wasm_alloc_u8_array", 4096u32)?;
        let layout: u32 = vt.call("ghostty_type_json", ())?;
        let data = vt
            .memory
            .data(&vt.store)
            .get(layout as usize..)
            .ok_or("Invalid libghostty layout")?;
        let length = data
            .iter()
            .position(|byte| *byte == 0)
            .ok_or("Invalid libghostty layout")?;
        let layout: serde_json::Value =
            serde_json::from_slice(&data[..length]).map_err(|e| e.to_string())?;
        if layout["GhosttyTerminalOptions"]["size"] != 8 || layout["GhosttyStyle"]["size"] != 72 {
            return Err("Unsupported libghostty layout".into());
        }
        vt.put(0, &[cols.to_le_bytes(), rows.to_le_bytes()].concat())?;
        vt.put(4, &10000u32.to_le_bytes())?;
        vt.ok("ghostty_terminal_new", (0u32, vt.scratch + 16, vt.scratch))?;
        vt.terminal = vt.u32(16)?;
        vt.render = vt.create("ghostty_render_state_new")?;
        vt.iterator = vt.create("ghostty_render_state_row_iterator_new")?;
        vt.cells = vt.create("ghostty_render_state_row_cells_new")?;
        vt.encoder = vt.create("ghostty_key_encoder_new")?;
        vt.event = vt.create("ghostty_key_event_new")?;
        Ok(vt)
    }

    fn create(&mut self, name: &str) -> Result<u32, String> {
        self.ok(name, (0u32, self.scratch))?;
        self.u32(0)
    }

    fn call<P: WasmParams, R: WasmResults>(&mut self, name: &str, args: P) -> Result<R, String> {
        self.instance
            .get_typed_func::<P, R>(&self.store, name)
            .map_err(|e| format!("{name}: {e}"))?
            .call(&mut self.store, args)
            .map_err(|e| format!("{name}: {e}"))
    }

    fn ok<P: WasmParams>(&mut self, name: &str, args: P) -> Result<(), String> {
        let result: i32 = self.call(name, args)?;
        if result != 0 {
            return Err(format!("{name}: {result}"));
        }
        Ok(())
    }

    fn fuel(&mut self) -> Result<(), String> {
        self.store.set_fuel(100_000_000).map_err(|e| e.to_string())
    }

    fn put(&mut self, offset: u32, data: &[u8]) -> Result<(), String> {
        self.memory
            .write(&mut self.store, (self.scratch + offset) as usize, data)
            .map_err(|e| e.to_string())
    }

    fn bytes(&self, offset: u32, length: usize) -> Result<&[u8], String> {
        let start = (self.scratch + offset) as usize;
        self.memory
            .data(&self.store)
            .get(start..start + length)
            .ok_or_else(|| "Invalid libghostty memory range".into())
    }

    fn u32(&self, offset: u32) -> Result<u32, String> {
        Ok(u32::from_le_bytes(
            self.bytes(offset, 4)?.try_into().unwrap(),
        ))
    }

    fn number(&mut self, name: &str, handle: u32, kind: u32) -> Result<u16, String> {
        self.put(0, &[0; 4])?;
        self.ok(name, (handle, kind, self.scratch))?;
        Ok(self.u32(0)? as u16)
    }

    pub(super) fn resize(&mut self, cols: u16, rows: u16) -> Result<(), String> {
        self.fuel()?;
        self.ok(
            "ghostty_terminal_resize",
            (self.terminal, u32::from(cols), u32::from(rows), 9u32, 18u32),
        )?;
        self.cols = cols;
        self.rows = rows;
        Ok(())
    }

    fn write(&mut self, bytes: &[u8]) -> Result<(), String> {
        for chunk in bytes.chunks(4096) {
            self.put(0, chunk)?;
            self.call::<_, ()>(
                "ghostty_terminal_vt_write",
                (self.terminal, self.scratch, chunk.len() as u32),
            )?;
        }
        Ok(())
    }

    pub(super) fn feed(&mut self, bytes: &[u8]) -> Result<Vec<u8>, String> {
        self.fuel()?;
        let mut responses = Vec::new();
        let mut start = 0;
        for (i, byte) in bytes.iter().copied().enumerate() {
            if self.query.is_empty() {
                if byte == 27 {
                    self.query.push(byte);
                }
                continue;
            }
            self.query.push(byte);
            let complete = if self.query.get(1) == Some(&b'[') {
                self.query.len() > 2 && (0x40..=0x7e).contains(&byte)
            } else if self.query.get(1) == Some(&b']') {
                byte == 7 || self.query.ends_with(b"\x1b\\")
            } else {
                true
            };
            if complete {
                self.write(&bytes[start..=i])?;
                start = i + 1;
                let response = match self.query.as_slice() {
                    b"\x1b[5n" => "\x1b[0n".into(),
                    b"\x1b[6n" | b"\x1b[?6n" => {
                        let x = self.number("ghostty_terminal_get", self.terminal, 3)? + 1;
                        let y = self.number("ghostty_terminal_get", self.terminal, 4)? + 1;
                        format!(
                            "\x1b[{}{y};{x}R",
                            if self.query[2] == b'?' { "?" } else { "" }
                        )
                    }
                    b"\x1b[18t" => format!("\x1b[8;{};{}t", self.rows, self.cols),
                    b"\x1b[14t" => format!("\x1b[4;{};{}t", self.rows * 18, self.cols * 9),
                    b"\x1b[16t" => "\x1b[6;18;9t".into(),
                    b"\x1b[c" | b"\x1b[0c" => "\x1b[?62;22c".into(),
                    _ => String::new(),
                };
                responses.extend_from_slice(response.as_bytes());
                self.query.clear();
            } else if self.query.len() >= 4096 {
                self.query.clear();
            }
        }
        self.write(&bytes[start..])?;
        Ok(responses)
    }

    pub(super) fn scroll(&mut self, delta: Option<i32>) -> Result<(), String> {
        self.fuel()?;
        self.put(0, &[0; 24])?;
        self.put(0, &if delta.is_some() { 2u32 } else { 1u32 }.to_le_bytes())?;
        self.put(8, &delta.unwrap_or(0).to_le_bytes())?;
        self.call(
            "ghostty_terminal_scroll_viewport",
            (self.terminal, self.scratch),
        )
    }

    pub(super) fn paste(&mut self, text: &str) -> Result<Vec<u8>, String> {
        self.fuel()?;
        let bracketed = self.number("ghostty_terminal_mode_get", self.terminal, 2004)? != 0;
        let text: String = text
            .chars()
            .filter(|c| !c.is_control() || matches!(c, '\n' | '\r' | '\t'))
            .collect();
        Ok(if bracketed {
            format!("\x1b[200~{text}\x1b[201~").into_bytes()
        } else {
            text.replace('\n', "\r").into_bytes()
        })
    }

    pub(super) fn key(
        &mut self,
        key: u32,
        mods: u32,
        text: &str,
        action: u32,
    ) -> Result<Vec<u8>, String> {
        self.fuel()?;
        if text.len() > 2048 {
            return Err("Keyboard text is too long".into());
        }
        self.call::<_, ()>(
            "ghostty_key_encoder_setopt_from_terminal",
            (self.encoder, self.terminal),
        )?;
        self.call::<_, ()>("ghostty_key_event_set_action", (self.event, action))?;
        self.call::<_, ()>("ghostty_key_event_set_key", (self.event, key))?;
        self.call::<_, ()>("ghostty_key_event_set_mods", (self.event, mods))?;
        self.put(0, text.as_bytes())?;
        self.call::<_, ()>(
            "ghostty_key_event_set_utf8",
            (self.event, self.scratch, text.len() as u32),
        )?;
        self.call::<_, ()>(
            "ghostty_key_event_set_unshifted_codepoint",
            (
                self.event,
                if (20..=45).contains(&key) {
                    u32::from(b'a') + key - 20
                } else {
                    0
                },
            ),
        )?;
        self.ok(
            "ghostty_key_encoder_encode",
            (
                self.encoder,
                self.event,
                self.scratch + 2048,
                1024u32,
                self.scratch + 4092,
            ),
        )?;
        let length = self.u32(4092)? as usize;
        if length > 1024 {
            return Err("Invalid libghostty key length".into());
        }
        Ok(self.bytes(2048, length)?.to_vec())
    }

    fn color(
        &mut self,
        name: &str,
        handle: u32,
        kind: u32,
        fallback: [u8; 3],
    ) -> Result<[u8; 3], String> {
        let result: i32 = self.call(name, (handle, kind, self.scratch))?;
        if result == 0 {
            Ok(self.bytes(0, 3)?.try_into().unwrap())
        } else {
            Ok(fallback)
        }
    }

    pub(super) fn frame(&mut self) -> Result<Frame, String> {
        self.fuel()?;
        self.ok("ghostty_render_state_update", (self.render, self.terminal))?;
        let background = self.color("ghostty_render_state_get", self.render, 5, [0; 3])?;
        let foreground = self.color("ghostty_render_state_get", self.render, 6, [220; 3])?;
        let cursor = if self.number("ghostty_render_state_get", self.render, 11)? != 0
            && self.number("ghostty_render_state_get", self.render, 14)? != 0
        {
            Some((
                self.number("ghostty_render_state_get", self.render, 15)?,
                self.number("ghostty_render_state_get", self.render, 16)?,
            ))
        } else {
            None
        };
        self.put(0, &self.iterator.to_le_bytes())?;
        self.ok(
            "ghostty_render_state_get",
            (self.render, 4u32, self.scratch),
        )?;
        let mut lines = Vec::new();
        while self.call::<_, u32>("ghostty_render_state_row_iterator_next", self.iterator)? != 0 {
            if lines.len() >= usize::from(self.rows) {
                return Err("Invalid libghostty row count".into());
            }
            self.put(0, &self.cells.to_le_bytes())?;
            self.ok(
                "ghostty_render_state_row_get",
                (self.iterator, 3u32, self.scratch),
            )?;
            let mut line = Vec::new();
            while self.call::<_, u32>("ghostty_render_state_row_cells_next", self.cells)? != 0 {
                if line.len() >= usize::from(self.cols) {
                    return Err("Invalid libghostty column count".into());
                }
                self.ok(
                    "ghostty_render_state_row_cells_get",
                    (self.cells, 1u32, self.scratch),
                )?;
                let raw = u64::from_le_bytes(self.bytes(0, 8)?.try_into().unwrap());
                self.ok("ghostty_cell_get", (raw, 3u32, self.scratch))?;
                let width = match self.u32(0)? {
                    1 => 2,
                    2 => 0,
                    _ => 1,
                };
                self.ok(
                    "ghostty_render_state_row_cells_get",
                    (self.cells, 3u32, self.scratch),
                )?;
                let length = self.u32(0)? as usize;
                if length > 1024 {
                    return Err("Terminal grapheme exceeds the display limit".into());
                }
                let text = if length == 0 {
                    " ".into()
                } else {
                    self.ok(
                        "ghostty_render_state_row_cells_get",
                        (self.cells, 4u32, self.scratch),
                    )?;
                    self.bytes(0, length * 4)?
                        .chunks_exact(4)
                        .filter_map(|bytes| {
                            char::from_u32(u32::from_le_bytes(bytes.try_into().unwrap()))
                        })
                        .collect()
                };
                let mut fg = self.color(
                    "ghostty_render_state_row_cells_get",
                    self.cells,
                    6,
                    foreground,
                )?;
                let mut bg = self.color(
                    "ghostty_render_state_row_cells_get",
                    self.cells,
                    5,
                    background,
                )?;
                self.put(0, &72u32.to_le_bytes())?;
                self.ok(
                    "ghostty_render_state_row_cells_get",
                    (self.cells, 2u32, self.scratch),
                )?;
                let style = self.bytes(0, 72)?;
                if style[60] != 0 {
                    std::mem::swap(&mut fg, &mut bg);
                }
                if style[61] != 0 {
                    fg = bg;
                }
                line.push(Cell {
                    text,
                    foreground: fg,
                    background: bg,
                    bold: style[56] != 0,
                    underline: style[64] != 0,
                    width,
                });
            }
            lines.push(line);
        }
        Ok(Frame {
            lines,
            cursor,
            background,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ghostty_tracks_unicode_colors_cursor_and_alternate_screen() {
        let mut vt = Ghostty::new(20, 4).unwrap();
        vt.feed("Hello \x1b[31m世界\x1b[0m".as_bytes()).unwrap();
        let frame = vt.frame().unwrap();
        assert_eq!(frame.lines.len(), 4);
        assert_eq!(frame.lines[0][0].text, "H");
        assert_eq!(frame.lines[0][6].text, "世");
        assert_ne!(frame.lines[0][6].foreground, frame.lines[0][0].foreground);
        assert_eq!(frame.cursor, Some((10, 0)));
        vt.feed(b"\x1b[?1049h\x1b[Halt").unwrap();
        assert_eq!(vt.frame().unwrap().lines[0][0].text, "a");
        vt.feed(b"\x1b[?1049l").unwrap();
        assert_eq!(vt.frame().unwrap().lines[0][0].text, "H");
    }

    #[test]
    fn queries_survive_split_frames_and_paste_uses_terminal_mode() {
        let mut vt = Ghostty::new(20, 4).unwrap();
        assert!(vt.feed(b"abc\x1b[").unwrap().is_empty());
        assert_eq!(vt.feed(b"6n").unwrap(), b"\x1b[1;4R");
        vt.feed(b"\x1b[?2004h").unwrap();
        assert_eq!(
            vt.paste("hello\nworld\x1b").unwrap(),
            b"\x1b[200~hello\nworld\x1b[201~"
        );
        assert_eq!(vt.key(22, 2, "c", 1).unwrap(), b"\x03");
        vt.resize(30, 6).unwrap();
        assert_eq!(vt.frame().unwrap().lines.len(), 6);
    }
}
