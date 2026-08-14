use ical_vcard::Contentline; 
use std::io::Write;

pub mod collation;
pub mod filter;
pub mod query;

pub fn parse_lossy(raw: &[u8]) -> Vec<Contentline> {
    let mut contentlines = vec![];
    let mut parser = ical_vcard::Parser::new(raw);
    match parser.next() {
        Some(Ok(cl)) if cl.name() == "BEGIN" && cl.value() == "VCARD" => (),
        _ => {
            tracing::warn!("cannot parse vCard: does not start with BEGIN:VCARD");
            return vec![]
        },
    };
    for line_res in parser {
        match line_res {
            Ok(cl) if cl.name() == "END" && cl.value() == "VCARD" => break,
            Ok(cl) => contentlines.push(cl),
            Err(e) => {
                tracing::warn!(err=?e, "cannot parse vCard property");
            }
        }
    }
    contentlines
}

pub fn write(mut w: &mut impl Write, lines: impl IntoIterator<Item = Contentline>) -> std::io::Result<()> {
    w.write(b"BEGIN:VCARD\r\n")?;
    {
        let mut w = ical_vcard::Writer::new(&mut w);
        w.write_all(lines)?
    }
    w.write(b"END:VCARD\r\n")?;
    Ok(())
}
