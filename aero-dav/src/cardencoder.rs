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
            Self::AddressData(req) => req.qwrite(xml).await,
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
            Self::AddressData(inner) => inner.qwrite(xml).await,
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
            Self::SupportedFilter { prop_filters, param_filters } => {
                let start = xml.create_card_element("supported-filter");
                let end = start.to_end();
                xml.q.write_event_async(Event::Start(start.clone())).await?;
                for prop_filter in prop_filters {
                    prop_filter.qwrite(xml).await?;
                }
                for param_filter in param_filters {
                    param_filter.qwrite(xml).await?;
                }
                xml.q.write_event_async(Event::End(end)).await
            }
            Self::NumberOfMatchesWithinLimits => {
                let empty_tag = xml.create_dav_element("number-of-matches-within-limits");
                xml.q.write_event_async(Event::Empty(empty_tag)).await
            }
        }
    }
}

// ----------------------- REPORT METHOD -------------------------------------

impl QWrite for ReportTypeName {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        match self {
            Self::Query => {
                let start = xml.create_card_element("addressbook-query");
                xml.q.write_event_async(Event::Empty(start)).await
            }
            Self::Multiget => {
                let start = xml.create_card_element("addressbook-multiget");
                xml.q.write_event_async(Event::Empty(start)).await
            }
        }
    }
}

impl<E: Extension> QWrite for ReportType<E> {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        match self {
            Self::Query(v) => v.qwrite(xml).await,
            Self::Multiget(v) => v.qwrite(xml).await,
        }
    }
}


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
        if let Some(content_type) = self.content_type.as_explicit() {
            start.push_attribute(("content-type", content_type.0.as_str()));
        }
        if let Some(version) = self.version.as_explicit() {
            start.push_attribute(("version", version.0.as_str()));
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
        if let Some(content_type) = self.content_type.as_explicit() {
            start.push_attribute(("content-type", content_type.0.as_str()));
        }
        if let Some(version) = self.version.as_explicit() {
            start.push_attribute(("version", version.0.as_str()));
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
        if let Some(content_type) = self.content_type.as_explicit() {
            typ.push_attribute(("content-type", content_type.0.as_str()))
        };
        if let Some(version) = self.version.as_explicit() {
            typ.push_attribute(("version", version.0.as_str()));
        }
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
        if let Some(test) = self.test.as_explicit() {
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
        if let Some(test) = self.test.as_explicit() {
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
        if let Some(collation) = self.collation.as_explicit() {
            start.push_attribute(("collation", collation.as_str()));
        }
        if let Some(ng) = self.negate_condition.as_explicit() {
            start.push_attribute(("negate-condition", ng.as_str()));
        }
        if let Some(match_type) = self.match_type.as_explicit() {
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
        if let Some(nv) = self.novalue.as_explicit() {
            empty.push_attribute(("novalue", nv.as_str()))
        }
        xml.q.write_event_async(Event::Empty(empty)).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::realization::Addressbook;
    use crate::coretypes as dav;
    use crate::xml::WithDefault;
    use tokio::io::AsyncWriteExt;
    use pretty_assertions::assert_eq;

    async fn serialize(elem: &impl QWrite) -> String {
        let mut buffer = Vec::new();
        let mut tokio_buffer = tokio::io::BufWriter::new(&mut buffer);
        let q = quick_xml::writer::Writer::new_with_indent(&mut tokio_buffer, b' ', 4);
        let ns_to_apply = vec![
            ("xmlns:D".into(), "DAV:".into()),
            ("xmlns:C".into(), "urn:ietf:params:xml:ns:carddav".into()),
        ];
        let mut writer = Writer { q, ns_to_apply };

        elem.qwrite(&mut writer).await.expect("xml serialization");
        tokio_buffer.flush().await.expect("tokio buffer flush");
        let got = std::str::from_utf8(buffer.as_slice()).unwrap();

        return got.into();
    }

    #[tokio::test]
    async fn rfc_principal_address() {
        let got = Property::PrincipalAddress(dav::Href("/system/cyrus.vcf".to_string()));

        let expected = r#"<C:principal-address xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:carddav">
    <D:href>/system/cyrus.vcf</D:href>
</C:principal-address>"#;
        
        let ser = serialize(&got).await;

        assert_eq!(ser, expected)
    }

    #[tokio::test]
    async fn rfc_supported_collation_set() {
        let got = Property::SupportedCollationSet(vec![
            SupportedCollation(Collation::AsciiCaseMap),
            SupportedCollation(Collation::Unknown("i;octet".to_string())),
            SupportedCollation(Collation::UnicodeCaseMap),
        ]);

        let expected = r#"<C:supported-collation-set xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:carddav">
    <C:supported-collation>i;ascii-casemap</C:supported-collation>
    <C:supported-collation>i;octet</C:supported-collation>
    <C:supported-collation>i;unicode-casemap</C:supported-collation>
</C:supported-collation-set>"#;

        let ser = serialize(&got).await;

        assert_eq!(ser, expected)
    }

    // §8.6.3, query
    #[tokio::test]
    async fn rfc_addressbook_query_8_6_3() {
        let got = AddressbookQuery::<Addressbook> {
            selector: Some(AddressbookSelector::Prop(dav::PropName(vec![
                dav::PropertyRequest::GetEtag,
                dav::PropertyRequest::Extension(PropertyRequest::AddressData(
                    AddressDataRequest {
                        content_type: Default::default(),
                        version: Default::default(),
                        prop_kind: Some(PropKind::Prop(vec![
                            CardProp {
                                name: PropertyName("VERSION".into()),
                                novalue: Default::default(),
                            },
                            CardProp {
                                name: PropertyName("UID".into()),
                                novalue: Default::default(),
                            },
                            CardProp {
                                name: PropertyName("NICKNAME".into()),
                                novalue: Default::default(),
                            },
                            CardProp {
                                name: PropertyName("EMAIL".into()),
                                novalue: Default::default(),
                            },
                            CardProp {
                                name: PropertyName("FN".into()),
                                novalue: Default::default(),
                            },
                        ])),
                    }
                )),
            ]))),
            filter: Filter {
                prop_filters: vec![PropFilter {
                    name: PropertyName("NICKNAME".to_string()),
                    test: Default::default(),
                    rules: PropFilterRules::Match {
                        text_match: vec![TextMatch {
                            collation: WithDefault::new(Collation::UnicodeCaseMap),
                            match_type: WithDefault::new(TextMatchType::Equals),
                            negate_condition: WithDefault::default(),
                            text: "me".to_string(),
                        }],
                        param_filter: vec![],
                    },
                }],
                test: Default::default(),
            },
            limit: None,
        };

        let expected = r#"<C:addressbook-query xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:carddav">
    <D:prop>
        <D:getetag/>
        <C:address-data>
            <C:prop name="VERSION"/>
            <C:prop name="UID"/>
            <C:prop name="NICKNAME"/>
            <C:prop name="EMAIL"/>
            <C:prop name="FN"/>
        </C:address-data>
    </D:prop>
    <C:filter>
        <C:prop-filter name="NICKNAME">
            <C:text-match collation="i;unicode-casemap" match-type="equals">me</C:text-match>
        </C:prop-filter>
    </C:filter>
</C:addressbook-query>"#;
        
        let ser = serialize(&got).await;

        assert_eq!(ser, expected)
    }

    // §8.6.3, response
    #[tokio::test]
    async fn rfc_addressbook_query_res_8_6_3() {
        let got = dav::Multistatus::<Addressbook> {
            extension: None,
            responses: vec![
                dav::Response {
                    status_or_propstat: dav::StatusOrPropstat::PropStat(
                        dav::Href("/home/bernard/addressbook/v102.vcf".into()),
                        vec![dav::PropStat {
                            prop: dav::AnyProp(vec![
                                dav::AnyProperty::Value(dav::Property::GetEtag(
                                    "\"23ba4d-ff11fb\"".into(),
                                )),
                                dav::AnyProperty::Value(dav::Property::Extension(
                                    Property::AddressData(AddressDataPayload {
                                        content_type: Default::default(),
                                        version: Default::default(),
                                        payload: "BEGIN:VCARD".into(),
                                    }),
                                )),
                            ]),
                            status: dav::Status(http::status::StatusCode::OK),
                            error: None,
                            responsedescription: None,
                        }],
                    ),
                    error: None,
                    location: None,
                    responsedescription: None,
                },
            ],
            responsedescription: None,
        };
        
        let expected = r#"<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:carddav">
    <D:response>
        <D:href>/home/bernard/addressbook/v102.vcf</D:href>
        <D:propstat>
            <D:prop>
                <D:getetag>&quot;23ba4d-ff11fb&quot;</D:getetag>
                <C:address-data>BEGIN:VCARD</C:address-data>
            </D:prop>
            <D:status>HTTP/1.1 200 OK</D:status>
        </D:propstat>
    </D:response>
</D:multistatus>"#;

        let ser = serialize(&got).await;
        
        assert_eq!(ser, expected);
    }

    // §8.6.4, query
    #[tokio::test]
    async fn rfc_addressbook_query_8_6_4() {
        let got = AddressbookQuery::<Addressbook> {
            selector: Some(AddressbookSelector::Prop(dav::PropName(vec![
                dav::PropertyRequest::GetEtag,
                dav::PropertyRequest::Extension(PropertyRequest::AddressData(
                    AddressDataRequest {
                        content_type: Default::default(),
                        version: Default::default(),
                        prop_kind: Some(PropKind::Prop(vec![
                            CardProp {
                                name: PropertyName("VERSION".into()),
                                novalue: Default::default(),
                            },
                            CardProp {
                                name: PropertyName("UID".into()),
                                novalue: Default::default(),
                            },
                            CardProp {
                                name: PropertyName("NICKNAME".into()),
                                novalue: Default::default(),
                            },
                            CardProp {
                                name: PropertyName("EMAIL".into()),
                                novalue: Default::default(),
                            },
                            CardProp {
                                name: PropertyName("FN".into()),
                                novalue: Default::default(),
                            },
                        ])),
                    }
                )),
            ]))),
            filter: Filter {
                prop_filters: vec![
                    PropFilter {
                        name: PropertyName("FN".to_string()),
                        test: Default::default(),
                        rules: PropFilterRules::Match {
                            text_match: vec![TextMatch {
                                collation: WithDefault::new(Collation::UnicodeCaseMap),
                                match_type: WithDefault::new(TextMatchType::Contains),
                                negate_condition: WithDefault::default(),
                                text: "daboo".to_string(),
                            }],
                            param_filter: vec![],
                        },
                    },
                    PropFilter {
                        name: PropertyName("EMAIL".to_string()),
                        test: Default::default(),
                        rules: PropFilterRules::Match {
                            text_match: vec![TextMatch {
                                collation: WithDefault::new(Collation::UnicodeCaseMap),
                                match_type: WithDefault::new(TextMatchType::Contains),
                                negate_condition: WithDefault::default(),
                                text: "daboo".to_string(),
                            }],
                            param_filter: vec![],
                        },
                    },
                ],
                test: WithDefault::new(FilterTest::AnyOf),
            },
            limit: None,
        };

        let expected = r#"<C:addressbook-query xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:carddav">
    <D:prop>
        <D:getetag/>
        <C:address-data>
            <C:prop name="VERSION"/>
            <C:prop name="UID"/>
            <C:prop name="NICKNAME"/>
            <C:prop name="EMAIL"/>
            <C:prop name="FN"/>
        </C:address-data>
    </D:prop>
    <C:filter test="anyof">
        <C:prop-filter name="FN">
            <C:text-match collation="i;unicode-casemap" match-type="contains">daboo</C:text-match>
        </C:prop-filter>
        <C:prop-filter name="EMAIL">
            <C:text-match collation="i;unicode-casemap" match-type="contains">daboo</C:text-match>
        </C:prop-filter>
    </C:filter>
</C:addressbook-query>"#;
        
        let ser = serialize(&got).await;

        assert_eq!(ser, expected)
    }

    // §8.6.4, response
    #[tokio::test]
    async fn rfc_addressbook_query_res_8_6_4() {
        let got = dav::Multistatus::<Addressbook> {
            extension: None,
            responses: vec![
                dav::Response {
                    status_or_propstat: dav::StatusOrPropstat::PropStat(
                        dav::Href("/home/bernard/addressbook/v102.vcf".into()),
                        vec![dav::PropStat {
                            prop: dav::AnyProp(vec![
                                dav::AnyProperty::Value(dav::Property::GetEtag(
                                    "\"23ba4d-ff11fb\"".into(),
                                )),
                                dav::AnyProperty::Value(dav::Property::Extension(
                                    Property::AddressData(AddressDataPayload {
                                        content_type: Default::default(),
                                        version: Default::default(),
                                        payload: "BEGIN:VCARD".into(),
                                    }),
                                )),
                            ]),
                            status: dav::Status(http::status::StatusCode::OK),
                            error: None,
                            responsedescription: None,
                        }],
                    ),
                    error: None,
                    location: None,
                    responsedescription: None,
                },
                dav::Response {
                    status_or_propstat: dav::StatusOrPropstat::PropStat(
                        dav::Href("/home/bernard/addressbook/v104.vcf".into()),
                        vec![dav::PropStat {
                            prop: dav::AnyProp(vec![
                                dav::AnyProperty::Value(dav::Property::GetEtag(
                                    "\"23ba4d-ff11fc\"".into(),
                                )),
                                dav::AnyProperty::Value(dav::Property::Extension(
                                    Property::AddressData(AddressDataPayload {
                                        content_type: Default::default(),
                                        version: Default::default(),
                                        payload: "BEGIN:VCARD".into(),
                                    }),
                                )),
                            ]),
                            status: dav::Status(http::status::StatusCode::OK),
                            error: None,
                            responsedescription: None,
                        }],
                    ),
                    error: None,
                    location: None,
                    responsedescription: None,
                },
            ],
            responsedescription: None,
        };
        
        let expected = r#"<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:carddav">
    <D:response>
        <D:href>/home/bernard/addressbook/v102.vcf</D:href>
        <D:propstat>
            <D:prop>
                <D:getetag>&quot;23ba4d-ff11fb&quot;</D:getetag>
                <C:address-data>BEGIN:VCARD</C:address-data>
            </D:prop>
            <D:status>HTTP/1.1 200 OK</D:status>
        </D:propstat>
    </D:response>
    <D:response>
        <D:href>/home/bernard/addressbook/v104.vcf</D:href>
        <D:propstat>
            <D:prop>
                <D:getetag>&quot;23ba4d-ff11fc&quot;</D:getetag>
                <C:address-data>BEGIN:VCARD</C:address-data>
            </D:prop>
            <D:status>HTTP/1.1 200 OK</D:status>
        </D:propstat>
    </D:response>
</D:multistatus>"#;

        let ser = serialize(&got).await;
        assert_eq!(ser, expected);
    }

    // §8.6.5, query
    #[tokio::test]
    async fn rfc_addressbook_query_8_6_5() {
        let got = AddressbookQuery::<Addressbook> {
            selector: Some(AddressbookSelector::Prop(dav::PropName(vec![
                dav::PropertyRequest::GetEtag,
            ]))),
            filter: Filter {
                prop_filters: vec![
                    PropFilter {
                        name: PropertyName("FN".to_string()),
                        test: Default::default(),
                        rules: PropFilterRules::Match {
                            text_match: vec![TextMatch {
                                collation: WithDefault::new(Collation::UnicodeCaseMap),
                                match_type: WithDefault::new(TextMatchType::Contains),
                                negate_condition: WithDefault::default(),
                                text: "daboo".to_string(),
                            }],
                            param_filter: vec![],
                        },
                    },
                ],
                test: WithDefault::new(FilterTest::AnyOf),
            },
            limit: Some(Limit { nresults: 2 }),
        };

        let expected = r#"<C:addressbook-query xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:carddav">
    <D:prop>
        <D:getetag/>
    </D:prop>
    <C:filter test="anyof">
        <C:prop-filter name="FN">
            <C:text-match collation="i;unicode-casemap" match-type="contains">daboo</C:text-match>
        </C:prop-filter>
    </C:filter>
    <C:limit>
        <C:nresults>2</C:nresults>
    </C:limit>
</C:addressbook-query>"#;
        
        let ser = serialize(&got).await;
        assert_eq!(ser, expected)
    }

    // §8.6.5, response
    #[tokio::test]
    async fn rfc_addressbook_query_res_8_6_5() {
        let got = dav::Multistatus::<Addressbook> {
            extension: None,
            responses: vec![
                dav::Response {
                    status_or_propstat: dav::StatusOrPropstat::Status(
                        vec![dav::Href("/home/bernard/addressbook/".into())],
                        dav::Status(http::status::StatusCode::INSUFFICIENT_STORAGE),
                    ),
                    error: Some(dav::Error(vec![
                        dav::Violation::Extension(Violation::NumberOfMatchesWithinLimits),
                    ])),
                    responsedescription: Some(dav::ResponseDescription(
                        "\n         Only two matching records were returned\n       ".into()
                    )),
                    location: None,
                },
                dav::Response {
                    status_or_propstat: dav::StatusOrPropstat::PropStat(
                        dav::Href("/home/bernard/addressbook/v102.vcf".into()),
                        vec![dav::PropStat {
                            prop: dav::AnyProp(vec![
                                dav::AnyProperty::Value(dav::Property::GetEtag(
                                    "\"23ba4d-ff11fb\"".into(),
                                )),
                            ]),
                            status: dav::Status(http::status::StatusCode::OK),
                            error: None,
                            responsedescription: None,
                        }],
                    ),
                    error: None,
                    location: None,
                    responsedescription: None,
                },
                dav::Response {
                    status_or_propstat: dav::StatusOrPropstat::PropStat(
                        dav::Href("/home/bernard/addressbook/v104.vcf".into()),
                        vec![dav::PropStat {
                            prop: dav::AnyProp(vec![
                                dav::AnyProperty::Value(dav::Property::GetEtag(
                                    "\"23ba4d-ff11fc\"".into(),
                                )),
                            ]),
                            status: dav::Status(http::status::StatusCode::OK),
                            error: None,
                            responsedescription: None,
                        }],
                    ),
                    error: None,
                    location: None,
                    responsedescription: None,
                },
            ],
            responsedescription: None,
        };
        
        let expected = r#"<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:carddav">
    <D:response>
        <D:href>/home/bernard/addressbook/</D:href>
        <D:status>HTTP/1.1 507 Insufficient Storage</D:status>
        <D:error>
            <D:number-of-matches-within-limits/>
        </D:error>
        <D:responsedescription>
         Only two matching records were returned
       </D:responsedescription>
    </D:response>
    <D:response>
        <D:href>/home/bernard/addressbook/v102.vcf</D:href>
        <D:propstat>
            <D:prop>
                <D:getetag>&quot;23ba4d-ff11fb&quot;</D:getetag>
            </D:prop>
            <D:status>HTTP/1.1 200 OK</D:status>
        </D:propstat>
    </D:response>
    <D:response>
        <D:href>/home/bernard/addressbook/v104.vcf</D:href>
        <D:propstat>
            <D:prop>
                <D:getetag>&quot;23ba4d-ff11fc&quot;</D:getetag>
            </D:prop>
            <D:status>HTTP/1.1 200 OK</D:status>
        </D:propstat>
    </D:response>
</D:multistatus>"#;

        let ser = serialize(&got).await;
        assert_eq!(ser, expected);
    }

    // §8.7.1, query
    #[tokio::test]
    async fn rfc_multiget_query_8_7_1() {
        let got = AddressbookMultiget::<Addressbook> {
            selector: Some(AddressbookSelector::Prop(dav::PropName(vec![
                dav::PropertyRequest::GetEtag,
                dav::PropertyRequest::Extension(PropertyRequest::AddressData(
                    AddressDataRequest {
                        content_type: Default::default(),
                        version: Default::default(),
                        prop_kind: Some(PropKind::Prop(vec![
                            CardProp {
                                name: PropertyName("VERSION".into()),
                                novalue: Default::default(),
                            },
                            CardProp {
                                name: PropertyName("UID".into()),
                                novalue: Default::default(),
                            },
                            CardProp {
                                name: PropertyName("NICKNAME".into()),
                                novalue: Default::default(),
                            },
                            CardProp {
                                name: PropertyName("EMAIL".into()),
                                novalue: Default::default(),
                            },
                            CardProp {
                                name: PropertyName("FN".into()),
                                novalue: Default::default(),
                            },
                        ])),
                    }
                )),
            ]))),
            href: vec![
                dav::Href("/home/bernard/addressbook/vcf102.vcf".into()),
                dav::Href("/home/bernard/addressbook/vcf1.vcf".into()),
            ],
        };

        let expected = r#"<C:addressbook-multiget xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:carddav">
    <D:prop>
        <D:getetag/>
        <C:address-data>
            <C:prop name="VERSION"/>
            <C:prop name="UID"/>
            <C:prop name="NICKNAME"/>
            <C:prop name="EMAIL"/>
            <C:prop name="FN"/>
        </C:address-data>
    </D:prop>
    <D:href>/home/bernard/addressbook/vcf102.vcf</D:href>
    <D:href>/home/bernard/addressbook/vcf1.vcf</D:href>
</C:addressbook-multiget>"#;
        
        let ser = serialize(&got).await;
        assert_eq!(ser, expected)
    }

    // §8.7.1, response
    #[tokio::test]
    async fn rfc_multiget_query_res_8_7_1() {
        let got = dav::Multistatus::<Addressbook> {
            extension: None,
            responses: vec![
                dav::Response {
                    status_or_propstat: dav::StatusOrPropstat::PropStat(
                        dav::Href("/home/bernard/addressbook/vcf102.vcf".into()),
                        vec![dav::PropStat {
                            prop: dav::AnyProp(vec![
                                dav::AnyProperty::Value(dav::Property::GetEtag(
                                    "\"23ba4d-ff11fb\"".into(),
                                )),
                                dav::AnyProperty::Value(dav::Property::Extension(
                                    Property::AddressData(AddressDataPayload {
                                        content_type: Default::default(),
                                        version: Default::default(),
                                        payload: "BEGIN:VCARD".into(),
                                    }),
                                )),
                            ]),
                            status: dav::Status(http::status::StatusCode::OK),
                            error: None,
                            responsedescription: None,
                        }],
                    ),
                    error: None,
                    location: None,
                    responsedescription: None,
                },
                dav::Response {
                    status_or_propstat: dav::StatusOrPropstat::Status(
                        vec![dav::Href("/home/bernard/addressbook/vcf1.vcf".into())],
                        dav::Status(http::status::StatusCode::NOT_FOUND),
                    ),
                    error: None,
                    location: None,
                    responsedescription: None,
                },
            ],
            responsedescription: None,
        };
        
        let expected = r#"<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:carddav">
    <D:response>
        <D:href>/home/bernard/addressbook/vcf102.vcf</D:href>
        <D:propstat>
            <D:prop>
                <D:getetag>&quot;23ba4d-ff11fb&quot;</D:getetag>
                <C:address-data>BEGIN:VCARD</C:address-data>
            </D:prop>
            <D:status>HTTP/1.1 200 OK</D:status>
        </D:propstat>
    </D:response>
    <D:response>
        <D:href>/home/bernard/addressbook/vcf1.vcf</D:href>
        <D:status>HTTP/1.1 404 Not Found</D:status>
    </D:response>
</D:multistatus>"#;

        let ser = serialize(&got).await;
        assert_eq!(ser, expected);
    }
    
    // §8.7.2, query
    #[tokio::test]
    async fn rfc_multiget_query_8_7_2() {
        let got = AddressbookMultiget::<Addressbook> {
            selector: Some(AddressbookSelector::Prop(dav::PropName(vec![
                dav::PropertyRequest::GetEtag,
                dav::PropertyRequest::Extension(PropertyRequest::AddressData(
                    AddressDataRequest {
                        content_type: WithDefault::new(ContentType("text/vcard".to_string())),
                        version: WithDefault::new(Version("4.0".to_string())),
                        prop_kind: None,
                    }
                )),
            ]))),
            href: vec![
                dav::Href("/home/bernard/addressbook/vcf3.vcf".into()),
            ],
        };

        let expected = r#"<C:addressbook-multiget xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:carddav">
    <D:prop>
        <D:getetag/>
        <C:address-data content-type="text/vcard" version="4.0">
        </C:address-data>
    </D:prop>
    <D:href>/home/bernard/addressbook/vcf3.vcf</D:href>
</C:addressbook-multiget>"#;
        
        let ser = serialize(&got).await;
        assert_eq!(ser, expected)
    }

    // §8.7.1, response
    #[tokio::test]
    async fn rfc_multiget_query_res_8_7_2() {
        let got = dav::Multistatus::<Addressbook> {
            extension: None,
            responses: vec![
                dav::Response {
                    status_or_propstat: dav::StatusOrPropstat::Status(
                        vec![dav::Href("/home/bernard/addressbook/vcf3.vcf".into())],
                        dav::Status(http::status::StatusCode::UNSUPPORTED_MEDIA_TYPE),
                    ),
                    error: Some(dav::Error(vec![
                        dav::Violation::Extension(
                            Violation::SupportedAddressDataConversion
                        )
                    ])),
                    location: None,
                    responsedescription: Some(dav::ResponseDescription(
                        "Unable to convert from vCard v3.0\n       to vCard v4.0".into()
                    )),
                },
            ],
            responsedescription: None,
        };
        
        let expected = r#"<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:carddav">
    <D:response>
        <D:href>/home/bernard/addressbook/vcf3.vcf</D:href>
        <D:status>HTTP/1.1 415 Unsupported Media Type</D:status>
        <D:error>
            <C:supported-address-data-conversion/>
        </D:error>
        <D:responsedescription>Unable to convert from vCard v3.0
       to vCard v4.0</D:responsedescription>
    </D:response>
</D:multistatus>"#;

        let ser = serialize(&got).await;
        assert_eq!(ser, expected);
    }
}
