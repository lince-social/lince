pub async fn presentation_web_page_testing_presentation() -> &'static str {
    r#"
   <input data-bind-foo />
    <div data-show="$foo != ''">
        <input data-bind-input />
        <div data-class-framed="$input != ''">Teste</div>
            <div data-computed-repeated="$input.repeat(2)">
            <div data-text="$repeated">
            </div>
        </div>
    </div>
   "#
}
