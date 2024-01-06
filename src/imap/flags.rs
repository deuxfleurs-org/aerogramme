use imap_codec::imap_types::core::Atom;
use imap_codec::imap_types::flag::{Flag, FlagFetch};

pub fn from_str(f: &str) -> Option<FlagFetch<'static>> {
    match f.chars().next() {
        Some('\\') => match f {
            "\\Seen" => Some(FlagFetch::Flag(Flag::Seen)),
            "\\Answered" => Some(FlagFetch::Flag(Flag::Answered)),
            "\\Flagged" => Some(FlagFetch::Flag(Flag::Flagged)),
            "\\Deleted" => Some(FlagFetch::Flag(Flag::Deleted)),
            "\\Draft" => Some(FlagFetch::Flag(Flag::Draft)),
            "\\Recent" => Some(FlagFetch::Recent),
            _ => match Atom::try_from(f.strip_prefix('\\').unwrap().to_string()) {
                Err(_) => {
                    tracing::error!(flag=%f, "Unable to encode flag as IMAP atom");
                    None
                }
                Ok(a) => Some(FlagFetch::Flag(Flag::system(a))),
            },
        },
        Some(_) => match Atom::try_from(f.to_string()) {
            Err(_) => {
                tracing::error!(flag=%f, "Unable to encode flag as IMAP atom");
                None
            }
            Ok(a) => Some(FlagFetch::Flag(Flag::keyword(a))),
        },
        None => None,
    }
}
