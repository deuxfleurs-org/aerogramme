use imap_codec::imap_types::core::NonEmptyVec;
use imap_codec::imap_types::response::Capability;

#[derive(Debug, Clone)]
pub struct ServerCapability {
    r#move: bool,
    unselect: bool,
}

impl Default for ServerCapability {
    fn default() -> Self {
        Self {
            r#move: true,
            unselect: true,
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
            acc.push(Capability::try_from("UNSELECT").unwrap());
        }
        acc.try_into().unwrap()
    }
}
