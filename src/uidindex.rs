use im::{HashMap, HashSet, OrdMap};
use serde::{de::Error, Deserialize, Deserializer, Serialize, Serializer};

use crate::bayou::*;

pub type ImapUid = u32;
pub type ImapUidvalidity = u32;
pub type Flag = String;

/// A Mail UUID is composed of two components:
/// - a process identifier, 128 bits
/// - a sequence number, 64 bits
#[derive(Clone, Copy, PartialOrd, Ord, PartialEq, Eq, Hash, Debug)]
pub struct MailUuid(pub [u8; 24]);

#[derive(Clone)]
/// A UidIndex handles the mutable part of a mailbox
/// It is built by running the event log on it
/// Each applied log generates a new UidIndex by cloning the previous one
/// and applying the event. This is why we use immutable datastructures
/// that are optimized for cloning (they clone underlying values only if they are modified)
pub struct UidIndex {
    pub mail_uid: OrdMap<MailUuid, ImapUid>,
    pub mail_flags: OrdMap<MailUuid, Vec<Flag>>,
    pub mails_by_uid: OrdMap<ImapUid, MailUuid>,
    pub flags: HashMap<Flag, HashSet<MailUuid>>,

    pub uidvalidity: ImapUidvalidity,
    pub uidnext: ImapUid,
    pub internalseq: ImapUid,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum UidIndexOp {
    MailAdd(MailUuid, ImapUid, Vec<Flag>),
    MailDel(MailUuid),
    FlagAdd(MailUuid, Vec<Flag>),
    FlagDel(MailUuid, Vec<Flag>),
}

impl UidIndex {
    #[must_use]
    pub fn op_mail_add(&self, uuid: MailUuid, flags: Vec<Flag>) -> UidIndexOp {
        UidIndexOp::MailAdd(uuid, self.internalseq, flags)
    }

    #[must_use]
    pub fn op_mail_del(&self, uuid: MailUuid) -> UidIndexOp {
        UidIndexOp::MailDel(uuid)
    }

    #[must_use]
    pub fn op_flag_add(&self, uuid: MailUuid, flags: Vec<Flag>) -> UidIndexOp {
        UidIndexOp::FlagAdd(uuid, flags)
    }

    #[must_use]
    pub fn op_flag_del(&self, uuid: MailUuid, flags: Vec<Flag>) -> UidIndexOp {
        UidIndexOp::FlagDel(uuid, flags)
    }
}

impl Default for UidIndex {
    fn default() -> Self {
        Self {
            mail_flags: OrdMap::new(),
            mail_uid: OrdMap::new(),
            mails_by_uid: OrdMap::new(),
            flags: HashMap::new(),
            uidvalidity: 1,
            uidnext: 1,
            internalseq: 1,
        }
    }
}

impl BayouState for UidIndex {
    type Op = UidIndexOp;

    fn apply(&self, op: &UidIndexOp) -> Self {
        let mut new = self.clone();
        match op {
            UidIndexOp::MailAdd(uuid, uid, flags) => {
                if *uid < new.internalseq {
                    new.uidvalidity += new.internalseq - *uid;
                }
                let new_uid = new.internalseq;

                if let Some(prev_uid) = new.mail_uid.get(uuid) {
                    new.mails_by_uid.remove(prev_uid);
                } else {
                    new.mail_flags.insert(*uuid, flags.clone());
                }
                new.mails_by_uid.insert(new_uid, *uuid);
                new.mail_uid.insert(*uuid, new_uid);

                new.internalseq += 1;
                new.uidnext = new.internalseq;
            }
            UidIndexOp::MailDel(uuid) => {
                if let Some(uid) = new.mail_uid.get(uuid) {
                    new.mails_by_uid.remove(uid);
                    new.mail_uid.remove(uuid);
                    new.mail_flags.remove(uuid);
                }
                new.internalseq += 1;
            }
            UidIndexOp::FlagAdd(uuid, new_flags) => {
                // Upate mapping Email -> Flag
                let mail_flags = new.mail_flags.entry(*uuid).or_insert(vec![]);
                for flag in new_flags {
                    if !mail_flags.contains(flag) {
                        mail_flags.push(flag.to_string());
                    }
                }

                // Update mapping Flag -> Email
                let _ = new_flags.iter().map(|flag| {
                    new.flags
                        .entry(flag.clone())
                        .or_insert(HashSet::new())
                        .update(*uuid)
                });
            }
            UidIndexOp::FlagDel(uuid, rm_flags) => {
                // Upate mapping Email -> Flag
                if let Some(mail_flags) = new.mail_flags.get_mut(uuid) {
                    mail_flags.retain(|x| !rm_flags.contains(x));
                }

                // Update mapping Flag -> Email
                rm_flags.iter().for_each(|flag| {
                    new.flags
                        .entry(flag.clone())
                        .and_modify(|hs| { hs.remove(uuid); });
                });
            }
        }
        new
    }
}

// ---- CUSTOM SERIALIZATION AND DESERIALIZATION ----

#[derive(Serialize, Deserialize)]
struct UidIndexSerializedRepr {
    mails: Vec<(ImapUid, MailUuid, Vec<Flag>)>,
    uidvalidity: ImapUidvalidity,
    uidnext: ImapUid,
    internalseq: ImapUid,
}

impl<'de> Deserialize<'de> for UidIndex {
    fn deserialize<D>(d: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let val: UidIndexSerializedRepr = UidIndexSerializedRepr::deserialize(d)?;

        let mut uidindex = UidIndex {
            mail_flags: OrdMap::new(),
            mail_uid: OrdMap::new(),
            mails_by_uid: OrdMap::new(),
            flags: HashMap::new(),
            uidvalidity: val.uidvalidity,
            uidnext: val.uidnext,
            internalseq: val.internalseq,
        };

        for (uid, uuid, flags) in val.mails {
            uidindex.mail_flags.insert(uuid, flags);
            uidindex.mail_uid.insert(uuid, uid);
            uidindex.mails_by_uid.insert(uid, uuid);
        }

        Ok(uidindex)
    }
}

impl Serialize for UidIndex {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut mails = vec![];
        for (uid, uuid) in self.mails_by_uid.iter() {
            mails.push((
                *uid,
                *uuid,
                self.mail_flags.get(uuid).cloned().unwrap_or_default(),
            ));
        }

        let val = UidIndexSerializedRepr {
            mails,
            uidvalidity: self.uidvalidity,
            uidnext: self.uidnext,
            internalseq: self.internalseq,
        };

        val.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for MailUuid {
    fn deserialize<D>(d: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let v = String::deserialize(d)?;
        let bytes = hex::decode(v).map_err(|_| D::Error::custom("invalid hex"))?;

        if bytes.len() != 24 {
            return Err(D::Error::custom("bad length"));
        }

        let mut tmp = [0u8; 24];
        tmp[..].copy_from_slice(&bytes);
        Ok(Self(tmp))
    }
}

impl Serialize for MailUuid {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&hex::encode(self.0))
    }
}
