use super::cardtypes::*;
use super::error::ParsingError;
use super::xml::{IRead, QRead, Reader, CARD_URN};

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
