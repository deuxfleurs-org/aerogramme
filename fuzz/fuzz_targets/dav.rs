#![no_main]

use libfuzzer_sys::fuzz_target;
use aerogramme::dav::{types, realization, xml};
use quick_xml::reader::NsReader;
use tokio::runtime::Runtime;
use tokio::io::AsyncWriteExt;

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

fuzz_target!(|data: &[u8]| {
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
