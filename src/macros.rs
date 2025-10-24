#[macro_export]
macro_rules! fetch_all {
    ($model:ty, $session:expr) => {
        <$model>::find_all()
            .execute(&$session)
            .await
            .map_err(Error::other)?
            .try_collect()
            .await
            .map_err(Error::other)?
    };
}

#[macro_export]
macro_rules! ok {
    ($expression:expr) => {
        $expression.map_err(Error::other)?
    };
}
#[macro_export]
macro_rules! htmx_edit_cell {
    // default tag = td
    ($table:expr, $id:expr, $field:expr, $value:expr) => {
        $crate::htmx_edit_cell!($table, $id, $field, $value, td)
    };
    // explicit tag (e.g. div)
    ($table:expr, $id:expr, $field:expr, $value:expr, $tag:ident) => {{
        let value_str = $value.to_string();
        let post_value = if value_str.is_empty() {
            "NULL".to_string()
        } else {
            value_str.clone()
        };
        let display_value = if value_str.is_empty() {
            "".to_string()
        } else {
            value_str.clone()
        };
        html! {
            $tag {
                form
                    hx-post=(format!("/table/{}/{}/{}", $table, $id, $field))
                    hx-swap="outerHTML"
                    hx-target=(format!("closest {}", stringify!($tag)))
                    hx-trigger="click"
                {
                    input type="hidden" name="value" value=(post_value) {}
                    button type="submit" class="plain-button" {
                        (display_value)
                    }
                }
            }
        }
    }};
}

#[macro_export]
macro_rules! query {
    ($query:expr, $db:expr) => {
        sqlx::query($query)
            .execute(&*$db)
            .await
            .map_err(Error::other)?
    };
}

#[macro_export]
macro_rules! log {
    ($error:expr, $($message:tt)+) => {
        $crate::infrastructure::utils::logging::log(
            $crate::infrastructure::utils::logging::LogEntry::Error($error.kind(), format!(
                "{}:{} | {}",
                file!(),
                line!(),
                format!($($message)+)
            ))
        )
    };
}

#[macro_export]
macro_rules! info {
    ($($arg:tt)+) => {
        $crate::infrastructure::utils::logging::log(
            $crate::infrastructure::utils::logging::LogEntry::Info(format!(
                "{}:{} | {}",
                file!(),
                line!(),
                format!($($arg)+)
            ))
        )
    };
}
