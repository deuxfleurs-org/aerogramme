use imap_codec::imap_types::core::NonEmptyVec;
use imap_codec::imap_types::response::Capability;

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
pub struct ServerCapability {
    r#move: bool,
    unselect: bool,
    condstore: bool,
    qresync: bool,
}

impl Default for ServerCapability {
    fn default() -> Self {
        Self {
            r#move: true,
            unselect: true,
            condstore: false,
            qresync: false,
        }
    }
}

impl ServerCapability {
    pub fn to_vec(&self) -> NonEmptyVec<Capability<'static>> {
        let mut acc = vec![Capability::Imap4Rev1];
        if self.r#move {
            acc.push(Capability::Move);
        }
        if self.unselect {
            acc.push(capability_unselect());
        }
        if self.condstore {
            acc.push(capability_condstore());
        }
        if self.qresync {
            acc.push(capability_qresync());
        }
        acc.try_into().unwrap()
    }

    pub fn support(&self, cap: &Capability<'static>) -> bool {
        match cap {
            Capability::Imap4Rev1 => true,
            Capability::Move => self.r#move,
            x if *x == capability_condstore() => self.condstore,
            x if *x == capability_qresync() => self.qresync,
            x if *x == capability_unselect() => self.unselect,
            _ => false,
        }
    }
}

pub struct ClientCapability {
    condstore: bool,
    qresync: bool,
}

impl Default for ClientCapability {
    fn default() -> Self {
        Self {
            condstore: false,
            qresync: false,
        }
    }
}

impl ClientCapability {
    pub fn try_enable(
        &mut self,
        srv: &ServerCapability,
        caps: &[Capability<'static>],
    ) -> Vec<Capability<'static>> {
        let mut enabled = vec![];
        for cap in caps {
            match cap {
                x if *x == capability_condstore() && srv.condstore && !self.condstore => {
                    self.condstore = true;
                    enabled.push(x.clone());
                }
                x if *x == capability_qresync() && srv.qresync && !self.qresync => {
                    self.qresync = true;
                    enabled.push(x.clone());
                }
                _ => (),
            }
        }

        enabled
    }
}
