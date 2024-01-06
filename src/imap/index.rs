use std::num::NonZeroU32;

use anyhow::{anyhow, bail, Result};
use imap_codec::imap_types::sequence::{self, SequenceSet};

use crate::mail::uidindex::{ImapUid, UidIndex};
use crate::mail::unique_ident::UniqueIdent;

pub struct Index<'a>(pub &'a UidIndex);
impl<'a> Index<'a> {
    pub fn fetch(
        self: &Index<'a>,
        sequence_set: &SequenceSet,
        by_uid: bool,
    ) -> Result<Vec<MailIndex<'a>>> {
        let mail_vec = self
            .0
            .idx_by_uid
            .iter()
            .map(|(uid, uuid)| (*uid, *uuid))
            .collect::<Vec<_>>();

        let mut mails = vec![];

        if by_uid {
            if mail_vec.is_empty() {
                return Ok(vec![]);
            }
            let iter_strat = sequence::Strategy::Naive {
                largest: mail_vec.last().unwrap().0,
            };

            let mut i = 0;
            for uid in sequence_set.iter(iter_strat) {
                while mail_vec.get(i).map(|mail| mail.0 < uid).unwrap_or(false) {
                    i += 1;
                }
                if let Some(mail) = mail_vec.get(i) {
                    if mail.0 == uid {
                        mails.push(MailIndex {
                            i: NonZeroU32::try_from(i as u32 + 1).unwrap(),
                            uid: mail.0,
                            uuid: mail.1,
                            flags: self
                                .0
                                .table
                                .get(&mail.1)
                                .ok_or(anyhow!("mail is missing from index"))?
                                .1
                                .as_ref(),
                        });
                    }
                } else {
                    break;
                }
            }
        } else {
            if mail_vec.is_empty() {
                bail!("No such message (mailbox is empty)");
            }

            let iter_strat = sequence::Strategy::Naive {
                largest: NonZeroU32::try_from((mail_vec.len()) as u32).unwrap(),
            };

            for i in sequence_set.iter(iter_strat) {
                if let Some(mail) = mail_vec.get(i.get() as usize - 1) {
                    mails.push(MailIndex {
                        i,
                        uid: mail.0,
                        uuid: mail.1,
                        flags: self
                            .0
                            .table
                            .get(&mail.1)
                            .ok_or(anyhow!("mail is missing from index"))?
                            .1
                            .as_ref(),
                    });
                } else {
                    bail!("No such mail: {}", i);
                }
            }
        }

        Ok(mails)
    }
}

pub struct MailIndex<'a> {
    pub i: NonZeroU32,
    pub uid: ImapUid,
    pub uuid: UniqueIdent,
    pub flags: &'a Vec<String>,
}
