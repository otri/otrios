// SPDX-License-Identifier: LGPL-3.0-only
//! Conservative public-HTTPS policy for immutable app sources. No URL is trust.
use super::*;
use std::net::{IpAddr, Ipv6Addr};

/// This host permits public HTTPS on port 443 only. Deliberately excludes local
/// networks, transition mechanisms and special-purpose ranges. A resolver's
/// entire answer must pass before the adapter connects to any pinned address.
pub fn public_address(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(ip) => {
            let [a, b, c, _] = ip.octets();
            !(matches!(a, 0 | 10 | 127 | 224..=255)
                || a == 100 && (64..=127).contains(&b)
                || a == 169 && b == 254
                || a == 172 && (16..=31).contains(&b)
                || a == 192 && (b == 0 && matches!(c, 0 | 2) || b == 168 || b == 88 && c == 99)
                || a == 198 && (b == 18 || b == 19 || b == 51 && c == 100)
                || a == 203 && b == 0 && c == 113)
        }
        IpAddr::V6(ip) => {
            let s = ip.segments();
            s[0] & 0xe000 == 0x2000
                && !(s[0] == 0x2001 && (s[1] < 0x0200 || s[1] == 0x0db8)
                    || s[0] == 0x2002
                    || s[0] == 0x3fff && s[1] < 0x1000)
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppSource {
    host: String,
    authority: String,
    app: String,
    digest: String,
}
impl AppSource {
    pub fn parse(url: &str) -> Result<Self> {
        if url.len() > 512 || !url.is_ascii() || url.bytes().any(|b| b.is_ascii_control()) {
            return Err("App source URL budget or syntax".into());
        }
        let (authority, path) = url
            .strip_prefix("https://")
            .and_then(|s| s.split_once('/'))
            .ok_or("Use an HTTPS immutable app page URL")?;
        let host = if let Some(ip) = authority
            .strip_prefix('[')
            .and_then(|s| s.strip_suffix(']'))
        {
            ip.parse::<Ipv6Addr>()
                .map_err(|_| "Invalid app source host")?;
            ip.to_string()
        } else {
            if authority.is_empty()
                || authority.len() > 253
                || authority.split('.').any(|s| {
                    s.is_empty()
                        || s.len() > 63
                        || s.starts_with('-')
                        || s.ends_with('-')
                        || !s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
                })
            {
                return Err("App source requires a plain HTTPS host on port 443".into());
            }
            authority.to_ascii_lowercase()
        };
        let fields: Vec<_> = path.split('/').collect();
        if !matches!(fields.len(), 3 | 4)
            || fields[0] != "apps"
            || fields.len() == 4 && fields[3] != "download"
            || !otrios_interfaces::canvas::valid_destination(fields[1])
            || fields[1].is_empty()
            || fields[1].contains('/')
            || fields[2].len() != 64
            || !fields[2]
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err("App source must name an immutable app and SHA-256 digest".into());
        }
        Ok(Self {
            host,
            authority: authority.to_ascii_lowercase(),
            app: fields[1].into(),
            digest: fields[2].into(),
        })
    }
    pub fn host(&self) -> &str {
        &self.host
    }
    pub fn app(&self) -> &str {
        &self.app
    }
    pub fn digest(&self) -> &str {
        &self.digest
    }
    pub fn download_url(&self) -> String {
        format!(
            "https://{}/apps/{}/{}/download",
            self.authority, self.app, self.digest
        )
    }
    pub fn request(&self) -> String {
        format!(
            "GET /apps/{}/{}/download HTTP/1.1\r\nHost: {}\r\nAccept: {}\r\nAccept-Encoding: identity\r\nConnection: close\r\n\r\n",
            self.app, self.digest, self.authority, APP_MEDIA_TYPE
        )
    }
    pub fn redirect(&self, location: &str) -> Result<Self> {
        let absolute = if location.starts_with("/apps/") {
            format!("https://{}{location}", self.authority)
        } else {
            location.to_string()
        };
        let next = Self::parse(&absolute)?;
        if next.app != self.app || next.digest != self.digest {
            return Err("App redirect changed the named identity".into());
        }
        Ok(next)
    }
    pub fn check_bundle(&self, bundle: &crate::bundle::InspectedBundle) -> Result<()> {
        if bundle.info.app.id != self.app || bundle.digest != self.digest {
            return Err("Downloaded app differs from the immutable URL".into());
        }
        Ok(())
    }
    pub fn addresses(&self, addresses: &[IpAddr]) -> Result<()> {
        if addresses.is_empty()
            || addresses.len() > 8
            || addresses.iter().any(|ip| !public_address(*ip))
        {
            return Err("App source destination rejected by public HTTPS policy".into());
        }
        Ok(())
    }
}
