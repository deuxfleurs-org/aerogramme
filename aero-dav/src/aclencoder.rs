use quick_xml::events::Event;
use quick_xml::Error as QError;

use super::acltypes::*;
use super::xml::{IWrite, QWrite, Writer};

impl QWrite for Property {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        match self {
            Self::Owner(href) => {
                let start = xml.create_dav_element("owner");
                let end = start.to_end();
                xml.q.write_event_async(Event::Start(start.clone())).await?;
                href.qwrite(xml).await?;
                xml.q.write_event_async(Event::End(end)).await
            }
            Self::CurrentUserPrincipal(user) => {
                let start = xml.create_dav_element("current-user-principal");
                let end = start.to_end();
                xml.q.write_event_async(Event::Start(start.clone())).await?;
                user.qwrite(xml).await?;
                xml.q.write_event_async(Event::End(end)).await
            }
            Self::CurrentUserPrivilegeSet(privileges) => {
                let start = xml.create_dav_element("current-user-privilege-set");
                let end = start.to_end();
                xml.q.write_event_async(Event::Start(start.clone())).await?;
                privileges.qwrite(xml).await?;
                xml.q.write_event_async(Event::End(end)).await
            }
        }
    }
}

impl QWrite for PropertyRequest {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        match self {
            Self::Owner => xml.create_dav_atom("owner").await,
            Self::CurrentUserPrincipal => xml.create_dav_atom("current-user-principal").await,
            Self::CurrentUserPrivilegeSet => xml.create_dav_atom("current-user-privilege-set").await,
        }
    }
}

impl QWrite for ResourceType {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        match self {
            Self::Principal => xml.create_dav_atom("principal").await
        }
    }
}

impl QWrite for Privilege {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        match self {
            Self::Read => xml.create_dav_atom("read").await,
            Self::Write => xml.create_dav_atom("write").await,
            Self::WriteProperties => xml.create_dav_atom("write-properties").await,
            Self::WriteContent => xml.create_dav_atom("write-content").await,
            Self::Unlock => xml.create_dav_atom("unlock").await,
            Self::ReadAcl => xml.create_dav_atom("read-acl").await,
            Self::ReadCurrentUserPrivilegeSet => xml.create_dav_atom("read-current-user-privilege-set").await,
            Self::WriteAcl => xml.create_dav_atom("write-acl").await,
            Self::Bind => xml.create_dav_atom("bind").await,
            Self::Unbind => xml.create_dav_atom("unbind").await,
            Self::All => xml.create_dav_atom("all").await,
        }
    }
}

impl QWrite for PrivilegeSet {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let start = xml.create_dav_element("privilege");
        let end = start.to_end();
        xml.q.write_event_async(Event::Start(start.clone())).await?;
        for p in &self.0 {
            p.qwrite(xml).await?;
        }
        xml.q.write_event_async(Event::End(end)).await
    }
}

// -----

impl QWrite for User {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        match self {
            Self::Unauthenticated => xml.create_dav_atom("unauthenticated").await,
            Self::Authenticated(href) => href.qwrite(xml).await,
        }
    }
}
