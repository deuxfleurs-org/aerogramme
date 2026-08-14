use aero_dav::caltypes::Collation;

pub fn normalize<'a>(c: Collation, s: &'a str) -> Vec<u8> {
    match c {
        Collation::Octet => s.as_bytes().into(),
        Collation::AsciiCaseMap => s.to_ascii_uppercase().into_bytes(),
    }
}

pub fn contains_subslice(s: &[u8], pat: &[u8]) -> bool {
    s.windows(pat.len()).any(|window| window == pat)
}
