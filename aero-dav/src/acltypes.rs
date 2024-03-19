use super::types as dav;

//RFC covered: RFC3744 (ACL core) + RFC5397 (ACL Current Principal Extension)


//@FIXME required for a full CalDAV implementation
// See section 6. of the CalDAV RFC
// It seems mainly required for free-busy that I will not implement now.
// It can also be used for discovering main calendar, not sure it is used.
// Note: it is used by Thunderbird


#[derive(Debug, PartialEq, Clone)]
pub enum PropertyRequest {
    Owner,
    CurrentUserPrincipal,
    CurrentUserPrivilegeSet,
}

#[derive(Debug, PartialEq, Clone)]
pub enum Property {
    Owner(dav::Href),
    CurrentUserPrincipal(User),
    CurrentUserPrivilegeSet(Vec<Privilege>),
}

#[derive(Debug, PartialEq, Clone)]
pub enum ResourceType {
    Principal,
}

/// Not implemented, it's a placeholder
#[derive(Debug, PartialEq, Clone)]
pub struct Privilege(());

#[derive(Debug, PartialEq, Clone)]
pub enum User {
    Unauthenticated,
    Authenticated(dav::Href),
}
