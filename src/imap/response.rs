use anyhow::Result;
use imap_codec::imap_types::command::Command;
use imap_codec::imap_types::core::Tag;
use imap_codec::imap_types::response::{Code, Data, Status};

#[derive(Debug)]
pub enum Body<'a> {
    Data(Data<'a>),
    Status(Status<'a>),
}

pub struct ResponseBuilder<'a> {
    tag: Option<Tag<'a>>,
    code: Option<Code<'a>>,
    text: String,
    body: Vec<Body<'a>>,
}

impl<'a> ResponseBuilder<'a> {
    pub fn to_req(mut self, cmd: &Command<'a>) -> Self {
        self.tag = Some(cmd.tag.clone());
        self
    }
    pub fn tag(mut self, tag: Tag<'a>) -> Self {
        self.tag = Some(tag);
        self
    }

    pub fn message(mut self, txt: impl Into<String>) -> Self {
        self.text = txt.into();
        self
    }

    pub fn code(mut self, code: Code<'a>) -> Self {
        self.code = Some(code);
        self
    }

    pub fn data(mut self, data: Data<'a>) -> Self {
        self.body.push(Body::Data(data));
        self
    }

    pub fn many_data(mut self, data: Vec<Data<'a>>) -> Self {
        for d in data.into_iter() {
            self = self.data(d);
        }
        self
    }

    #[allow(dead_code)]
    pub fn info(mut self, status: Status<'a>) -> Self {
        self.body.push(Body::Status(status));
        self
    }

    #[allow(dead_code)]
    pub fn many_info(mut self, status: Vec<Status<'a>>) -> Self {
        for d in status.into_iter() {
            self = self.info(d);
        }
        self
    }

    pub fn set_body(mut self, body: Vec<Body<'a>>) -> Self {
        self.body = body;
        self
    }

    pub fn ok(self) -> Result<Response<'a>> {
        Ok(Response {
            completion: Status::ok(self.tag, self.code, self.text)?,
            body: self.body,
        })
    }

    pub fn no(self) -> Result<Response<'a>> {
        Ok(Response {
            completion: Status::no(self.tag, self.code, self.text)?,
            body: self.body,
        })
    }

    pub fn bad(self) -> Result<Response<'a>> {
        Ok(Response {
            completion: Status::bad(self.tag, self.code, self.text)?,
            body: self.body,
        })
    }
}

#[derive(Debug)]
pub struct Response<'a> {
    pub body: Vec<Body<'a>>,
    pub completion: Status<'a>,
}

impl<'a> Response<'a> {
    pub fn build() -> ResponseBuilder<'a> {
        ResponseBuilder {
            tag: None,
            code: None,
            text: "".to_string(),
            body: vec![],
        }
    }

    pub fn bye() -> Result<Response<'a>> {
        Ok(Response {
            completion: Status::bye(None, "bye")?,
            body: vec![],
        })
    }
}

#[derive(Debug)]
pub enum ResponseOrIdle {
    Response(Response<'static>),
    Idle,
}
