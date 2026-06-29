use quick_xml::events::{BytesText, Event};
use quick_xml::Error as QError;

use super::cardtypes::*;
use super::extension::Extension;
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

// ----------------------- REPORT METHOD -------------------------------------

impl<E: Extension> QWrite for AddressbookQuery<E> {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let start = xml.create_card_element("addressbook-query");
        let end = start.to_end();

        xml.q.write_event_async(Event::Start(start.clone())).await?;
        if let Some(selector) = &self.selector {
            selector.qwrite(xml).await?;
        }
        self.filter.qwrite(xml).await?;
        if let Some(limit) = &self.limit {
            limit.qwrite(xml).await?;
        }
        xml.q.write_event_async(Event::End(end)).await
    }
}

impl QWrite for AddressDataRequest {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let mut start = xml.create_card_element("address-data");
        if let Some(content_type) = &self.content_type {
            start.push_attribute(("content-type", content_type.as_str()));
        }
        if let Some(version) = &self.version {
            start.push_attribute(("version", version.as_str()));
        }
        let end = start.to_end();
        xml.q.write_event_async(Event::Start(start.clone())).await?;
        if let Some(prop_kind) = &self.prop_kind {
            prop_kind.qwrite(xml).await?;
        }
        xml.q.write_event_async(Event::End(end)).await
    }
}

impl QWrite for AddressDataPayload {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let mut start = xml.create_card_element("address-data");
        if let Some(content_type) = &self.content_type {
            start.push_attribute(("content-type", content_type.as_str()));
        }
        if let Some(version) = &self.version {
            start.push_attribute(("version", version.as_str()));
        }
        let end = start.to_end();

        xml.q.write_event_async(Event::Start(start.clone())).await?;
        xml.q
           .write_event_async(Event::Text(BytesText::new(self.payload.as_str())))
           .await?;
        xml.q.write_event_async(Event::End(end)).await
    }
}

impl<E: Extension> QWrite for AddressbookMultiget<E> {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let start = xml.create_card_element("addressbook-multiget");
        let end = start.to_end();

        xml.q.write_event_async(Event::Start(start.clone())).await?;
        if let Some(selector) = &self.selector {
            selector.qwrite(xml).await?;
        }
        for href in self.href.iter() {
            href.qwrite(xml).await?;
        }
        xml.q.write_event_async(Event::End(end)).await
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

impl<E: Extension> QWrite for AddressbookSelector<E> {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        match self {
            Self::AllProp => {
                let empty_tag = xml.create_dav_element("allprop");
                xml.q.write_event_async(Event::Empty(empty_tag)).await
            }
            Self::PropName => {
                let empty_tag = xml.create_dav_element("propname");
                xml.q.write_event_async(Event::Empty(empty_tag)).await
            }
            Self::Prop(prop) => prop.qwrite(xml).await,
        }
    }
}

impl QWrite for Filter {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let mut start = xml.create_card_element("filter");
        if let Some(test) = &self.test {
            start.push_attribute(("test", test.as_str()));
        }
        let end = start.to_end();

        xml.q.write_event_async(Event::Start(start.clone())).await?;
        for filter in &self.prop_filters {
            filter.qwrite(xml).await?;
        }
        xml.q.write_event_async(Event::End(end)).await
    }
}

impl QWrite for PropFilter {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let mut start = xml.create_card_element("prop-filter");
        start.push_attribute(("name", self.name.0.as_str()));
        if let Some(test) = &self.test {
            start.push_attribute(("test", test.as_str()));
        }
        let end = start.to_end();

        xml.q.write_event_async(Event::Start(start.clone())).await?;
        self.rules.qwrite(xml).await?;
        xml.q.write_event_async(Event::End(end)).await
    }
}

impl QWrite for PropFilterRules {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        match &self {
            Self::Empty =>
                Ok(()),
            Self::IsNotDefined => {
                let empty_tag = xml.create_card_element("is-not-defined");
                xml.q.write_event_async(Event::Empty(empty_tag)).await
            },
            Self::Match { text_match, param_filter } => {
                for tmatch in text_match {
                    tmatch.qwrite(xml).await?;
                }
                for pfilter in param_filter {
                    pfilter.qwrite(xml).await?;
                }
                Ok(())
            }
        }
    }
}

impl QWrite for TextMatch {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let mut start = xml.create_card_element("text-match");
        if let Some(collation) = &self.collation {
            start.push_attribute(("collation", collation.as_str()));
        }
        match self.negate_condition {
            None => (),
            Some(true) => start.push_attribute(("negate-condition", "yes")),
            Some(false) => start.push_attribute(("negate-condition", "no")),
        }
        if let Some(match_type) = &self.match_type {
            start.push_attribute(("match-type", match_type.as_str()));
        }
        let end = start.to_end();

        xml.q.write_event_async(Event::Start(start.clone())).await?;
        xml.q
            .write_event_async(Event::Text(BytesText::new(self.text.as_str())))
            .await?;
        xml.q.write_event_async(Event::End(end)).await
    }
}

impl QWrite for ParamFilter {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let mut start = xml.create_card_element("param-filter");
        start.push_attribute(("name", self.name.0.as_str()));

        match &self.rules {
            None => xml.q.write_event_async(Event::Empty(start)).await,
            Some(rules) => {
                let end = start.to_end();
                xml.q.write_event_async(Event::Start(start.clone())).await?;
                rules.qwrite(xml).await?;
                xml.q.write_event_async(Event::End(end)).await
            }
        }
    }
}

impl QWrite for ParamFilterMatch {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        match self {
            Self::IsNotDefined => {
                let empty_tag = xml.create_card_element("is-not-defined");
                xml.q.write_event_async(Event::Empty(empty_tag)).await
            }
            Self::Match(tm) => tm.qwrite(xml).await,
        }
    }
}

impl QWrite for Limit {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let start = xml.create_card_element("limit");
        let end = start.to_end();
        xml.q.write_event_async(Event::Start(start.clone())).await?;
        {
            let start = xml.create_card_element("nresults");
            let end = start.to_end();
            xml.q.write_event_async(Event::Start(start.clone())).await?;
            xml.q.write_event_async(Event::Text(BytesText::new(&self.nresults.to_string()))).await?;
            xml.q.write_event_async(Event::End(end)).await?;
        }
        xml.q.write_event_async(Event::End(end)).await
    }
}

impl QWrite for PropKind {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        match self {
            Self::AllProp => {
                let empty_tag = xml.create_card_element("allprop");
                xml.q.write_event_async(Event::Empty(empty_tag)).await
            }
            Self::Prop(many_prop) => {
                for prop in many_prop.iter() {
                    prop.qwrite(xml).await?;
                }
                Ok(())
            }
        }
    }
}

impl QWrite for CardProp {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let mut empty = xml.create_card_element("prop");
        empty.push_attribute(("name", self.name.0.as_str()));
        match self.novalue {
            None => (),
            Some(true) => empty.push_attribute(("novalue", "yes")),
            Some(false) => empty.push_attribute(("novalue", "no")),
        }
        xml.q.write_event_async(Event::Empty(empty)).await
    }
}
