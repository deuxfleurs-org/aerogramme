use quick_xml::events::Event;
use std::str::FromStr;

use super::cardtypes::*;
use super::coretypes as dav;
use super::error::ParsingError;
use super::extension::Extension;
use super::xml::{IRead, QRead, Reader, CARD_URN, DAV_URN};

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
        return Err(ParsingError::Recoverable)
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
        let content_type = xml.prev_attr("content-type");
        let version = xml.prev_attr("version");
        let prop_kind = xml.maybe_find().await?;
        xml.close().await?;
        Ok(Self { prop_kind, content_type, version })
    }
}

impl QRead<AddressDataPayload> for AddressDataPayload {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(CARD_URN, "address-data").await?;
        let content_type = xml.prev_attr("content-type");
        let version = xml.prev_attr("version");
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
        let ct = xml.prev_attr("content-type");
        let vs = xml.prev_attr("version");
        let (content_type, version) = match (ct, vs) {
            (Some(content_type), Some(version)) => (content_type, version),
            _ => return Err(ParsingError::Recoverable),
        };
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
        let test = xml
            .prev_attr("test")
            .map(|s| FilterTest::from_str(&s))
            .transpose()
            .map_err(|()| ParsingError::InvalidValue)?;
        let prop_filters = xml.collect().await?;
        xml.close().await?;
        Ok(Self { prop_filters, test })
    }
}

impl QRead<PropFilter> for PropFilter {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(CARD_URN, "prop-filter").await?;
        let name = PropertyName(
            xml.prev_attr("name")
                .ok_or(ParsingError::MissingAttribute)?,
        );
        let test = xml.prev_attr("test")
                      .map(|s| FilterTest::from_str(&s))
                      .transpose()
                      .map_err(|()| ParsingError::InvalidValue)?;
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
        let collation = xml.prev_attr("collation").map(Collation::new);
        let negate_condition = xml.prev_attr("negate-condition").map(|v| v == "yes");
        let match_type = xml.prev_attr("match-type")
                            .map(|s| TextMatchType::from_str(&s))
                            .transpose()
                            .map_err(|()| ParsingError::InvalidValue)?;
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
        xml.open(CARD_URN, "limit").await?;
        xml.open(CARD_URN, "nresults").await?;
        let nresults = u64::from_str(&xml.tag_string().await?)
            .map_err(|_| ParsingError::InvalidValue)?;
        xml.close().await?;
        xml.close().await?;
        Ok(Self { nresults })
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
                break;
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
        let name = PropertyName(
            xml.prev_attr("name")
                .ok_or(ParsingError::MissingAttribute)?,
        );
        let novalue = xml.prev_attr("novalue").map(|v| v == "yes");
        xml.close().await?;
        Ok(Self { name, novalue })
    }
}
