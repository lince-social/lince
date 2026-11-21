// non-lean-yet
// Scope: struct Function, macro exports.
// BeautifulRust: No site flagged by this pass. Check contracts and feature coverage.

use wasmi::{Instance, Store, StoreLimits, TypedFunc, WasmParams, WasmResults};

#[derive(Clone, Copy)]
pub(super) struct Function<P, R> {
    pub(super) name: &'static str,
    function: TypedFunc<P, R>,
}

impl<P: WasmParams, R: WasmResults> Function<P, R> {
    fn bind(
        store: &Store<StoreLimits>,
        instance: &Instance,
        name: &'static str,
    ) -> Result<Self, String> {
        let function = instance
            .get_typed_func(store, name)
            .map_err(|error| format!("{name}: {error}"))?;
        Ok(Self { name, function })
    }

    pub(super) fn call(self, store: &mut Store<StoreLimits>, args: P) -> Result<R, String> {
        self.function
            .call(store, args)
            .map_err(|error| format!("{}: {error}", self.name))
    }
}

macro_rules! exports {
    ($($field:ident: $params:ty => $result:ty = $name:literal;)*) => {
        pub(super) struct Api {
            $(pub(super) $field: Function<$params, $result>,)*
        }

        impl Api {
            pub(super) fn bind(store: &Store<StoreLimits>, instance: &Instance) -> Result<Self, String> {
                Ok(Self { $($field: Function::bind(store, instance, $name)?,)* })
            }
        }
    };
}

exports! {
    alloc: u32 => u32 = "ghostty_wasm_alloc_u8_array";
    type_json: () => u32 = "ghostty_type_json";
    terminal_new: (u32, u32, u32) => i32 = "ghostty_terminal_new";
    render_new: (u32, u32) => i32 = "ghostty_render_state_new";
    iterator_new: (u32, u32) => i32 = "ghostty_render_state_row_iterator_new";
    cells_new: (u32, u32) => i32 = "ghostty_render_state_row_cells_new";
    encoder_new: (u32, u32) => i32 = "ghostty_key_encoder_new";
    event_new: (u32, u32) => i32 = "ghostty_key_event_new";
    terminal_get: (u32, u32, u32) => i32 = "ghostty_terminal_get";
    terminal_mode_get: (u32, u32, u32) => i32 = "ghostty_terminal_mode_get";
    terminal_resize: (u32, u32, u32, u32, u32) => i32 = "ghostty_terminal_resize";
    terminal_write: (u32, u32, u32) => () = "ghostty_terminal_vt_write";
    terminal_scroll: (u32, u32) => () = "ghostty_terminal_scroll_viewport";
    encoder_options: (u32, u32) => () = "ghostty_key_encoder_setopt_from_terminal";
    event_action: (u32, u32) => () = "ghostty_key_event_set_action";
    event_key: (u32, u32) => () = "ghostty_key_event_set_key";
    event_mods: (u32, u32) => () = "ghostty_key_event_set_mods";
    event_utf8: (u32, u32, u32) => () = "ghostty_key_event_set_utf8";
    event_codepoint: (u32, u32) => () = "ghostty_key_event_set_unshifted_codepoint";
    encode: (u32, u32, u32, u32, u32) => i32 = "ghostty_key_encoder_encode";
    render_update: (u32, u32) => i32 = "ghostty_render_state_update";
    render_get: (u32, u32, u32) => i32 = "ghostty_render_state_get";
    iterator_next: u32 => u32 = "ghostty_render_state_row_iterator_next";
    row_get: (u32, u32, u32) => i32 = "ghostty_render_state_row_get";
    cells_next: u32 => u32 = "ghostty_render_state_row_cells_next";
    cells_get: (u32, u32, u32) => i32 = "ghostty_render_state_row_cells_get";
    cell_get: (u64, u32, u32) => i32 = "ghostty_cell_get";
}
