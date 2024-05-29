use quick_xml::events::Event;
use quick_xml::Error as QError;

use super::acltypes::*;
use super::error::ParsingError;
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
            Self::CurrentUserPrivilegeSet(_) => {
                let empty_tag = xml.create_dav_element("current-user-privilege-set");
                xml.q.write_event_async(Event::Empty(empty_tag)).await
            }
        }
    }
}

impl QWrite for PropertyRequest {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let mut atom = async |c| {
            let empty_tag = xml.create_dav_element(c);
            xml.q.write_event_async(Event::Empty(empty_tag)).await
        };

        match self {
            Self::Owner => atom("owner").await,
            Self::CurrentUserPrincipal => atom("current-user-principal").await,
            Self::CurrentUserPrivilegeSet => atom("current-user-privilege-set").await,
        }
    }
}

impl QWrite for ResourceType {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        match self {
            Self::Principal => {
                let empty_tag = xml.create_dav_element("principal");
                xml.q.write_event_async(Event::Empty(empty_tag)).await
            }
        }
    }
}

// -----

impl QWrite for User {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        match self {
            Self::Unauthenticated => {
                let tag = xml.create_dav_element("unauthenticated");
                xml.q.write_event_async(Event::Empty(tag)).await
            }
            Self::Authenticated(href) => href.qwrite(xml).await,
        }
    }
}
