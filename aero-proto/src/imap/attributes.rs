use imap_codec::imap_types::command::FetchModifier;
use imap_codec::imap_types::fetch::{MacroOrMessageDataItemNames, MessageDataItemName, Section};

use aero_collections::mail::query::QueryScope;

/// Internal decisions based on fetched attributes
/// passed by the client

pub struct AttributesProxy {
    pub attrs: Vec<MessageDataItemName<'static>>,
}
impl AttributesProxy {
    pub fn new(
        attrs: &MacroOrMessageDataItemNames<'static>,
        modifiers: &[FetchModifier],
        is_uid_fetch: bool,
    ) -> Self {
        // Expand macros
        let mut fetch_attrs = match attrs {
            MacroOrMessageDataItemNames::Macro(m) => {
                use imap_codec::imap_types::fetch::Macro;
                use MessageDataItemName::*;
                match m {
                    Macro::All => vec![Flags, InternalDate, Rfc822Size, Envelope],
                    Macro::Fast => vec![Flags, InternalDate, Rfc822Size],
                    Macro::Full => vec![Flags, InternalDate, Rfc822Size, Envelope, Body],
                    _ => {
                        tracing::error!("unimplemented macro");
                        vec![]
                    }
                }
            }
            MacroOrMessageDataItemNames::MessageDataItemNames(a) => a.clone(),
        };

        // Include a UID request if this is a UID command
        //
        // "server implementations MUST implicitly include the UID message data
        // item as part of any FETCH response caused by a UID command,
        // regardless of whether a UID was specified as a message data item to
        // the FETCH."
        if is_uid_fetch && !fetch_attrs.contains(&MessageDataItemName::Uid) {
            fetch_attrs.push(MessageDataItemName::Uid);
        }

        // Handle inferred MODSEQ tag
        let is_changed_since = modifiers
            .iter()
            .any(|m| matches!(m, FetchModifier::ChangedSince(..)));
        if is_changed_since && !fetch_attrs.contains(&MessageDataItemName::ModSeq) {
            fetch_attrs.push(MessageDataItemName::ModSeq);
        }

        Self { attrs: fetch_attrs }
    }

    pub fn is_enabling_condstore(&self) -> bool {
        self.attrs
            .iter()
            .any(|x| matches!(x, MessageDataItemName::ModSeq))
    }

    pub fn scope(&self) -> QueryScope {
        self.attrs.iter().map(|x| match x {
            MessageDataItemName::Body
            | MessageDataItemName::Rfc822
            | MessageDataItemName::Rfc822Text
            | MessageDataItemName::BodyStructure => QueryScope::Full,

            MessageDataItemName::BodyExt {
                section: Some(section),
                partial: _,
                peek: _,
            } => match section {
                Section::Header(None)
                | Section::HeaderFields(None, _)
                | Section::HeaderFieldsNot(None, _) => QueryScope::Partial,
                _ => QueryScope::Full,
            },
            MessageDataItemName::BodyExt { .. } => QueryScope::Full,

            MessageDataItemName::Envelope
            | MessageDataItemName::InternalDate
            | MessageDataItemName::Rfc822Header
            | MessageDataItemName::Rfc822Size => QueryScope::Partial,

            MessageDataItemName::Flags
            | MessageDataItemName::Uid
            | MessageDataItemName::ModSeq => QueryScope::Index,
        }).reduce(|q1, q2| q1.union(&q2)).unwrap_or(QueryScope::Index)
    }
}
