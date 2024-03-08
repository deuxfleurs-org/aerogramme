#![no_main]

use libfuzzer_sys::fuzz_target;
use libfuzzer_sys::arbitrary;
use libfuzzer_sys::arbitrary::Arbitrary;

use aero_dav::{types, realization, xml};
use quick_xml::reader::NsReader;
use tokio::runtime::Runtime;
use tokio::io::AsyncWriteExt;

const tokens: [&str; 63] = [
"0",
"1",
"activelock",
"allprop",
"encoding",
"utf-8",
"http://ns.example.com/boxschema/",
"HTTP/1.1 200 OK",
"1997-12-01T18:27:21-08:00",
"Mon, 12 Jan 1998 09:25:56 GMT",
"\"abcdef\"",
"cannot-modify-protected-property",
"collection",
"creationdate",
"DAV:",
"D",
"C",
"xmlns:D",
"depth",
"displayname",
"error",
"exclusive",
"getcontentlanguage",
"getcontentlength",
"getcontenttype",
"getetag",
"getlastmodified",
"href",
"include",
"Infinite",
"infinity",
"location",
"lockdiscovery",
"lockentry",
"lockinfo",
"lockroot",
"lockscope",
"locktoken",
"lock-token-matches-request-uri",
"lock-token-submitted",
"locktype",
"multistatus",
"no-conflicting-lock",
"no-external-entities",
"owner",
"preserved-live-properties",
"prop",
"propertyupdate",
"propfind",
"propfind-finite-depth",
"propname",
"propstat",
"remove",
"resourcetype",
"response",
"responsedescription",
"set",
"shared",
"status",
"supportedlock",
"text/html",
"timeout",
"write",
];

#[derive(Arbitrary)]
enum Token {
    Known(usize),
    //Unknown(String),
}
impl Token {
    fn serialize(&self) -> String {
        match self {
            Self::Known(i) => tokens[i % tokens.len()].to_string(),
            //Self::Unknown(v) => v.to_string(),
        }
    }
}

#[derive(Arbitrary)]
struct Tag {
    //prefix: Option<Token>,
    name: Token,
    attr: Option<(Token, Token)>,
}
impl Tag {
    fn start(&self) -> String {
        let mut acc = String::new();
        /*if let Some(p) = &self.prefix {
            acc.push_str(p.serialize().as_str());
            acc.push_str(":");
        }*/
        acc.push_str("D:");
        acc.push_str(self.name.serialize().as_str());

        if let Some((k,v)) = &self.attr {
            acc.push_str(" ");
            acc.push_str(k.serialize().as_str());
            acc.push_str("=\"");
            acc.push_str(v.serialize().as_str());
            acc.push_str("\"");
        }
        acc
    }
    fn end(&self) -> String {
        let mut acc = String::new();
        acc.push_str("D:");
        acc.push_str(self.name.serialize().as_str());
        acc
    }
}


#[derive(Arbitrary)]
enum XmlNode {
    Node(Tag, Vec<Self>),
    Number(u64),
    Text(Token),
}
impl std::fmt::Debug for XmlNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.serialize())
    }
}
impl XmlNode {
    fn serialize(&self) -> String {
        match self {
            Self::Node(tag, children) => {
                let stag = tag.start();
                match children.is_empty() {
                    true => format!("<{}/>", stag),
                    false => format!("<{}>{}</{}>", stag, children.iter().map(|v| v.serialize()).collect::<String>(), tag.end()),
                }
            },
            Self::Number(v) => format!("{}", v),
            Self::Text(v) => v.serialize(),
        }
    }
}

async fn serialize(elem: &impl xml::QWrite) -> Vec<u8> {
    let mut buffer = Vec::new();
    let mut tokio_buffer = tokio::io::BufWriter::new(&mut buffer);
    let q = quick_xml::writer::Writer::new_with_indent(&mut tokio_buffer, b' ', 4);
    let ns_to_apply = vec![ ("xmlns:D".into(), "DAV:".into()) ];
    let mut writer = xml::Writer { q, ns_to_apply };

    elem.qwrite(&mut writer).await.expect("xml serialization");
    tokio_buffer.flush().await.expect("tokio buffer flush");

    return buffer
}

type Object = types::Multistatus<realization::Core, types::PropValue<realization::Core>>;

fuzz_target!(|nodes: XmlNode| {
    let gen = format!("<D:multistatus xmlns:D=\"DAV:\">{}<D:/multistatus>", nodes.serialize());
    //println!("--------\n{}", gen);
    let data = gen.as_bytes();

    let rt = Runtime::new().expect("tokio runtime initialization");

    rt.block_on(async {
        // 1. Setup fuzzing by finding an input that seems correct, do not crash yet then.
        let mut rdr = match xml::Reader::new(NsReader::from_reader(data)).await {
            Err(_) => return,
            Ok(r) => r,
        };
        let reference = match rdr.find::<Object>().await {
            Err(_) => return,
            Ok(m) => m,
        };

        // 2. Re-serialize the input
        let my_serialization = serialize(&reference).await;

        // 3. De-serialize my serialization
        let mut rdr2 = xml::Reader::new(NsReader::from_reader(my_serialization.as_slice())).await.expect("XML Reader init");
        let comparison = rdr2.find::<Object>().await.expect("Deserialize again");

        // 4. Both the first decoding and last decoding must be identical
        assert_eq!(reference, comparison);
    })
});
