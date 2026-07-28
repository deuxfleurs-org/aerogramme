use quick_xml::events::Event;

use super::acltypes::*;
use super::error::ParsingError;
use super::coretypes as dav;
use super::xml::{IRead, QRead, Reader, DAV_URN};

impl QRead<Property> for Property {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        if xml.maybe_open_start(DAV_URN, "owner").await?.is_some() {
            let href = xml.find().await?;
            xml.close().await?;
            return Ok(Self::Owner(href));
        }
        if xml
            .maybe_open_start(DAV_URN, "current-user-principal")
            .await?
            .is_some()
        {
            let user = xml.find().await?;
            xml.close().await?;
            return Ok(Self::CurrentUserPrincipal(user));
        }
        if xml
            .maybe_open_start(DAV_URN, "current-user-privilege-set")
            .await?
            .is_some()
        {
            let privilegeset = xml.find().await?;
            xml.close().await?;
            return Ok(Self::CurrentUserPrivilegeSet(privilegeset))
        }

        Err(ParsingError::Recoverable)
    }
}

impl QRead<Violation> for Violation {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        if xml
            .maybe_open(DAV_URN, "number-of-matches-within-limits")
            .await?
            .is_some()
        {
            xml.close().await?;
            Ok(Self::NumberOfMatchesWithinLimits)
        } else {
            Err(ParsingError::Recoverable)
        }
    }
}

impl QRead<PropertyRequest> for PropertyRequest {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        if xml.maybe_open(DAV_URN, "owner").await?.is_some() {
            xml.close().await?;
            return Ok(Self::Owner);
        }

        if xml
            .maybe_open(DAV_URN, "current-user-principal")
            .await?
            .is_some()
        {
            xml.close().await?;
            return Ok(Self::CurrentUserPrincipal);
        }

        if xml
            .maybe_open(DAV_URN, "current-user-privilege-set")
            .await?
            .is_some()
        {
            xml.close().await?;
            return Ok(Self::CurrentUserPrivilegeSet);
        }

        Err(ParsingError::Recoverable)
    }
}

impl QRead<ResourceType> for ResourceType {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        if xml.maybe_open(DAV_URN, "principal").await?.is_some() {
            xml.close().await?;
            return Ok(Self::Principal);
        }
        Err(ParsingError::Recoverable)
    }
}

impl QRead<Privilege> for Privilege {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        if xml.maybe_open(DAV_URN, "read").await?.is_some() {
            xml.close().await?;
            return Ok(Self::Read);
        }
        if xml.maybe_open(DAV_URN, "write").await?.is_some() {
            xml.close().await?;
            return Ok(Self::Write);
        }
        if xml.maybe_open(DAV_URN, "write-properties").await?.is_some() {
            xml.close().await?;
            return Ok(Self::WriteProperties);
        }
        if xml.maybe_open(DAV_URN, "write-content").await?.is_some() {
            xml.close().await?;
            return Ok(Self::WriteContent);
        }
        if xml.maybe_open(DAV_URN, "unlock").await?.is_some() {
            xml.close().await?;
            return Ok(Self::Unlock);
        }
        if xml.maybe_open(DAV_URN, "read-acl").await?.is_some() {
            xml.close().await?;
            return Ok(Self::ReadAcl);
        }
        if xml.maybe_open(DAV_URN, "read-current-user-privilege-set").await?.is_some() {
            xml.close().await?;
            return Ok(Self::ReadCurrentUserPrivilegeSet);
        }
        if xml.maybe_open(DAV_URN, "write-acl").await?.is_some() {
            xml.close().await?;
            return Ok(Self::WriteAcl);
        }
        if xml.maybe_open(DAV_URN, "bind").await?.is_some() {
            xml.close().await?;
            return Ok(Self::Bind);
        }
        if xml.maybe_open(DAV_URN, "unbind").await?.is_some() {
            xml.close().await?;
            return Ok(Self::Unbind);
        }
        if xml.maybe_open(DAV_URN, "all").await?.is_some() {
            xml.close().await?;
            return Ok(Self::All);
        }
        Err(ParsingError::Recoverable)
    }
}

impl QRead<PrivilegeSet> for PrivilegeSet {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        if xml
            .maybe_open_start(DAV_URN, "privilege")
            .await?
            .is_some()
        {
            let mut privileges = Vec::new();
            loop {
                let mut dirty = false;
                xml.maybe_push(&mut privileges, &mut dirty).await?;
                if !dirty {
                    match xml.peek() {
                        Event::End(_) => break,
                        _ => xml.skip().await?,
                    };
                }
            }
           
            xml.close().await?;
            return Ok(Self(privileges));
        }

        Err(ParsingError::Recoverable)
    }
}

// -----
impl QRead<User> for User {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        if xml.maybe_open(DAV_URN, "unauthenticated").await?.is_some() {
            xml.close().await?;
            return Ok(Self::Unauthenticated);
        }

        dav::Href::qread(xml).await.map(Self::Authenticated)
    }
}
