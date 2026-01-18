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
        $crate::logging::log(
            $crate::logging::LogEntry::Error($error.kind(), format!(
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
        $crate::logging::log(
            $crate::logging::LogEntry::Info(format!(
                "{}:{} | {}",
                file!(),
                line!(),
                format!($($arg)+)
            ))
        )
    };
}
