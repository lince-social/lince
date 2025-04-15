use crate::presentation::web::section::body::nested_body;

pub fn presentation_web_operation_get_operation_input() -> &'static str {
    r##"
        <div>
            <form
                id='operationinput'
                hx-post='/operation'
                hx-target='#body'
                hx-on::after-request="if(event.detail.successful) this.reset()"
            >
                <input name='operation' placeholder='Operation here...' >
                <button type="submit" style="display: none;"></button>
            </form>
        </div>
    "##
}
pub async fn presentation_web_operation_get_nested_body(element: String) -> String {
    nested_body(element).await
}
