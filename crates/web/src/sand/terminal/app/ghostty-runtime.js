import { getUnshiftedCodepoint, keyCodeMap } from "./keymap.js";

const SUCCESS = 0;
const OUT_OF_SPACE = -3;
const FORMAT_HTML = 2;

const TERMINAL_DATA = {
  COLS: 1,
  ROWS: 2,
  CURSOR_X: 3,
  CURSOR_Y: 4,
  CURSOR_VISIBLE: 7,
  CURSOR_STYLE: 10,
  TOTAL_ROWS: 14,
  SCROLLBACK_ROWS: 15,
};

const encoder = new TextEncoder();
const decoder = new TextDecoder();

export class GhosttyRuntime {
  static async create(wasmUrl) {
    const response = await fetch(wasmUrl);
    if (!response.ok) {
      throw new Error(`Falha ao carregar ${wasmUrl}.`);
    }

    const bytes = await response.arrayBuffer();
    const module = await WebAssembly.instantiate(bytes, {
      env: {
        log: () => {},
      },
    });

    const runtime = new GhosttyRuntime(module.instance.exports);
    runtime.loadTypeLayout();
    runtime.createKeyEncoder();
    return runtime;
  }

  constructor(exports) {
    this.exports = exports;
    this.memory = exports.memory;
    this.typeLayout = null;
    this.terminal = 0;
    this.formatter = 0;
    this.keyEncoder = 0;
  }

  destroy() {
    if (this.keyEncoder) {
      this.exports.ghostty_key_encoder_free(this.keyEncoder);
      this.keyEncoder = 0;
    }

    if (this.formatter) {
      this.exports.ghostty_formatter_free(this.formatter);
      this.formatter = 0;
    }

    if (this.terminal) {
      this.exports.ghostty_terminal_free(this.terminal);
      this.terminal = 0;
    }
  }

  resetTerminal(geometry) {
    if (!this.typeLayout) {
      throw new Error("Ghostty type layout indisponivel.");
    }

    if (this.formatter) {
      this.exports.ghostty_formatter_free(this.formatter);
      this.formatter = 0;
    }

    if (this.terminal) {
      this.exports.ghostty_terminal_free(this.terminal);
      this.terminal = 0;
    }

    const termOptionsSize = this.typeLayout.GhosttyTerminalOptions.size;
    const optionsPtr = this.exports.ghostty_wasm_alloc_u8_array(termOptionsSize);
    new Uint8Array(this.buffer(), optionsPtr, termOptionsSize).fill(0);
    const optionsView = new DataView(this.buffer(), optionsPtr, termOptionsSize);
    this.setField(optionsView, "GhosttyTerminalOptions", "cols", geometry.cols);
    this.setField(optionsView, "GhosttyTerminalOptions", "rows", geometry.rows);
    this.setField(optionsView, "GhosttyTerminalOptions", "max_scrollback", 3000);

    const terminalPtrPtr = this.exports.ghostty_wasm_alloc_opaque();
    const result = this.exports.ghostty_terminal_new(0, terminalPtrPtr, optionsPtr);
    this.exports.ghostty_wasm_free_u8_array(optionsPtr, termOptionsSize);
    if (result !== SUCCESS) {
      this.exports.ghostty_wasm_free_opaque(terminalPtrPtr);
      throw new Error(`ghostty_terminal_new falhou: ${result}`);
    }

    this.terminal = this.readOpaquePointer(terminalPtrPtr);
    this.exports.ghostty_wasm_free_opaque(terminalPtrPtr);
    this.resize(geometry);
    this.createFormatter();
    this.syncKeyEncoder();
  }

  resize(geometry) {
    if (!this.terminal) {
      return;
    }

    const result = this.exports.ghostty_terminal_resize(
      this.terminal,
      geometry.cols,
      geometry.rows,
      geometry.cellWidth,
      geometry.cellHeight,
    );
    if (result !== SUCCESS) {
      throw new Error(`ghostty_terminal_resize falhou: ${result}`);
    }
    this.syncKeyEncoder();
  }

  writeBytes(bytes) {
    if (!this.terminal || !(bytes instanceof Uint8Array) || bytes.length === 0) {
      return;
    }

    const dataPtr = this.exports.ghostty_wasm_alloc_u8_array(bytes.length);
    new Uint8Array(this.buffer()).set(bytes, dataPtr);
    this.exports.ghostty_terminal_vt_write(this.terminal, dataPtr, bytes.length);
    this.exports.ghostty_wasm_free_u8_array(dataPtr, bytes.length);
    this.syncKeyEncoder();
  }

  formatHtml() {
    if (!this.formatter) {
      return { css: "", html: "" };
    }

    const outPtrPtr = this.exports.ghostty_wasm_alloc_opaque();
    const outLenPtr = this.exports.ghostty_wasm_alloc_usize();
    const result = this.exports.ghostty_formatter_format_alloc(
      this.formatter,
      0,
      outPtrPtr,
      outLenPtr,
    );

    if (result !== SUCCESS) {
      this.exports.ghostty_wasm_free_opaque(outPtrPtr);
      this.exports.ghostty_wasm_free_usize(outLenPtr);
      throw new Error(`ghostty_formatter_format_alloc falhou: ${result}`);
    }

    const outPtr = this.readOpaquePointer(outPtrPtr);
    const outLen = this.readUsize(outLenPtr);
    const formatted = decoder.decode(new Uint8Array(this.buffer(), outPtr, outLen));

    this.exports.ghostty_free(0, outPtr, outLen);
    this.exports.ghostty_wasm_free_opaque(outPtrPtr);
    this.exports.ghostty_wasm_free_usize(outLenPtr);

    return splitFormattedHtml(formatted);
  }

  queryState() {
    if (!this.terminal) {
      return {
        cols: 0,
        rows: 0,
        cursorX: 0,
        cursorY: 0,
        cursorVisible: false,
        totalRows: 0,
        scrollbackRows: 0,
      };
    }

    return {
      cols: this.getU16(TERMINAL_DATA.COLS),
      rows: this.getU16(TERMINAL_DATA.ROWS),
      cursorX: this.getU16(TERMINAL_DATA.CURSOR_X),
      cursorY: this.getU16(TERMINAL_DATA.CURSOR_Y),
      cursorVisible: this.getBool(TERMINAL_DATA.CURSOR_VISIBLE),
      totalRows: this.getU64(TERMINAL_DATA.TOTAL_ROWS),
      scrollbackRows: this.getU64(TERMINAL_DATA.SCROLLBACK_ROWS),
    };
  }

  encodeKeyboardEvent(event, action) {
    if (!this.keyEncoder) {
      return new Uint8Array(0);
    }

    this.syncKeyEncoder();

    const eventPtrPtr = this.exports.ghostty_wasm_alloc_opaque();
    let utf8Ptr = 0;
    let utf8Length = 0;

    try {
      const createResult = this.exports.ghostty_key_event_new(0, eventPtrPtr);
      if (createResult !== SUCCESS) {
        throw new Error(`ghostty_key_event_new falhou: ${createResult}`);
      }

      const eventPtr = this.readOpaquePointer(eventPtrPtr);
      this.exports.ghostty_key_event_set_action(eventPtr, action);
      this.exports.ghostty_key_event_set_key(eventPtr, keyCodeMap[event.code] || 0);
      this.exports.ghostty_key_event_set_mods(eventPtr, buildMods(event));

      if (typeof event.key === "string" && event.key.length === 1) {
        const utf8Bytes = encoder.encode(event.key);
        utf8Length = utf8Bytes.length;
        utf8Ptr = this.exports.ghostty_wasm_alloc_u8_array(utf8Length);
        new Uint8Array(this.buffer()).set(utf8Bytes, utf8Ptr);
        this.exports.ghostty_key_event_set_utf8(eventPtr, utf8Ptr, utf8Length);
      }

      const unshifted = getUnshiftedCodepoint(event);
      if (unshifted) {
        this.exports.ghostty_key_event_set_unshifted_codepoint(eventPtr, unshifted);
      }

      const requiredPtr = this.exports.ghostty_wasm_alloc_usize();
      const requiredResult = this.exports.ghostty_key_encoder_encode(
        this.keyEncoder,
        eventPtr,
        0,
        0,
        requiredPtr,
      );
      const required = this.readUsize(requiredPtr);
      this.exports.ghostty_wasm_free_usize(requiredPtr);

      if (required === 0) {
        this.exports.ghostty_key_event_free(eventPtr);
        return new Uint8Array(0);
      }

      if (requiredResult !== OUT_OF_SPACE && requiredResult !== SUCCESS) {
        this.exports.ghostty_key_event_free(eventPtr);
        return new Uint8Array(0);
      }

      const bufferPtr = this.exports.ghostty_wasm_alloc_u8_array(required);
      const writtenPtr = this.exports.ghostty_wasm_alloc_usize();
      const encodeResult = this.exports.ghostty_key_encoder_encode(
        this.keyEncoder,
        eventPtr,
        bufferPtr,
        required,
        writtenPtr,
      );
      const written = this.readUsize(writtenPtr);
      const bytes = new Uint8Array(this.buffer().slice(bufferPtr, bufferPtr + written));

      this.exports.ghostty_wasm_free_u8_array(bufferPtr, required);
      this.exports.ghostty_wasm_free_usize(writtenPtr);
      this.exports.ghostty_key_event_free(eventPtr);

      if (encodeResult !== SUCCESS) {
        return new Uint8Array(0);
      }

      return bytes;
    } finally {
      if (utf8Ptr && utf8Length) {
        this.exports.ghostty_wasm_free_u8_array(utf8Ptr, utf8Length);
      }
      this.exports.ghostty_wasm_free_opaque(eventPtrPtr);
    }
  }

  loadTypeLayout() {
    const jsonPtr = this.exports.ghostty_type_json();
    const json = decoder
      .decode(new Uint8Array(this.buffer(), jsonPtr))
      .split("\0")[0];
    this.typeLayout = JSON.parse(json);
  }

  createFormatter() {
    const formatterOptionsSize = this.typeLayout.GhosttyFormatterTerminalOptions.size;
    const formatterOptionsPtr = this.exports.ghostty_wasm_alloc_u8_array(formatterOptionsSize);
    new Uint8Array(this.buffer(), formatterOptionsPtr, formatterOptionsSize).fill(0);
    const formatterView = new DataView(
      this.buffer(),
      formatterOptionsPtr,
      formatterOptionsSize,
    );

    this.setField(
      formatterView,
      "GhosttyFormatterTerminalOptions",
      "size",
      formatterOptionsSize,
    );
    this.setField(
      formatterView,
      "GhosttyFormatterTerminalOptions",
      "emit",
      FORMAT_HTML,
    );
    this.setField(
      formatterView,
      "GhosttyFormatterTerminalOptions",
      "unwrap",
      false,
    );
    this.setField(
      formatterView,
      "GhosttyFormatterTerminalOptions",
      "trim",
      false,
    );

    const extraOffset =
      this.fieldInfo("GhosttyFormatterTerminalOptions", "extra").offset;
    const extraSize = this.typeLayout.GhosttyFormatterTerminalExtra.size;
    const extraView = new DataView(this.buffer(), formatterOptionsPtr + extraOffset, extraSize);
    this.setField(extraView, "GhosttyFormatterTerminalExtra", "size", extraSize);
    this.setField(extraView, "GhosttyFormatterTerminalExtra", "palette", true);

    const screenOffset =
      this.fieldInfo("GhosttyFormatterTerminalExtra", "screen").offset;
    const screenSize = this.typeLayout.GhosttyFormatterScreenExtra.size;
    const screenView = new DataView(
      this.buffer(),
      formatterOptionsPtr + extraOffset + screenOffset,
      screenSize,
    );
    this.setField(screenView, "GhosttyFormatterScreenExtra", "size", screenSize);

    const formatterPtrPtr = this.exports.ghostty_wasm_alloc_opaque();
    const result = this.exports.ghostty_formatter_terminal_new(
      0,
      formatterPtrPtr,
      this.terminal,
      formatterOptionsPtr,
    );
    this.exports.ghostty_wasm_free_u8_array(formatterOptionsPtr, formatterOptionsSize);
    if (result !== SUCCESS) {
      this.exports.ghostty_wasm_free_opaque(formatterPtrPtr);
      throw new Error(`ghostty_formatter_terminal_new falhou: ${result}`);
    }

    this.formatter = this.readOpaquePointer(formatterPtrPtr);
    this.exports.ghostty_wasm_free_opaque(formatterPtrPtr);
  }

  createKeyEncoder() {
    if (this.keyEncoder) {
      return;
    }

    const encoderPtrPtr = this.exports.ghostty_wasm_alloc_opaque();
    const result = this.exports.ghostty_key_encoder_new(0, encoderPtrPtr);
    if (result !== SUCCESS) {
      this.exports.ghostty_wasm_free_opaque(encoderPtrPtr);
      throw new Error(`ghostty_key_encoder_new falhou: ${result}`);
    }

    this.keyEncoder = this.readOpaquePointer(encoderPtrPtr);
    this.exports.ghostty_wasm_free_opaque(encoderPtrPtr);
  }

  syncKeyEncoder() {
    if (!this.keyEncoder || !this.terminal) {
      return;
    }

    this.exports.ghostty_key_encoder_setopt_from_terminal(
      this.keyEncoder,
      this.terminal,
    );
  }

  fieldInfo(structName, fieldName) {
    return this.typeLayout[structName].fields[fieldName];
  }

  setField(view, structName, fieldName, value) {
    const field = this.fieldInfo(structName, fieldName);
    switch (field.type) {
      case "u8":
      case "bool":
        view.setUint8(field.offset, value ? 1 : 0);
        break;
      case "u16":
        view.setUint16(field.offset, value, true);
        break;
      case "u32":
      case "enum":
      case "usize":
        view.setUint32(field.offset, value, true);
        break;
      case "u64":
        view.setBigUint64(field.offset, BigInt(value), true);
        break;
      default:
        throw new Error(`Tipo nao suportado: ${field.type}`);
    }
  }

  getU16(kind) {
    const ptr = this.exports.ghostty_wasm_alloc_u16_array(1);
    const result = this.exports.ghostty_terminal_get(this.terminal, kind, ptr);
    const value = result === SUCCESS ? new DataView(this.buffer()).getUint16(ptr, true) : 0;
    this.exports.ghostty_wasm_free_u16_array(ptr, 1);
    return value;
  }

  getBool(kind) {
    const ptr = this.exports.ghostty_wasm_alloc_u8();
    const result = this.exports.ghostty_terminal_get(this.terminal, kind, ptr);
    const value = result === SUCCESS ? new DataView(this.buffer()).getUint8(ptr) !== 0 : false;
    this.exports.ghostty_wasm_free_u8(ptr);
    return value;
  }

  getU64(kind) {
    const ptr = this.exports.ghostty_wasm_alloc_u8_array(8);
    const result = this.exports.ghostty_terminal_get(this.terminal, kind, ptr);
    const value =
      result === SUCCESS ? Number(new DataView(this.buffer()).getBigUint64(ptr, true)) : 0;
    this.exports.ghostty_wasm_free_u8_array(ptr, 8);
    return value;
  }

  readOpaquePointer(ptr) {
    return new DataView(this.buffer()).getUint32(ptr, true);
  }

  readUsize(ptr) {
    return new DataView(this.buffer()).getUint32(ptr, true);
  }

  buffer() {
    return this.memory.buffer;
  }
}

function buildMods(event) {
  let mods = 0;
  if (event.shiftKey) {
    mods |= 0x01;
    if (event.code === "ShiftRight") {
      mods |= 0x40;
    }
  }
  if (event.ctrlKey) {
    mods |= 0x02;
    if (event.code === "ControlRight") {
      mods |= 0x80;
    }
  }
  if (event.altKey) {
    mods |= 0x04;
    if (event.code === "AltRight") {
      mods |= 0x100;
    }
  }
  if (event.metaKey) {
    mods |= 0x08;
    if (event.code === "MetaRight") {
      mods |= 0x200;
    }
  }
  return mods;
}

function splitFormattedHtml(formatted) {
  const match = formatted.match(/^<style>([\s\S]*?)<\/style>([\s\S]*)$/);
  if (!match) {
    return {
      css: "",
      html: formatted,
    };
  }

  return {
    css: match[1],
    html: match[2],
  };
}
