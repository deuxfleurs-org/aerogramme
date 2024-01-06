use imap_codec::imap_types::fetch::{MacroOrMessageDataItemNames, MessageDataItemName};

/// Internal decisions based on fetched attributes
/// passed by the client

pub struct AttributesProxy {
    pub attrs: Vec<MessageDataItemName<'static>>,
}
impl AttributesProxy {
    pub fn new(attrs: &MacroOrMessageDataItemNames<'static>, is_uid_fetch: bool) -> Self {
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

        // Handle uids
        if is_uid_fetch && !fetch_attrs.contains(&MessageDataItemName::Uid) {
            fetch_attrs.push(MessageDataItemName::Uid);
        }

        Self { attrs: fetch_attrs }
    }

    pub fn need_body(&self) -> bool {
        self.attrs.iter().any(|x| {
            matches!(
                x,
                MessageDataItemName::Body
                    | MessageDataItemName::BodyExt { .. }
                    | MessageDataItemName::Rfc822
                    | MessageDataItemName::Rfc822Text
                    | MessageDataItemName::BodyStructure
            )
        })
    }
}
