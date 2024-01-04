use std::iter::zip;

use anyhow::{anyhow, Result};


use crate::cryptoblob::Key;
use crate::imap::mail_view::{MailView, FetchedMail};
use crate::imap::mailbox_view::MailIdentifiers;
use crate::mail::mailbox::MailMeta;
use crate::mail::unique_ident::UniqueIdent;

pub struct BodyIdentifier<'a> {
    pub msg_uuid: &'a UniqueIdent,
    pub msg_key: &'a Key,
}


#[derive(Default)]
pub struct MailSelectionBuilder<'a> {
    //attrs: AttributeProxy,
    mail_count: usize,
    need_body: bool,
    mi: &'a [MailIdentifiers],
    meta: &'a [MailMeta],
    flags: &'a [&'a Vec<String>],
    bodies: &'a [Vec<u8>],
}

impl<'a> MailSelectionBuilder<'a> {
    pub fn new(need_body: bool, mail_count: usize) -> Self {
        Self {
            mail_count,
            need_body,
            ..MailSelectionBuilder::default()
        }
    }

    pub fn with_mail_identifiers(&mut self, mi: &'a [MailIdentifiers]) -> &mut Self {
        self.mi = mi;
        self
    }

    pub fn with_metadata(&mut self, meta: &'a [MailMeta]) -> &mut Self {
        self.meta = meta;
        self
    }

    pub fn with_flags(&mut self, flags: &'a [&'a Vec<String>]) -> &mut Self {
        self.flags = flags;
        self
    }

    pub fn bodies_to_collect(&self) -> Vec<BodyIdentifier> {
        if !self.need_body {
            return vec![];
        }
        zip(self.mi, self.meta)
            .map(|(mi, meta)| BodyIdentifier {
                msg_uuid: &mi.uuid,
                msg_key: &meta.message_key,
            })
            .collect::<Vec<_>>()
    }

    pub fn with_bodies(&mut self, rbodies: &'a [Vec<u8>]) -> &mut Self {
        self.bodies = rbodies;
        self
    }

    pub fn build(&self) -> Result<Vec<MailView<'a>>> {
        let mut bodies = vec![];

        if !self.need_body {
            for m in self.meta.iter() {
                let (_, hdrs) =
                    eml_codec::parse_imf(&m.headers).or(Err(anyhow!("Invalid mail headers")))?;
                bodies.push(FetchedMail::Partial(hdrs));
            }
        } else {
            for rb in self.bodies.iter() {
                let (_, p) = eml_codec::parse_message(&rb).or(Err(anyhow!("Invalid mail body")))?;
                bodies.push(FetchedMail::new_from_message(p));
            }
        }

        if self.mi.len() != self.mail_count && self.meta.len() != self.mail_count
            || self.flags.len() != self.mail_count
            || bodies.len() != self.mail_count
        {
            return Err(anyhow!("Can't build a mail view selection as parts were not correctly registered into the builder."));
        }

        Ok(zip(self.mi, zip(self.meta, zip(self.flags, bodies)))
            .map(|(ids, (meta, (flags, content)))| MailView {
                ids,
                meta,
                flags,
                content,
            })
            .collect())
    }
}

