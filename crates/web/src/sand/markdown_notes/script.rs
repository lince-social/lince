pub(super) fn script() -> String {
    let mut script = String::from(crate::sand::shared_markdown::JS_HELPERS);
    script.push_str(crate::sand::record_editor::script::EDITOR_RUNTIME);
    script.push_str(
        r##"
      const root = document.querySelector("[data-note-record-editor]");
      const editor = window.LinceRecordEditor.mount(root, { mode: "standalone" });
      root.addEventListener("record-editor:record-created", () => {
        window.LinceWidgetHost?.print?.("note-record-created");
      });
      window.Note = { editor };
    "##,
    );
    script
}
