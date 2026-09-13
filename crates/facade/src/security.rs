use crate::auth::Failure;
use axum::http::{HeaderMap, StatusCode, Uri, header};

#[derive(Clone)]
pub(crate) struct Security {
    origin: String,
    authority: String,
    pub(crate) public: bool,
}

impl Security {
    pub(crate) fn new(origin: &str, public: bool) -> Result<Self, std::io::Error> {
        let invalid = || {
            std::io::Error::other(
                "Facade needs a public HTTPS origin such as https://lince.mycompany.mydomain",
            )
        };
        let uri: Uri = origin.parse().map_err(|_| invalid())?;
        let authority = uri.authority().ok_or_else(invalid)?.as_str();
        if uri.scheme_str() != Some(if public { "https" } else { "http" })
            || uri.path() != "/"
            || uri.query().is_some()
            || authority.contains('@')
        {
            return Err(invalid());
        }
        Ok(Self {
            origin: format!("{}://{authority}", uri.scheme_str().unwrap()),
            authority: authority.into(),
            public,
        })
    }

    pub(crate) fn request(&self, headers: &HeaderMap) -> Result<(), Failure> {
        if headers
            .get(header::HOST)
            .and_then(|value| value.to_str().ok())
            != Some(self.authority.as_str())
            || (self.public
                && headers
                    .get("x-forwarded-proto")
                    .and_then(|value| value.to_str().ok())
                    != Some("https"))
        {
            return Err((
                StatusCode::FORBIDDEN,
                "Use the configured Facade address".into(),
            ));
        }
        Ok(())
    }

    pub(crate) fn same_origin(&self, headers: &HeaderMap) -> Result<(), Failure> {
        self.request(headers)?;
        if headers
            .get(header::ORIGIN)
            .and_then(|value| value.to_str().ok())
            != Some(self.origin.as_str())
            || headers
                .get("sec-fetch-site")
                .is_some_and(|value| value != "same-origin" && value != "none")
        {
            return Err((
                StatusCode::FORBIDDEN,
                "Open Facade from its own address".into(),
            ));
        }
        Ok(())
    }

    pub(crate) fn cookie_name(&self) -> &'static str {
        if self.public {
            "__Host-lince_facade"
        } else {
            "lince_facade"
        }
    }

    pub(crate) fn cookie(&self, token: &str, seconds: u64) -> String {
        format!(
            "{}={token}; Path=/; HttpOnly; SameSite=Strict; Max-Age={seconds}{}",
            self.cookie_name(),
            if self.public { "; Secure" } else { "" }
        )
    }
}
