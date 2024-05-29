use super::acltypes::*;
use super::error::ParsingError;
use super::types as dav;
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
            xml.close().await?;
            return Ok(Self::CurrentUserPrivilegeSet(vec![]));
        }

        Err(ParsingError::Recoverable)
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
