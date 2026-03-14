use maud::html;

pub fn presentation_html_test_page() -> String {
    html!(
            button data-on:click="alert('I’m sorry, Dave. I’m afraid I can’t do that.')" {
                "Open the pod bay doors, HAL."
        }
    )
    .0
}
