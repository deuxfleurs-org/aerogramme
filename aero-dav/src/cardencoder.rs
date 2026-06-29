use quick_xml::events::{BytesText, Event};
use quick_xml::Error as QError;

use super::cardtypes::*;
use super::xml::{IWrite, Node, QWrite, Writer};

// ---------------------- DAV::resourcetype ----------------------------------
impl QWrite for ResourceType {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        match self {
            Self::Addressbook => {
                let empty_tag = xml.create_card_element("addressbook");
                xml.q.write_event_async(Event::Empty(empty_tag)).await
            }
        }
    }
}

// -------------------------- DAV::prop --------------------------------------
impl QWrite for PropertyRequest {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let mut atom = async |c| {
            let empty_tag = xml.create_card_element(c);
            xml.q.write_event_async(Event::Empty(empty_tag)).await
        };

        match self {
            Self::AddressbookDescription => atom("addressbook-description").await,
            Self::SupportedAddressData => atom("supported-address-data").await,
            Self::MaxResourceSize => atom("max-resource-size").await,
            Self::AddressbookHomeSet => atom("addressbook-home-set").await,
            Self::PrincipalAddress => atom("principal-address").await,
            Self::SupportedCollationSet => atom("supported-collation-set").await,
        }
    }
}
impl QWrite for Property {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        match self {
            Self::AddressbookDescription { lang, text } => {
                let mut start = xml.create_card_element("addressbook-description");
                if let Some(the_lang) = lang {
                    start.push_attribute(("xml:lang", the_lang.as_str()));
                }
                let end = start.to_end();

                xml.q.write_event_async(Event::Start(start.clone())).await?;
                xml.q
                    .write_event_async(Event::Text(BytesText::new(text)))
                    .await?;
                xml.q.write_event_async(Event::End(end)).await
            }
            Self::SupportedAddressData(many_types) => {
                let start = xml.create_card_element("supported-address-data");
                let end = start.to_end();

                xml.q.write_event_async(Event::Start(start.clone())).await?;
                for typ in many_types.iter() {
                    typ.qwrite(xml).await?;
                }
                xml.q.write_event_async(Event::End(end)).await
            }
            Self::MaxResourceSize(bytes) => {
                let start = xml.create_card_element("max-resource-size");
                let end = start.to_end();

                xml.q.write_event_async(Event::Start(start.clone())).await?;
                xml.q
                    .write_event_async(Event::Text(BytesText::new(bytes.to_string().as_str())))
                    .await?;
                xml.q.write_event_async(Event::End(end)).await
            }
            Self::AddressbookHomeSet(href) => {
                let start = xml.create_card_element("addressbook-home-set");
                let end = start.to_end();
                xml.q.write_event_async(Event::Start(start.clone())).await?;
                href.qwrite(xml).await?;
                xml.q.write_event_async(Event::End(end)).await
            }
            Self::PrincipalAddress(href) => {
                let start = xml.create_card_element("principal-address");
                let end = start.to_end();
                xml.q.write_event_async(Event::Start(start.clone())).await?;
                href.qwrite(xml).await?;
                xml.q.write_event_async(Event::End(end)).await
            }
            Self::SupportedCollationSet(many_collations) => {
                let start = xml.create_card_element("supported-collation-set");
                let end = start.to_end();

                xml.q.write_event_async(Event::Start(start.clone())).await?;
                for collation in many_collations.iter() {
                    collation.qwrite(xml).await?;
                }
                xml.q.write_event_async(Event::End(end)).await
            }
        }
    }
}

// --------------------------- DAV::error ------------------------------------
impl QWrite for Violation {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let mut atom = async |c| {
            let empty_tag = xml.create_card_element(c);
            xml.q.write_event_async(Event::Empty(empty_tag)).await
        };
        match self {
            Self::SupportedAddressDataConversion =>
                atom("supported-address-data-conversion").await,
            Self::SupportedAddressData =>
                atom("supported-address-data").await,
            Self::ValidAddressData =>
                atom("valid-address-data").await,
            Self::NoUidConflict(href) => {
                let start = xml.create_card_element("no-uid-conflict");
                let end = start.to_end();

                xml.q.write_event_async(Event::Start(start.clone())).await?;
                href.qwrite(xml).await?;
                xml.q.write_event_async(Event::End(end)).await
            }
            Self::AddressbookCollectionLocationOk =>
                atom("addressbook-collection-location-ok").await,
            Self::MaxResourceSize =>
                atom("max-resource-size").await,
            Self::SupportedCollation =>
                atom("supported-collation").await,
        }
    }
}

// ---------------------------- Inner XML ------------------------------------
impl QWrite for AddressDataType {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let mut typ = xml.create_card_element("address-data-type");
        typ.push_attribute(("content-type", self.content_type.as_str()));
        typ.push_attribute(("version", self.version.as_str()));
        xml.q.write_event_async(Event::Empty(typ)).await
    }
}

impl QWrite for SupportedCollation {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let start = xml.create_card_element("supported-collation");
        let end = start.to_end();

        xml.q.write_event_async(Event::Start(start.clone())).await?;
        self.0.qwrite(xml).await?;
        xml.q.write_event_async(Event::End(end)).await
    }
}

impl QWrite for Collation {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        xml.q
            .write_event_async(Event::Text(BytesText::new(self.as_str())))
            .await
    }
}
