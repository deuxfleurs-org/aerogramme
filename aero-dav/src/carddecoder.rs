use quick_xml::events::Event;
use std::str::FromStr;

use super::cardtypes::*;
use super::coretypes as dav;
use super::error::ParsingError;
use super::extension::Extension;
use super::xml::{IRead, QRead, Reader, CARD_URN, DAV_URN, WithDefault};

impl<E: Extension> QRead<ReportType<E>> for ReportType<E> {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        match AddressbookQuery::<E>::qread(xml).await {
            Err(ParsingError::Recoverable) => (),
            otherwise => return otherwise.map(Self::Query),
        }

        AddressbookMultiget::<E>::qread(xml).await.map(Self::Multiget)
    }
}

impl QRead<ReportTypeName> for ReportTypeName {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        if xml.maybe_open(CARD_URN, "addressbook-query").await?.is_some() {
            xml.close().await?;
            return Ok(Self::Query);
        }
        if xml
            .maybe_open(CARD_URN, "addressbook-multiget")
            .await?
            .is_some()
        {
            xml.close().await?;
            return Ok(Self::Multiget);
        }
        Err(ParsingError::Recoverable)
    }
}

impl QRead<ResourceType> for ResourceType {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        if xml.maybe_open(CARD_URN, "addressbook").await?.is_some() {
            xml.close().await?;
            return Ok(Self::Addressbook);
        }
        Err(ParsingError::Recoverable)
    }
}

impl QRead<PropertyRequest> for PropertyRequest {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        if xml
            .maybe_open(CARD_URN, "addressbook-description")
            .await?
            .is_some()
        {
            xml.close().await?;
            return Ok(Self::AddressbookDescription);
        }
        if xml
            .maybe_open(CARD_URN, "supported-address-data")
            .await?
            .is_some()
        {
            xml.close().await?;
            return Ok(Self::SupportedAddressData);
        }
        if xml
            .maybe_open(CARD_URN, "max-resource-size")
            .await?
            .is_some()
        {
            xml.close().await?;
            return Ok(Self::MaxResourceSize);
        }
        if xml
            .maybe_open(CARD_URN, "addressbook-home-set")
            .await?
            .is_some()
        {
            xml.close().await?;
            return Ok(Self::AddressbookHomeSet);
        }
        if xml
            .maybe_open(CARD_URN, "principal-address")
            .await?
            .is_some()
        {
            xml.close().await?;
            return Ok(Self::PrincipalAddress);
        }
        if xml
            .maybe_open(CARD_URN, "supported-collation-set")
            .await?
            .is_some()
        {
            xml.close().await?;
            return Ok(Self::SupportedCollationSet);
        }
        let mut dirty = false;
        let mut m_cdr = None;
        xml.maybe_read(&mut m_cdr, &mut dirty).await?;
        m_cdr
            .ok_or(ParsingError::Recoverable)
            .map(Self::AddressData)
    }
}

impl QRead<Property> for Property {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        if xml
            .maybe_open_start(CARD_URN, "addressbook-description")
            .await?
            .is_some()
        {
            let lang = xml.prev_attr("xml:lang");
            let text = xml.tag_string().await?;
            xml.close().await?;
            return Ok(Property::AddressbookDescription { lang, text });
        }

        if xml
            .maybe_open_start(CARD_URN, "supported-address-data")
            .await?
            .is_some()
        {
            let types = xml.collect().await?;
            xml.close().await?;
            return Ok(Property::SupportedAddressData(types));
        }
        
        if xml
            .maybe_open_start(CARD_URN, "max-resource-size")
            .await?
            .is_some()
        {
            let sz = xml.tag_string().await?.parse::<u64>()?;
            xml.close().await?;
            return Ok(Property::MaxResourceSize(sz));
        }

        if xml
            .maybe_open_start(CARD_URN, "addressbook-home-set")
            .await?
            .is_some()
        {
            let href = xml.find().await?;
            xml.close().await?;
            return Ok(Property::AddressbookHomeSet(href));
        }
        
        if xml
            .maybe_open_start(CARD_URN, "principal-address")
            .await?
            .is_some()
        {
            let href = xml.find().await?;
            xml.close().await?;
            return Ok(Property::PrincipalAddress(href));
        }
        
        if xml
            .maybe_open_start(CARD_URN, "principal-address")
            .await?
            .is_some()
        {
            let href = xml.find().await?;
            xml.close().await?;
            return Ok(Property::PrincipalAddress(href));
        }
        
        if xml
            .maybe_open_start(CARD_URN, "supported-collation-set")
            .await?
            .is_some()
        {
            let cols = xml.collect().await?;
            xml.close().await?;
            return Ok(Property::SupportedCollationSet(cols));
        }

        let mut dirty = false;
        let mut addrdata: Option<AddressDataPayload> = None;
        xml.maybe_read(&mut addrdata, &mut dirty).await?;
        if let Some(addr) = addrdata {
            return Ok(Property::AddressData(addr));
        }
        
        Err(ParsingError::Recoverable)
    }
}

impl QRead<Violation> for Violation {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        if xml
            .maybe_open(CARD_URN, "supported-address-data-conversion")
            .await?
            .is_some()
        {
            xml.close().await?;
            Ok(Self::SupportedAddressDataConversion)
        } else if xml
            .maybe_open(CARD_URN, "supported-address-data")
            .await?
            .is_some()
        {
            xml.close().await?;
            Ok(Self::SupportedAddressData)
        } else if xml
            .maybe_open(CARD_URN, "valid-address-data")
            .await?
            .is_some()
        {
            xml.close().await?;
            Ok(Self::ValidAddressData)
        } else if xml
            .maybe_open(CARD_URN, "no-uid-conflict")
            .await?
            .is_some()
        {
            let href = xml.find().await?;
            xml.close().await?;
            Ok(Self::NoUidConflict(href))
        } else if xml
            .maybe_open(CARD_URN, "addressbook-collection-location-ok")
            .await?
            .is_some()
        {
            xml.close().await?;
            Ok(Self::AddressbookCollectionLocationOk)
        } else if xml
            .maybe_open(CARD_URN, "max-resource-size")
            .await?
            .is_some()
        {
            xml.close().await?;
            Ok(Self::MaxResourceSize)
        } else if xml
            .maybe_open(CARD_URN, "supported-collation")
            .await?
            .is_some()
        {
            xml.close().await?;
            Ok(Self::SupportedCollation)
        } else if xml
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

impl<E: Extension> QRead<AddressbookQuery<E>> for AddressbookQuery<E> {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(CARD_URN, "addressbook-query").await?;
        let (mut selector, mut filter, mut limit) = (None, None, None);
        loop {
            let mut dirty = false;
            xml.maybe_read(&mut selector, &mut dirty).await?;
            xml.maybe_read(&mut filter, &mut dirty).await?;
            xml.maybe_read(&mut limit, &mut dirty).await?;

            if !dirty {
                match xml.peek() {
                    Event::End(_) => break,
                    _ => xml.skip().await?,
                };
            }
        }
        xml.close().await?;

        match filter {
            Some(filter) => Ok(AddressbookQuery {
                selector,
                filter,
                limit,
            }),
            _ => Err(ParsingError::MissingChild),
        }
    }
}

impl QRead<AddressDataRequest> for AddressDataRequest {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(CARD_URN, "address-data").await?;
        let content_type = WithDefault::from_opt(
            xml.prev_attr("content-type").map(ContentType)
        );
        let version = WithDefault::from_opt(
            xml.prev_attr("version").map(Version)
        );
        let prop_kind = xml.maybe_find().await?;
        xml.close().await?;
        Ok(Self { prop_kind, content_type, version })
    }
}

impl QRead<AddressDataPayload> for AddressDataPayload {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(CARD_URN, "address-data").await?;
        let content_type = WithDefault::from_opt(
            xml.prev_attr("content-type").map(ContentType)
        );
        let version = WithDefault::from_opt(
            xml.prev_attr("version").map(Version)
        );
        let payload = xml.tag_string().await?;
        xml.close().await?;
        Ok(AddressDataPayload { payload, content_type, version })
    }
}

impl<E: Extension> QRead<AddressbookMultiget<E>> for AddressbookMultiget<E> {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(CARD_URN, "addressbook-multiget").await?;
        let mut selector = None;
        let mut href = Vec::new();

        loop {
            let mut dirty = false;
            xml.maybe_read(&mut selector, &mut dirty).await?;
            xml.maybe_push(&mut href, &mut dirty).await?;

            if !dirty {
                match xml.peek() {
                    Event::End(_) => break,
                    _ => xml.skip().await?,
                };
            }
        }

        xml.close().await?;
        Ok(AddressbookMultiget { selector, href })
    }
}

// ---- INNER XML ----
impl QRead<AddressDataType> for AddressDataType {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(CARD_URN, "address-data-type").await?;
        let content_type = WithDefault::from_opt(
            xml.prev_attr("content-type").map(ContentType)
        );
        let version = WithDefault::from_opt(
            xml.prev_attr("version").map(Version)
        );
        xml.close().await?;
        Ok(Self { content_type, version })
    }
}

impl QRead<SupportedCollation> for SupportedCollation {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(CARD_URN, "supported-collation").await?;
        let col = Collation::new(xml.tag_string().await?);
        xml.close().await?;
        Ok(SupportedCollation(col))
    }
}

impl<E: Extension> QRead<AddressbookSelector<E>> for AddressbookSelector<E> {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        // allprop
        if let Some(_) = xml.maybe_open(DAV_URN, "allprop").await? {
            xml.close().await?;
            return Ok(Self::AllProp);
        }

        // propname
        if let Some(_) = xml.maybe_open(DAV_URN, "propname").await? {
            xml.close().await?;
            return Ok(Self::PropName);
        }

        // prop
        let (mut maybe_prop, mut dirty) = (None, false);
        xml.maybe_read::<dav::PropName<E>>(&mut maybe_prop, &mut dirty)
            .await?;
        if let Some(prop) = maybe_prop {
            return Ok(Self::Prop(prop));
        }

        Err(ParsingError::Recoverable)
    }
}

impl QRead<Filter> for Filter {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(CARD_URN, "filter").await?;
        let test = WithDefault::from_opt(
            xml
                .prev_attr("test")
                .map(|s| FilterTest::from_str(&s))
                .transpose()
                .map_err(|()| ParsingError::InvalidValue)?
        );
        let prop_filters = xml.collect().await?;
        xml.close().await?;
        Ok(Self { prop_filters, test })
    }
}

impl QRead<PropFilter> for PropFilter {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(CARD_URN, "prop-filter").await?;
        let name = PropertyName::from_str(
            &xml.prev_attr("name")
                .ok_or(ParsingError::MissingAttribute)?
        ).map_err(|()| ParsingError::InvalidValue)?;
        let test = WithDefault::from_opt(
            xml.prev_attr("test")
               .map(|s| FilterTest::from_str(&s))
               .transpose()
               .map_err(|()| ParsingError::InvalidValue)?
        );
        let rules = xml.find().await?;
        xml.close().await?;
        Ok(Self {
            name,
            test,
            rules,
        })
    }
}

impl QRead<PropFilterRules> for PropFilterRules {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        let mut text_match = Vec::new();
        let mut param_filter = Vec::new();

        loop {
            let mut dirty = false;

            if xml.maybe_open(CARD_URN, "is-not-defined").await?.is_some() {
                xml.close().await?;
                return Ok(Self::IsNotDefined);
            }

            xml.maybe_push(&mut text_match, &mut dirty).await?;
            xml.maybe_push(&mut param_filter, &mut dirty).await?;

            if !dirty {
                match xml.peek() {
                    Event::End(_) => break,
                    _ => xml.skip().await?,
                };
            }
        }

        if text_match.is_empty() && param_filter.is_empty() {
            Ok(Self::Empty)
        } else {
            Ok(Self::Match {
                text_match,
                param_filter,
            })
        }
    }
}

impl QRead<TextMatch> for TextMatch {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(CARD_URN, "text-match").await?;
        let collation = WithDefault::from_opt(xml.prev_attr("collation").map(Collation::new));
        let negate_condition = WithDefault::from_opt(
            xml.prev_attr("negate-condition")
               .map(|s| NegateCondition::from_str(&s))
               .transpose()
               .map_err(|()| ParsingError::InvalidValue)?
        );
        let match_type = WithDefault::from_opt(
            xml.prev_attr("match-type")
               .map(|s| TextMatchType::from_str(&s))
               .transpose()
               .map_err(|()| ParsingError::InvalidValue)?
        );
        let text = xml.tag_string().await?;
        xml.close().await?;
        Ok(Self {
            collation,
            negate_condition,
            match_type,
            text,
        })
    }
}

impl QRead<ParamFilter> for ParamFilter {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(CARD_URN, "param-filter").await?;
        let name = PropertyParameterName(
            xml.prev_attr("name")
                .ok_or(ParsingError::MissingAttribute)?,
        );
        let rules = xml.maybe_find().await?;
        xml.close().await?;
        Ok(Self {
            name,
            rules,
        })
    }
}

impl QRead<ParamFilterMatch> for ParamFilterMatch {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        if xml.maybe_open(CARD_URN, "is-not-defined").await?.is_some() {
            xml.close().await?;
            return Ok(Self::IsNotDefined);
        }
        TextMatch::qread(xml).await.map(Self::Match)
    }
}

impl QRead<Limit> for Limit {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        let mut nresults = None;
        xml.open(CARD_URN, "limit").await?;
        loop {
            if xml.maybe_open(CARD_URN, "nresults").await?.is_some() {
                let text = xml.tag_string().await?;
                nresults = Some(
                    u64::from_str(text.trim())
                        .map_err(|_| ParsingError::InvalidValue)?
                );
                xml.close().await?;
                break;
            }
            match xml.peek() {
                Event::End(_) => break,
                _ => xml.skip().await?,
            };
        }
        xml.close().await?;
        if let Some(nresults) = nresults {
            Ok(Self { nresults })
        } else {
            Err(ParsingError::Recoverable)
        }
    }
}

impl QRead<PropKind> for PropKind {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        let mut prop = Vec::new();
        loop {
            let mut dirty = false;
            if xml.maybe_open(CARD_URN, "allprop").await?.is_some() {
                xml.close().await?;
                return Ok(PropKind::AllProp);
            }
            xml.maybe_push(&mut prop, &mut dirty).await?;

            if !dirty {
                match xml.peek() {
                    Event::End(_) => break,
                    _ => xml.skip().await?,
                };
            }
        }

        match &prop[..] {
            [] => Err(ParsingError::Recoverable),
            _ => Ok(PropKind::Prop(prop)),
        }
    }
}

impl QRead<CardProp> for CardProp {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(CARD_URN, "prop").await?;
        let name = PropertyName::from_str(
            &xml.prev_attr("name")
                .ok_or(ParsingError::MissingAttribute)?,
        ).map_err(|()| ParsingError::InvalidValue)?;
        let novalue = WithDefault::from_opt(
            xml.prev_attr("novalue")
               .map(|s| NoValue::from_str(&s))
               .transpose()
               .map_err(|()| ParsingError::InvalidValue)?
        );
        xml.close().await?;
        Ok(Self { name, novalue })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::realization::Addressbook;
    use crate::xml::Node;
    use pretty_assertions::assert_eq;
    
    async fn deserialize<T: Node<T>>(src: &str) -> T {
        let mut rdr = Reader::new(quick_xml::NsReader::from_reader(src.as_bytes()))
            .await
            .unwrap();
        rdr.find().await.unwrap()
    }
    
    #[tokio::test]
    async fn rfc_principal_address() {
        let expected = Property::PrincipalAddress(dav::Href("/system/cyrus.vcf".to_string()));

        let src = r#"
       <C:principal-address xmlns:D="DAV:"
          xmlns:C="urn:ietf:params:xml:ns:carddav">
          <D:href>/system/cyrus.vcf</D:href>
       </C:principal-address>
"#;
        
        let got = deserialize::<Property>(src).await;

        assert_eq!(got, expected)
    }

    #[tokio::test]
    async fn rfc_supported_collation_set() {
        let expected = Property::SupportedCollationSet(vec![
            SupportedCollation(Collation::AsciiCaseMap),
            SupportedCollation(Collation::Unknown("i;octet".to_string())),
            SupportedCollation(Collation::UnicodeCaseMap),
        ]);

        let src = r#"
      <C:supported-collation-set
        xmlns:C="urn:ietf:params:xml:ns:carddav">
        <C:supported-collation>i;ascii-casemap</C:supported-collation>
        <C:supported-collation>i;octet</C:supported-collation>
        <C:supported-collation>i;unicode-casemap</C:supported-collation>
      </C:supported-collation-set>
"#;
        
        let got = deserialize::<Property>(src).await;

        assert_eq!(got, expected)
    }

    // §8.6.3, query
    #[tokio::test]
    async fn rfc_addressbook_query_8_6_3() {
        let expected = AddressbookQuery {
            selector: Some(AddressbookSelector::Prop(dav::PropName(vec![
                dav::PropertyRequest::GetEtag,
                dav::PropertyRequest::Extension(PropertyRequest::AddressData(
                    AddressDataRequest {
                        content_type: Default::default(),
                        version: Default::default(),
                        prop_kind: Some(PropKind::Prop(vec![
                            CardProp {
                                name: PropertyName { group: None, name: "VERSION".into() },
                                novalue: Default::default(),
                            },
                            CardProp {
                                name: PropertyName { group: None, name: "UID".into() },
                                novalue: Default::default(),
                            },
                            CardProp {
                                name: PropertyName { group: None, name: "NICKNAME".into() },
                                novalue: Default::default(),
                            },
                            CardProp {
                                name: PropertyName{ group: None, name: "EMAIL".into() },
                                novalue: Default::default(),
                            },
                            CardProp {
                                name: PropertyName{ group: None, name: "FN".into() },
                                novalue: Default::default(),
                            },
                        ])),
                    }
                )),
            ]))),
            filter: Filter {
                prop_filters: vec![PropFilter {
                    name: PropertyName { group: None, name: "NICKNAME".to_string() },
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

        let src = r#"
   <?xml version="1.0" encoding="utf-8" ?>
   <C:addressbook-query xmlns:D="DAV:"
                     xmlns:C="urn:ietf:params:xml:ns:carddav">
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
         <C:text-match collation="i;unicode-casemap"
                       match-type="equals"
         >me</C:text-match>
       </C:prop-filter>
     </C:filter>
   </C:addressbook-query>
"#;
        
        let got = deserialize::<AddressbookQuery<Addressbook>>(src).await;

        assert_eq!(got, expected)
    }

    // §8.6.3, response
    #[tokio::test]
    async fn rfc_addressbook_query_res_8_6_3() {
        let expected = dav::Multistatus::<Addressbook> {
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
        
        let src = r#"
   <?xml version="1.0" encoding="utf-8" ?>
   <D:multistatus xmlns:D="DAV:"
                  xmlns:C="urn:ietf:params:xml:ns:carddav">
     <D:response>
       <D:href>/home/bernard/addressbook/v102.vcf</D:href>
       <D:propstat>
         <D:prop>
           <D:getetag>"23ba4d-ff11fb"</D:getetag>
           <C:address-data>BEGIN:VCARD</C:address-data>
         </D:prop>
         <D:status>HTTP/1.1 200 OK</D:status>
       </D:propstat>
     </D:response>
   </D:multistatus>
"#;

        let got = deserialize::<dav::Multistatus<Addressbook>>(src).await;
        assert_eq!(got, expected);
    }

    // §8.6.4, query
    #[tokio::test]
    async fn rfc_addressbook_query_8_6_4() {
        let expected = AddressbookQuery {
            selector: Some(AddressbookSelector::Prop(dav::PropName(vec![
                dav::PropertyRequest::GetEtag,
                dav::PropertyRequest::Extension(PropertyRequest::AddressData(
                    AddressDataRequest {
                        content_type: Default::default(),
                        version: Default::default(),
                        prop_kind: Some(PropKind::Prop(vec![
                            CardProp {
                                name: PropertyName { group: None, name: "VERSION".into() },
                                novalue: Default::default(),
                            },
                            CardProp {
                                name: PropertyName { group: None, name: "UID".into() },
                                novalue: Default::default(),
                            },
                            CardProp {
                                name: PropertyName { group: None, name: "NICKNAME".into() },
                                novalue: Default::default(),
                            },
                            CardProp {
                                name: PropertyName { group: None, name: "EMAIL".into() },
                                novalue: Default::default(),
                            },
                            CardProp {
                                name: PropertyName { group: None, name: "FN".into() },
                                novalue: Default::default(),
                            },
                        ])),
                    }
                )),
            ]))),
            filter: Filter {
                prop_filters: vec![
                    PropFilter {
                        name: PropertyName { group: None, name: "FN".to_string() },
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
                        name: PropertyName { group: None, name: "EMAIL".to_string() },
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

        let src = r#"
   <?xml version="1.0" encoding="utf-8" ?>
   <C:addressbook-query xmlns:D="DAV:"
                     xmlns:C="urn:ietf:params:xml:ns:carddav">
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
         <C:text-match collation="i;unicode-casemap"
                       match-type="contains"
         >daboo</C:text-match>
       </C:prop-filter>
       <C:prop-filter name="EMAIL">
         <C:text-match collation="i;unicode-casemap"
                       match-type="contains"
         >daboo</C:text-match>
       </C:prop-filter>
     </C:filter>
   </C:addressbook-query>
"#;
        
        let got = deserialize::<AddressbookQuery<Addressbook>>(src).await;

        assert_eq!(got, expected)
    }

    // §8.6.4, response
    #[tokio::test]
    async fn rfc_addressbook_query_res_8_6_4() {
        let expected = dav::Multistatus::<Addressbook> {
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
        
        let src = r#"
   <?xml version="1.0" encoding="utf-8" ?>
   <D:multistatus xmlns:D="DAV:"
                  xmlns:C="urn:ietf:params:xml:ns:carddav">
     <D:response>
       <D:href>/home/bernard/addressbook/v102.vcf</D:href>
       <D:propstat>
         <D:prop>
           <D:getetag>"23ba4d-ff11fb"</D:getetag>
           <C:address-data>BEGIN:VCARD</C:address-data>
         </D:prop>
         <D:status>HTTP/1.1 200 OK</D:status>
       </D:propstat>
     </D:response>
     <D:response>
       <D:href>/home/bernard/addressbook/v104.vcf</D:href>
       <D:propstat>
         <D:prop>
           <D:getetag>"23ba4d-ff11fc"</D:getetag>
           <C:address-data>BEGIN:VCARD</C:address-data>
         </D:prop>
         <D:status>HTTP/1.1 200 OK</D:status>
       </D:propstat>
     </D:response>
   </D:multistatus>
"#;

        let got = deserialize::<dav::Multistatus<Addressbook>>(src).await;
        assert_eq!(got, expected);
    }

    // §8.6.5, query
    #[tokio::test]
    async fn rfc_addressbook_query_8_6_5() {
        let expected = AddressbookQuery {
            selector: Some(AddressbookSelector::Prop(dav::PropName(vec![
                dav::PropertyRequest::GetEtag,
            ]))),
            filter: Filter {
                prop_filters: vec![
                    PropFilter {
                        name: PropertyName { group: None, name: "FN".to_string() },
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

        let src = r#"
   <?xml version="1.0" encoding="utf-8" ?>
   <C:addressbook-query xmlns:D="DAV:"
                     xmlns:C="urn:ietf:params:xml:ns:carddav">
     <D:prop>
       <D:getetag/>
     </D:prop>
     <C:filter test="anyof">
       <C:prop-filter name="FN">
         <C:text-match collation="i;unicode-casemap"
                       match-type="contains"
         >daboo</C:text-match>
       </C:prop-filter>
     </C:filter>
     <C:limit>
       <C:nresults>
         2
       </C:nresults>
     </C:limit>
   </C:addressbook-query>
"#;
        
        let got = deserialize::<AddressbookQuery<Addressbook>>(src).await;

        assert_eq!(got, expected)
    }

    // §8.6.5, response
    #[tokio::test]
    async fn rfc_addressbook_query_res_8_6_5() {
        let expected = dav::Multistatus::<Addressbook> {
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
        
        let src = r#"
   <?xml version="1.0" encoding="utf-8" ?>
   <D:multistatus xmlns:D="DAV:"
                  xmlns:C="urn:ietf:params:xml:ns:carddav">
     <D:response>
       <D:href>/home/bernard/addressbook/</D:href>
       <D:status>HTTP/1.1 507 Insufficient Storage</D:status>
       <D:error><D:number-of-matches-within-limits/></D:error>
       <D:responsedescription xml:lang="en">
         Only two matching records were returned
       </D:responsedescription>
     </D:response>
     <D:response>
       <D:href>/home/bernard/addressbook/v102.vcf</D:href>
       <D:propstat>
         <D:prop>
           <D:getetag>"23ba4d-ff11fb"</D:getetag>
         </D:prop>
         <D:status>HTTP/1.1 200 OK</D:status>
       </D:propstat>
     </D:response>
     <D:response>
       <D:href>/home/bernard/addressbook/v104.vcf</D:href>
       <D:propstat>
         <D:prop>
           <D:getetag>"23ba4d-ff11fc"</D:getetag>
         </D:prop>
         <D:status>HTTP/1.1 200 OK</D:status>
       </D:propstat>
     </D:response>
   </D:multistatus>
"#;

        let got = deserialize::<dav::Multistatus<Addressbook>>(src).await;
        assert_eq!(got, expected);
    }

    // §8.7.1, query
    #[tokio::test]
    async fn rfc_multiget_query_8_7_1() {
        let expected = AddressbookMultiget {
            selector: Some(AddressbookSelector::Prop(dav::PropName(vec![
                dav::PropertyRequest::GetEtag,
                dav::PropertyRequest::Extension(PropertyRequest::AddressData(
                    AddressDataRequest {
                        content_type: Default::default(),
                        version: Default::default(),
                        prop_kind: Some(PropKind::Prop(vec![
                            CardProp {
                                name: PropertyName { group: None, name: "VERSION".into() },
                                novalue: Default::default(),
                            },
                            CardProp {
                                name: PropertyName { group: None, name: "UID".into() },
                                novalue: Default::default(),
                            },
                            CardProp {
                                name: PropertyName { group: None, name: "NICKNAME".into() },
                                novalue: Default::default(),
                            },
                            CardProp {
                                name: PropertyName { group: None, name: "EMAIL".into() },
                                novalue: Default::default(),
                            },
                            CardProp {
                                name: PropertyName { group: None, name: "FN".into() },
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

        let src = r#"
   <?xml version="1.0" encoding="utf-8" ?>
   <C:addressbook-multiget xmlns:D="DAV:"
                        xmlns:C="urn:ietf:params:xml:ns:carddav">
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
   </C:addressbook-multiget>
"#;
        
        let got = deserialize::<AddressbookMultiget<Addressbook>>(src).await;

        assert_eq!(got, expected)
    }

    // §8.7.1, response
    #[tokio::test]
    async fn rfc_multiget_query_res_8_7_1() {
        let expected = dav::Multistatus::<Addressbook> {
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
        
        let src = r#"
   <?xml version="1.0" encoding="utf-8" ?>
   <D:multistatus xmlns:D="DAV:"
                  xmlns:C="urn:ietf:params:xml:ns:carddav">
     <D:response>
       <D:href>/home/bernard/addressbook/vcf102.vcf</D:href>
       <D:propstat>
         <D:prop>
           <D:getetag>"23ba4d-ff11fb"</D:getetag>
           <C:address-data>BEGIN:VCARD</C:address-data>
         </D:prop>
         <D:status>HTTP/1.1 200 OK</D:status>
       </D:propstat>
     </D:response>
     <D:response>
       <D:href>/home/bernard/addressbook/vcf1.vcf</D:href>
       <D:status>HTTP/1.1 404 Resource not found</D:status>
     </D:response>
   </D:multistatus>
"#;

        let got = deserialize::<dav::Multistatus<Addressbook>>(src).await;
        assert_eq!(got, expected);
    }

    // §8.7.2, query
    #[tokio::test]
    async fn rfc_multiget_query_8_7_2() {
        let expected = AddressbookMultiget {
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

        let src = r#"
 <?xml version="1.0" encoding="utf-8" ?>
   <C:addressbook-multiget xmlns:D="DAV:"
                        xmlns:C="urn:ietf:params:xml:ns:carddav">
     <D:prop>
       <D:getetag/>
       <C:address-data content-type='text/vcard' version='4.0'/>
     </D:prop>
     <D:href>/home/bernard/addressbook/vcf3.vcf</D:href>
   </C:addressbook-multiget>
"#;
        
        let got = deserialize::<AddressbookMultiget<Addressbook>>(src).await;

        assert_eq!(got, expected)
    }

    // §8.7.1, response
    #[tokio::test]
    async fn rfc_multiget_query_res_8_7_2() {
        let expected = dav::Multistatus::<Addressbook> {
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
        
        let src = r#"
   <?xml version="1.0" encoding="utf-8" ?>
   <D:multistatus xmlns:D="DAV:"
                  xmlns:C="urn:ietf:params:xml:ns:carddav">
     <D:response>
       <D:href>/home/bernard/addressbook/vcf3.vcf</D:href>
       <D:status>HTTP/1.1 415 Unsupported Media Type</D:status>
       <D:error><C:supported-address-data-conversion/></D:error>
       <D:responsedescription>Unable to convert from vCard v3.0
       to vCard v4.0</D:responsedescription>
     </D:response>
   </D:multistatus>
"#;

        let got = deserialize::<dav::Multistatus<Addressbook>>(src).await;
        assert_eq!(got, expected);
    }
}
