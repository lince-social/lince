use crate::presentation::web::section::body::nested_body;

pub fn presentation_web_operation_get_operation_input() -> String {
    r##"
            <div>
                <form
                        id='operationinput'
                        hx-post='/operation'
                        hx-target='#body'
                        hx-on::after-request="if(event.detail.successful) this.reset()"
                >
                    <input
                        name='operation'
                        placeholder='Operation here...'
                    >
                     <button type="submit" style="display: none;"></button>
                </form>
            </div>
        "##
    .to_string()
}
pub async fn presentation_web_operation_get_nested_body(element: String) -> String {
    nested_body(element).await
}
