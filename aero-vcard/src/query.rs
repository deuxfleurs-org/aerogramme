use aero_dav::cardtypes as card;
use ical_vcard::{Contentline, Param};

use crate::filter::prop_matches_name;

// NOTE: the filtering logic is naive and could be optimized

pub fn object_matches_filter(vcard: &[Contentline], filter: &card::Filter) -> bool {
    tests(*filter.test.get(), &filter.prop_filters, |prop_filter| {
        object_matches_prop_filter(vcard, prop_filter)
    })
}

fn object_matches_prop_filter(vcard: &[Contentline], filter: &card::PropFilter) -> bool {
    match &filter.rules {
        card::PropFilterRules::Empty =>
            vcard.iter().any(|prop| prop_matches_name(prop, &filter.name)),
        card::PropFilterRules::IsNotDefined =>
            vcard.iter().all(|prop| !prop_matches_name(prop, &filter.name)),
        card::PropFilterRules::Match { text_match, param_filter } =>
            vcard.iter().any(|prop| {
                prop_matches_name(prop, &filter.name) &&
                    test2(
                        *filter.test.get(),
                        || tests(*filter.test.get(), &text_match,
                                 |tm| is_text_match(prop.value(), tm)),
                        || tests(*filter.test.get(), &param_filter,
                                 |pf| params_match_filter(prop.params(), pf)),
                    )
            }),
    }
}

fn is_text_match(s: &str, text_match: &card::TextMatch) -> bool {
    //@FIXME ignoring collation
    let pat = text_match.text.as_str();
    let matches = match text_match.match_type.get() {
        card::TextMatchType::Equals => s == pat,
        card::TextMatchType::Contains => s.contains(pat),
        card::TextMatchType::StartsWith => s.starts_with(pat),
        card::TextMatchType::EndsWith => s.ends_with(pat),
    };
    match text_match.negate_condition.get() {
        card::NegateCondition::Yes => !matches,
        card::NegateCondition::No => matches,
    }
}

fn params_match_filter(params: &[Param], param_filter: &card::ParamFilter) -> bool {
    match &param_filter.rules {
        None =>
            params.iter().any(|param| param.name() == param_filter.name.0.as_str()),
        Some(card::ParamFilterMatch::IsNotDefined) =>
            params.iter().all(|param| param.name() != param_filter.name.0.as_str()),
        Some(card::ParamFilterMatch::Match(text_match)) =>
            params.iter().any(|param| {
                param.name() == param_filter.name.0.as_str()
                // NOTE: the RFC does not specify the behavior of param-filter
                // on multi-valued parameters. We use "any of" somewhat
                // arbitrarily (it is more permissive).
                    && param.values().iter().any(|val| is_text_match(val.as_str(), text_match)) 
            })
    }
}

fn tests<T>(test: card::FilterTest, items: &[T], f: impl Fn(&T) -> bool) -> bool {
    match test {
        card::FilterTest::AllOf => items.iter().all(f),
        card::FilterTest::AnyOf => items.iter().any(f),
    }
}

fn test2(test: card::FilterTest, f1: impl Fn() -> bool, f2: impl Fn() -> bool) -> bool {
    match test {
        card::FilterTest::AllOf => f1() && f2(),
        card::FilterTest::AnyOf => f1() || f2(),
    }
}
