use crate::SourceError;
pub struct GitFetchPolicy {
    pub(crate) origin: url::Url,
}
impl GitFetchPolicy {
    pub fn new(origin: &str) -> Result<Self, SourceError> {
        let uri = url::Url::parse(origin).map_err(|_| SourceError::InvalidConfiguration)?;
        let local = uri
            .host_str()
            .and_then(|host| host.parse::<std::net::IpAddr>().ok())
            .is_some_and(|ip| ip.is_loopback());
        if origin.len() > 2048
            || uri.origin().ascii_serialization() != origin
            || uri.path() != "/"
            || uri.query().is_some()
            || uri.fragment().is_some()
            || !uri.username().is_empty()
            || uri.password().is_some()
            || !(uri.scheme() == "https" || uri.scheme() == "http" && local)
        {
            return Err(SourceError::InvalidConfiguration);
        }
        Ok(Self { origin: uri })
    }
    pub(crate) fn repository(&self, name: &str) -> Result<url::Url, SourceError> {
        let parts = name.split('/').collect::<Vec<_>>();
        if name.len() > 512
            || !(2..=20).contains(&parts.len())
            || parts.iter().any(|part| {
                part.is_empty()
                    || matches!(*part, "." | "..")
                    || !part
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
            })
        {
            return Err(SourceError::InvalidConfiguration);
        }
        let mut url = self.origin.clone();
        url.set_path(&format!("/{name}.git"));
        Ok(url)
    }
}
