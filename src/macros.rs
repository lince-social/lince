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
macro_rules! log {
    ($error:expr, $($message:expr)*) => {
        $crate::infrastructure::utils::logging::generalog(
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
            $crate::infrastructure::utils::logging::LogType::Info(format!(
                "{}:{} | {}",
                file!(),
                line!(),
                format!($($arg)+)
            )),
            None,
        )
    };
}
