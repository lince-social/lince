const ESC = "\u001b";

export function createQueryReplyInterpreter(runtime) {
  const state = {
    active: false,
    bytes: [],
    kind: 0,
    oscSawEsc: false,
  };

  function reset() {
    state.active = false;
    state.bytes = [];
    state.kind = 0;
    state.oscSawEsc = false;
  }

  function complete(sequence) {
    return {
      boundary: true,
      sequence,
    };
  }

  return {
    feedByte(byte) {
      if (!state.active) {
        if (byte === 0x05) {
          return complete("\u0005");
        }

        if (byte !== 0x1b) {
          return null;
        }

        state.active = true;
        state.bytes = [byte];
        state.kind = 0;
        state.oscSawEsc = false;
        return null;
      }

      state.bytes.push(byte);
      if (state.bytes.length === 2) {
        state.kind = byte;
        if (byte !== 0x5b && byte !== 0x5d) {
          const result = complete(bytesToString(state.bytes));
          reset();
          return result;
        }
        return null;
      }

      if (state.kind === 0x5d) {
        if (byte === 0x07 || (state.oscSawEsc && byte === 0x5c)) {
          const result = complete(bytesToString(state.bytes));
          reset();
          return result;
        }

        state.oscSawEsc = byte === 0x1b;
        return null;
      }

      if (byte >= 0x40 && byte <= 0x7e) {
        const result = complete(bytesToString(state.bytes));
        reset();
        return result;
      }

      return null;
    },
    replyFor(sequence) {
      return buildReply(sequence, runtime);
    },
    reset,
  };
}

function bytesToString(bytes) {
  return String.fromCharCode(...bytes);
}

function buildReply(sequence, runtime) {
  const terminal = runtime.queryState();

  switch (sequence) {
    case `${ESC}[5n`:
      return `${ESC}[0n`;
    case `${ESC}[6n`:
      return `${ESC}[${terminal.cursorY + 1};${terminal.cursorX + 1}R`;
    case `${ESC}[?6n`:
      return `${ESC}[?${terminal.cursorY + 1};${terminal.cursorX + 1}R`;
    case `${ESC}[18t`:
      return `${ESC}[8;${terminal.rows};${terminal.cols}t`;
    case `${ESC}[14t`:
      return `${ESC}[4;${terminal.rows * 16};${terminal.cols * 8}t`;
    case `${ESC}[16t`:
      return `${ESC}[6;16;8t`;
    case "\u0005":
      return null;
    default:
      return null;
  }
}
