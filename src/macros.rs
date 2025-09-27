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
