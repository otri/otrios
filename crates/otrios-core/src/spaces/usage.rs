// SPDX-License-Identifier: LGPL-3.0-only
//! Count currently admitted participants without exposing participant identities.
use std::collections::BTreeSet;

pub struct AcceptedUser<'a> {
    pub collection: &'a str,
    pub principal: Option<&'a str>,
    pub anonymous: u64,
}
pub fn active_users<'a>(
    participants: impl Iterator<Item = AcceptedUser<'a>>,
    collection: Option<&str>,
) -> u16 {
    #[derive(PartialEq, Eq, PartialOrd, Ord)]
    enum User<'a> {
        Principal(&'a str),
        Anonymous(u64),
    }
    let users = participants
        .filter(|p| collection.is_none_or(|id| p.collection == id))
        .map(|p| match p.principal {
            Some(name) => User::Principal(name),
            None => User::Anonymous(p.anonymous),
        })
        .collect::<BTreeSet<_>>();
    u16::try_from(users.len()).unwrap_or(u16::MAX)
}
