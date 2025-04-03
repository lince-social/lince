use axum::{extract::Path, response::Html};

use crate::{
    application::providers::operation::execute_operation::execute_operation,
    presentation::web::section::body::nested_body,
};

pub fn get_operation() -> Html<String> {
    Html(
        r"
            <div>
                <form>
                    <input
                        id='operationinput'
                        name='operation'
                        placeholder='Operation here...'
                        hx-post='/operation'
                        hx-target='#body'
                    >
                </form>
            </div>
        "
        .to_string(),
    )
}
pub async fn post_operation(operation: String) -> Html<String> {
    println!("operation: {operation}");
    let element = execute_operation(operation).await;
    if element.is_err() {
        let error_element = operation_error_element();
        return nested_body(error_element).await;
    }
    nested_body(element.unwrap()).await
}

pub fn operation_error_element() -> String {
    "<div>Error when performing operation</div>".to_string()
}
//   const HandledOperation = await HandleOperation(operation);
//   return (
//     <FatherBody>
//       <div class="fixed inset-0 z-30 flex items-center justify-center">
//         <div
//           class="flex align-center rounded justify-center focus:outline-none focus:ring-0"
//           hx-get="/"
//           hx-trigger="keydown[key === 'Escape'] from:body"
//           hx-target="#body"
//         >
//           (Press Escape to remove)
//           {HandledOperation}
//         </div>
//       </div>
//     </FatherBody>
//   );
// }
