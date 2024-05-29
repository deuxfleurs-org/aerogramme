use aero_dav::caltypes as cal;
use icalendar::parser::{Component, Property};

pub fn component<'a>(src: &'a Component<'a>, prune: &cal::Comp) -> Option<Component<'a>> {
    if src.name.as_str() != prune.name.as_str() {
        return None;
    }

    let name = src.name.clone();

    let properties = match &prune.prop_kind {
        Some(cal::PropKind::AllProp) | None => src.properties.clone(),
        Some(cal::PropKind::Prop(l)) => src
            .properties
            .iter()
            .filter_map(|prop| {
                let sel_filt = match l
                    .iter()
                    .find(|filt| filt.name.0.as_str() == prop.name.as_str())
                {
                    Some(v) => v,
                    None => return None,
                };

                match sel_filt.novalue {
                    None | Some(false) => Some(prop.clone()),
                    Some(true) => Some(Property {
                        name: prop.name.clone(),
                        params: prop.params.clone(),
                        val: "".into(),
                    }),
                }
            })
            .collect::<Vec<_>>(),
    };

    let components = match &prune.comp_kind {
        Some(cal::CompKind::AllComp) | None => src.components.clone(),
        Some(cal::CompKind::Comp(many_inner_prune)) => src
            .components
            .iter()
            .filter_map(|src_component| {
                many_inner_prune
                    .iter()
                    .find_map(|inner_prune| component(src_component, inner_prune))
            })
            .collect::<Vec<_>>(),
    };

    Some(Component {
        name,
        properties,
        components,
    })
}
