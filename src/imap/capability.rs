use imap_codec::imap_types::core::NonEmptyVec;
use imap_codec::imap_types::extensions::enable::{CapabilityEnable, Utf8Kind};
use imap_codec::imap_types::response::Capability;
use std::collections::HashSet;

fn capability_unselect() -> Capability<'static> {
    Capability::try_from("UNSELECT").unwrap()
}

fn capability_condstore() -> Capability<'static> {
    Capability::try_from("CONDSTORE").unwrap()
}

fn capability_qresync() -> Capability<'static> {
    Capability::try_from("QRESYNC").unwrap()
}

#[derive(Debug, Clone)]
pub struct ServerCapability(HashSet<Capability<'static>>);

impl Default for ServerCapability {
    fn default() -> Self {
        Self(HashSet::from([
            Capability::Imap4Rev1,
            Capability::Enable,
            Capability::Move,
            Capability::LiteralPlus,
            capability_unselect(),
            //capability_condstore(),
            //capability_qresync(),
        ]))
    }
}

impl ServerCapability {
    pub fn to_vec(&self) -> NonEmptyVec<Capability<'static>> {
        self.0
            .iter()
            .map(|v| v.clone())
            .collect::<Vec<_>>()
            .try_into()
            .unwrap()
    }

    #[allow(dead_code)]
    pub fn support(&self, cap: &Capability<'static>) -> bool {
        self.0.contains(cap)
    }
}

enum ClientStatus {
    NotSupportedByServer,
    Disabled,
    Enabled,
}

pub struct ClientCapability {
    condstore: ClientStatus,
    utf8kind: Option<Utf8Kind>,
}

impl ClientCapability {
    pub fn new(sc: &ServerCapability) -> Self {
        Self {
            condstore: match sc.0.contains(&capability_condstore()) {
                true => ClientStatus::Disabled,
                _ => ClientStatus::NotSupportedByServer,
            },
            utf8kind: None,
        }
    }

    pub fn try_enable(
        &mut self,
        caps: &[CapabilityEnable<'static>],
    ) -> Vec<CapabilityEnable<'static>> {
        let mut enabled = vec![];
        for cap in caps {
            match cap {
                CapabilityEnable::CondStore if matches!(self.condstore, ClientStatus::Disabled) => {
                    self.condstore = ClientStatus::Enabled;
                    enabled.push(cap.clone());
                }
                CapabilityEnable::Utf8(kind) if Some(kind) != self.utf8kind.as_ref() => {
                    self.utf8kind = Some(kind.clone());
                    enabled.push(cap.clone());
                }
                _ => (),
            }
        }

        enabled
    }
}
