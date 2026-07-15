use aero_dav::cardtypes::Collation;

// TODO: merge with aero-ical/collation? (merging aero-ical and aero-vcard)

pub fn normalize<'a>(c: Collation, s: &'a str) -> Vec<u8> {
    match c {
        Collation::AsciiCaseMap => s.to_ascii_uppercase().into_bytes(),
        Collation::UnicodeCaseMap => normalize_unicode(s),
    }
}

pub fn contains_subslice(s: &[u8], pat: &[u8]) -> bool {
    s.windows(pat.len()).any(|window| window == pat)
}

/// RFC5051 normalization for i;unicode-casemap
fn normalize_unicode(s: &str) -> Vec<u8> {
    let mut titlecased = String::new();
    for c in s.chars() {
        // unicode_case_mapping has a fairly obtuse API
        let tcbytes = unicode_case_mapping::to_titlecase(c);
        // "A result of all zeros indicates the codepoint maps to itself."
        if tcbytes[0] == 0 {
            titlecased.push(c)
        } else {
            // "Unused elements in the returned array are set to 0."
            for b in tcbytes {
                if b != 0 {
                    titlecased.push(std::char::from_u32(b).unwrap());
                }
            }
        }
    }
    use unicode_normalization::UnicodeNormalization;
    titlecased.nfkd().collect::<String>().into_bytes()
}
