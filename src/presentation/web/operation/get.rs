use crate::presentation::web::section::body::presentation_web_section_body_home_modal;

pub fn presentation_web_operation_get_operation_input() -> &'static str {
    r##"
        <div
        hx-target="#body"
                hx-trigger="keyup[key === 'Escape'] from:body"
                hx-get="/section/body"
        >
            <form
                id='operation_input'
                hx-post='/operation'
                hx-target='#body'
                hx-on::after-request="if(event.detail.successful) this.reset(); this.style.display = 'none';"
                style="display: none;"
            >
                <input
                class="modal filled"
                id="operation_input_field" name='operation' autofocus>
                <button type="submit" style="display: none;"></button>
            </form>
        </div>
        <script>
            document.addEventListener('keydown', function (e) {
                const tag = document.activeElement.tagName.toLowerCase();
                if ((tag === 'input' || tag === 'textarea' || document.activeElement.isContentEditable)) return;

                if (e.key.length === 1 && !e.ctrlKey && !e.metaKey && !e.altKey) {
                    const form = document.getElementById('operation_input');
                    const input = document.getElementById('operation_input_field');
                    if (form.style.display === 'none') {
                        form.style.display = 'block';
                        input.value = e.key;
                        input.focus();
                        input.setcollectionRange(input.value.length, input.value.length);
                        e.preventDefault();
                    }
                }
            });
        </script>
    "##
}

pub async fn presentation_web_operation_get_nested_body(element: String) -> String {
    presentation_web_section_body_home_modal(element).await
}
