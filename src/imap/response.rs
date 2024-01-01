use anyhow::Result;
use imap_codec::imap_types::command::Command;
use imap_codec::imap_types::core::Tag;
use imap_codec::imap_types::response::{Code, Data, Status, StatusKind};

pub struct ResponseBuilder {
    status: StatusKind,
    tag: Option<Tag<'static>>,
    code: Option<Code<'static>>,
    text: String,
    data: Vec<Data<'static>>,
}

impl<'a> Default for ResponseBuilder {
    fn default() -> ResponseBuilder {
        ResponseBuilder {
            status: StatusKind::Bad,
            tag: None,
            code: None,
            text: "".to_string(),
            data: vec![],
        }
    }
}

impl ResponseBuilder {
    pub fn to_req(mut self, cmd: &Command) -> Self {
        self.tag = Some(cmd.tag);
        self
    }
    pub fn tag(mut self, tag: Tag) -> Self {
        self.tag = Some(tag);
        self
    }

    pub fn message(mut self, txt: impl Into<String>) -> Self {
        self.text = txt.into();
        self
    }

    pub fn code(mut self, code: Code) -> Self {
        self.code = Some(code);
        self
    }

    pub fn data(mut self, data: Data) -> Self {
        self.data.push(data);
        self
    }

    pub fn set_data(mut self, data: Vec<Data>) -> Self {
        self.data = data;
        self
    }

    pub fn build(self) -> Result<Response> {
        Ok(Response {
            status: Status::new(self.tag, self.status, self.code, self.text)?,
            data: self.data,
        })
    }
}

pub struct Response {
    data: Vec<Data<'static>>,
    status: Status<'static>,
}

impl Response {
    pub fn ok() -> ResponseBuilder {
        ResponseBuilder {
            status: StatusKind::Ok,
            ..ResponseBuilder::default()
        }
    }

    pub fn no() -> ResponseBuilder {
        ResponseBuilder {
            status: StatusKind::No,
            ..ResponseBuilder::default()
        }
    }

    pub fn bad() -> ResponseBuilder {
        ResponseBuilder {
            status: StatusKind::Bad,
            ..ResponseBuilder::default()
        }
    }

    pub fn bye() -> Result<Response> {
        Ok(Response {
            status: Status::bye(None, "bye")?,
            data: vec![],
        })
    }
}
