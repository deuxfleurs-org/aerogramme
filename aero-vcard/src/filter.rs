use aero_dav::cardtypes as card;
use ical_vcard::Contentline;

pub fn property(src: &Contentline, prune: &card::PropKind) -> Option<Contentline> {
    match prune {
        card::PropKind::AllProp => Some(src.clone()),
        card::PropKind::Prop(props) => {
            let cardprop = props.iter().find(|p| prop_matches_name(src, &p.name))?;
            match cardprop.novalue.get() {
                card::NoValue::Yes => Some(unset_value(src)),
                card::NoValue::No => Some(src.clone()),
            }
        }
    }
}

/// From card::CardProp:
///
///    vCard allows a "group" prefix to appear before a property name in
///    the vCard data.  When the "name" attribute does not specify a
///    group prefix, it MUST match properties in the vCard data without a
///    group prefix or with any group prefix.  When the "name" attribute
///    includes a group prefix, it MUST match properties that have
///    exactly the same group prefix and name.  For example, a "name" set
///    to "TEL" will match "TEL", "X-ABC.TEL", and "X-ABC-1.TEL" vCard
///    properties.  A "name" set to "X-ABC.TEL" will match an "X-ABC.TEL"
///    vCard property only; it will not match "TEL" or "X-ABC-1.TEL".
pub fn prop_matches_name(src: &Contentline, pname: &card::PropertyName) -> bool {
    src.name() == pname.name &&
        match &pname.group {
            None => true,
            Some(g) => src.group() == Some(g.as_str())
        }
}

/// From card::CardProp:
///
///    The "novalue" attribute can be used by clients to request that the actual
///    value of the property not be returned (if the "novalue" attribute is set
///    to "yes"). In that case, the server will return just the vCard property
///    name and any vCard parameters and a trailing ":" without the subsequent
///    value data.
fn unset_value(src: &Contentline) -> Contentline {
    let mut line = Contentline::new(src.name(), "");
    if let Some(g) = src.group() {
        line = line.set_group(g)
    }
    line.set_params(src.params().to_owned())
}
