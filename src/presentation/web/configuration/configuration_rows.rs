pub fn configuration_rows(configurations: Vec<(String, i32)>) -> String {
    let mut html = String::new();

    for configuration in configurations {
        let (configuration_name, configuration_quantity) = configuration;

        html.push_str(&format!(
        r#"<button style="background-color: {}; padding: 10px; border: none; border-radius: 5px;">{}</button>"#,
        if configuration_quantity == 1 { "red"
        } else { "lightgray" },
        configuration_name ).to_string()
        )
    }
    html
}
