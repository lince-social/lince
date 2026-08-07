#[macro_export]
macro_rules! ok {
    ($expression:expr) => {
        $expression.map_err(std::io::Error::other)?
    };
}
