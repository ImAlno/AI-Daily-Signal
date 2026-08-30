use url::Url;

use crate::{Result, SignalError};

pub fn display_safe_url(value: &str) -> Result<String> {
    let mut url = Url::parse(value).map_err(|_| invalid_projection())?;
    url.set_username("").map_err(|_| invalid_projection())?;
    url.set_password(None).map_err(|_| invalid_projection())?;
    url.set_query(None);
    url.set_fragment(None);
    Ok(url.to_string())
}

fn invalid_projection() -> SignalError {
    SignalError::InvalidConfiguration("URL cannot be projected safely".to_owned())
}
