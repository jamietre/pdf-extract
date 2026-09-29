//! Regression tests for the fork's panic-hardening work: extraction of malformed
//! PDFs must return (Ok or Err) but never panic. PDFs are synthesized in memory
//! with lopdf so no binary fixtures are needed.

use lopdf::{dictionary, Dictionary, Document, Object, Stream};
use pdf_extract::extract_text_from_mem;
use test_log::test;

/// A page's content stream plus the font resources it can reference.
struct Page {
    content: Vec<u8>,
    fonts: Vec<(&'static str, Dictionary)>,
}

fn page(content: &str, fonts: Vec<(&'static str, Dictionary)>) -> Page {
    Page { content: content.as_bytes().to_vec(), fonts }
}

fn helvetica() -> Dictionary {
    dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica" }
}

fn build(pages: Vec<Page>) -> Vec<u8> {
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let mut kids = vec![];
    for p in pages {
        let mut font_res = Dictionary::new();
        for (name, dict) in p.fonts {
            font_res.set(name, doc.add_object(dict));
        }
        let content_id = doc.add_object(Stream::new(Dictionary::new(), p.content));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => content_id,
            "Resources" => dictionary! { "Font" => font_res },
        });
        kids.push(Object::Reference(page_id));
    }
    let count = kids.len() as i64;
    doc.objects.insert(pages_id, Object::Dictionary(dictionary! {
        "Type" => "Pages", "Kids" => kids, "Count" => count,
    }));
    let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog_id);
    let mut out = vec![];
    doc.save_to(&mut out).unwrap();
    out
}

/// Extraction must not panic. Returns the text if extraction succeeded.
fn extract(pages: Vec<Page>) -> Option<String> {
    extract_text_from_mem(&build(pages)).ok()
}

fn text_page(font: Dictionary) -> Page {
    page("BT /F1 12 Tf 72 700 Td (Hello) Tj ET", vec![("F1", font)])
}

#[test]
fn baseline_well_formed_pdf_extracts_text() {
    let out = extract(vec![text_page(helvetica())]).expect("well-formed PDF should extract");
    assert!(out.contains("Hello"), "got {:?}", out);
}

// --- content stream operators -------------------------------------------------

#[test]
fn undecodable_content_stream() {
    extract(vec![page("BT ( unbalanced \x00\x01 << [ Tj", vec![])]);
    extract(vec![page("\x00\x01\x02 garbage <<>> ]] ))", vec![])]);
}

#[test]
fn tf_with_missing_operand() {
    extract(vec![page("BT Tf (Hello) Tj ET", vec![("F1", helvetica())])]);
}

#[test]
fn tf_operand_not_a_name() {
    extract(vec![page("BT 12 12 Tf (Hello) Tj ET", vec![("F1", helvetica())])]);
}

#[test]
fn tf_font_missing_from_resources() {
    extract(vec![page("BT /Nope 12 Tf (Hello) Tj ET", vec![("F1", helvetica())])]);
    extract(vec![page("BT /F1 12 Tf (Hello) Tj ET", vec![])]);
}

#[test]
fn text_shown_with_no_font_selected() {
    extract(vec![page("BT 72 700 Td (Hello) Tj ET", vec![])]);
}

#[test]
fn path_operators_missing_operands() {
    for ops in ["m", "l", "c", "v", "y", "re", "h", "m l c v y re h", "1 m", "1 2 3 c", "1 2 re", "h S f"] {
        extract(vec![page(ops, vec![])]);
    }
}

#[test]
fn path_ops_on_empty_path() {
    // current_point() on an empty path; closing / stroking / filling with nothing built
    extract(vec![page("h S", vec![])]);
    extract(vec![page("1 2 l 3 4 5 6 7 8 c f", vec![])]);
}

// --- font dictionaries --------------------------------------------------------

#[test]
fn font_missing_required_keys() {
    extract(vec![text_page(dictionary! { "Type" => "Font" })]);
    extract(vec![text_page(dictionary! { "Subtype" => "Type1" })]);
    extract(vec![text_page(dictionary! { "Subtype" => "TrueType" })]);
    extract(vec![text_page(dictionary! { "Subtype" => "Type0" })]);
    extract(vec![text_page(Dictionary::new())]);
}

#[test]
fn font_subtype_wrong_type() {
    extract(vec![text_page(dictionary! { "Subtype" => 5, "BaseFont" => 7 })]);
}

fn truetype(extra: Vec<(&'static str, Object)>) -> Dictionary {
    let mut d = dictionary! { "Type" => "Font", "Subtype" => "TrueType", "BaseFont" => "Foo" };
    for (k, v) in extra {
        d.set(k, v);
    }
    d
}

#[test]
fn widths_not_an_array() {
    extract(vec![text_page(truetype(vec![
        ("FirstChar", 32.into()), ("LastChar", 126.into()), ("Widths", 500.into()),
    ]))]);
}

#[test]
fn widths_length_mismatch() {
    // 3 widths but FirstChar..LastChar spans 95 entries (previously assert_eq! panic)
    extract(vec![text_page(truetype(vec![
        ("FirstChar", 32.into()), ("LastChar", 126.into()),
        ("Widths", vec![500.into(), 500.into(), 500.into()].into()),
    ]))]);
    // more widths than the range allows
    extract(vec![text_page(truetype(vec![
        ("FirstChar", 32.into()), ("LastChar", 33.into()),
        ("Widths", vec![500.into(); 10].into()),
    ]))]);
}

#[test]
fn widths_with_wrong_element_types() {
    extract(vec![text_page(truetype(vec![
        ("FirstChar", 32.into()), ("LastChar", 34.into()),
        ("Widths", vec![Object::string_literal("x"), Object::Null, "n".into()].into()),
    ]))]);
}

#[test]
fn first_last_char_missing_or_wrong_type() {
    extract(vec![text_page(truetype(vec![("Widths", vec![500.into()].into())]))]);
    extract(vec![text_page(truetype(vec![
        ("FirstChar", "a".into()), ("LastChar", Object::Null), ("Widths", vec![500.into()].into()),
    ]))]);
}

#[test]
fn font_descriptor_wrong_type() {
    extract(vec![text_page(truetype(vec![("FontDescriptor", 3.into())]))]);
    extract(vec![text_page(truetype(vec![("FontDescriptor", Object::Dictionary(Dictionary::new()))]))]);
}

// --- encodings ----------------------------------------------------------------

fn type1(encoding: Object) -> Dictionary {
    let mut d = helvetica();
    d.set("Encoding", encoding);
    d
}

#[test]
fn encoding_unexpected_object_type() {
    extract(vec![text_page(type1(Object::Integer(42)))]);
    extract(vec![text_page(type1(Object::Null))]);
    extract(vec![text_page(type1(Object::Boolean(true)))]);
    extract(vec![text_page(type1(vec![1.into()].into()))]);
}

#[test]
fn encoding_unknown_name_falls_back() {
    // previously panic!("unexpected encoding ...")
    let out = extract(vec![text_page(type1("NotARealEncoding".into()))]);
    if let Some(out) = out {
        assert!(out.contains("Hello"), "got {:?}", out);
    }
}

#[test]
fn encoding_dict_base_encoding_wrong_type() {
    extract(vec![text_page(type1(Object::Dictionary(dictionary! { "BaseEncoding" => 3 })))]);
    extract(vec![text_page(type1(Object::Dictionary(dictionary! { "BaseEncoding" => "Bogus" })))]);
}

#[test]
fn differences_with_wrong_element_types() {
    // stray strings/dicts/null where numbers or names belong
    let diffs: Vec<Object> = vec![
        65.into(), Object::string_literal("A"), Object::Null,
        Object::Dictionary(Dictionary::new()), Object::Boolean(false), "B".into(),
    ];
    extract(vec![text_page(type1(Object::Dictionary(dictionary! { "Differences" => diffs })))]);
}

#[test]
fn differences_with_unknown_and_zapf_glyph_names() {
    // unknown glyph names used to unwrap() -> panic in name_to_unicode
    let diffs: Vec<Object> = vec![
        65.into(), "thisglyphdoesnotexist".into(), "a1".into(), "a206".into(), "zzzz".into(),
    ];
    let out = extract(vec![text_page(type1(Object::Dictionary(dictionary! { "Differences" => diffs })))]);
    assert!(out.is_some(), "unknown glyph names should not fail extraction");
}

#[test]
fn differences_before_any_code() {
    // names with no preceding code number
    let diffs: Vec<Object> = vec!["A".into(), "B".into()];
    extract(vec![text_page(type1(Object::Dictionary(dictionary! { "Differences" => diffs })))]);
}

// --- embedded font programs / ToUnicode ---------------------------------------

fn with_stream(doc_font: &mut Document, key: &'static str, dict: Dictionary, data: &[u8]) -> Object {
    let _ = key;
    Object::Reference(doc_font.add_object(Stream::new(dict, data.to_vec())))
}

/// Build a one-page PDF where the font has a garbage stream under some key.
fn build_with_font_streams(
    font: Dictionary,
    descriptor: Option<Dictionary>,
    font_file_key: Option<(&'static str, Dictionary, &[u8])>,
    to_unicode: Option<&[u8]>,
) -> Vec<u8> {
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let mut font = font;
    if let Some(mut desc) = descriptor {
        if let Some((key, dict, data)) = font_file_key {
            let obj = with_stream(&mut doc, key, dict, data);
            desc.set(key, obj);
        }
        font.set("FontDescriptor", doc.add_object(desc));
    }
    if let Some(data) = to_unicode {
        let obj = with_stream(&mut doc, "ToUnicode", Dictionary::new(), data);
        font.set("ToUnicode", obj);
    }
    let font_id = doc.add_object(font);
    let content_id = doc.add_object(Stream::new(
        Dictionary::new(),
        b"BT /F1 12 Tf 72 700 Td (Hello) Tj ET".to_vec(),
    ));
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page", "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => content_id,
        "Resources" => dictionary! { "Font" => dictionary! { "F1" => font_id } },
    });
    doc.objects.insert(pages_id, Object::Dictionary(dictionary! {
        "Type" => "Pages", "Kids" => vec![Object::Reference(page_id)], "Count" => 1,
    }));
    let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog_id);
    let mut out = vec![];
    doc.save_to(&mut out).unwrap();
    out
}

fn descriptor() -> Dictionary {
    dictionary! { "Type" => "FontDescriptor", "FontName" => "Foo", "Flags" => 32 }
}

#[test]
fn garbage_type1_font_file() {
    // exercises the Type1 encoding parser path (get_encoding_map replaced by parse())
    for junk in [&b""[..], b"garbage", b"/Encoding 256 array put put put def", b"\xff\xfe\x00\x01 array array array"] {
        let bytes = build_with_font_streams(
            type1(Object::Null), Some(descriptor()), Some(("FontFile", Dictionary::new(), junk)), None);
        let _ = extract_text_from_mem(&bytes);
    }
}

#[test]
fn type1_encoding_program_with_short_operand_stack() {
    // "array" / "put" with fewer than the 2-3 preceding tokens the parser indexes back into
    for junk in [&b"array"[..], b"1 array", b"/Encoding array", b"put", b"dup put def", b"/Encoding 256 array"] {
        let bytes = build_with_font_streams(
            helvetica(), Some(descriptor()), Some(("FontFile", Dictionary::new(), junk)), None);
        let _ = extract_text_from_mem(&bytes);
    }
}

#[test]
fn garbage_type1c_font_file() {
    let mut f = helvetica();
    f.set("Subtype", "Type1");
    for junk in [&b""[..], b"\x01\x00\x04\x01", b"not a cff table at all", &[0xffu8; 64][..]] {
        let bytes = build_with_font_streams(
            f.clone(), Some(descriptor()),
            Some(("FontFile3", dictionary! { "Subtype" => "Type1C" }, junk)), None);
        let _ = extract_text_from_mem(&bytes);
    }
}

#[test]
fn garbage_to_unicode_cmap() {
    let cases: [&[u8]; 5] = [
        b"",
        b"garbage",
        b"\xff\xfe\x00\x01",
        // odd-length UTF-16BE destination (previously assert!(v.len() % 2 == 0))
        b"/CIDInit /ProcSet findresource begin 12 dict begin begincmap 1 begincodespacerange <00> <ff> endcodespacerange 1 beginbfchar <48> <005> endbfchar endcmap",
        // lone surrogate destination
        b"1 begincodespacerange <00> <ff> endcodespacerange 1 beginbfchar <48> <d800> endbfchar",
    ];
    for c in cases {
        let bytes = build_with_font_streams(helvetica(), None, None, Some(c));
        let _ = extract_text_from_mem(&bytes);
    }
}

#[test]
fn to_unicode_maps_to_invalid_codepoint() {
    // surrogate pair split across entries / out-of-range values
    let cmap = b"1 begincodespacerange <00> <ff> endcodespacerange \
                 2 beginbfchar <48> <dc00> <65> <ffff> endbfchar";
    let bytes = build_with_font_streams(helvetica(), None, None, Some(cmap));
    let _ = extract_text_from_mem(&bytes);
}

// --- Type3 fonts --------------------------------------------------------------

fn type3(extra: Vec<(&'static str, Object)>) -> Dictionary {
    let mut d = dictionary! {
        "Type" => "Font", "Subtype" => "Type3",
        "FontBBox" => vec![0.into(), 0.into(), 1000.into(), 1000.into()],
        "FontMatrix" => vec![0.001.into(), 0.into(), 0.into(), 0.001.into(), 0.into(), 0.into()],
    };
    for (k, v) in extra {
        d.set(k, v);
    }
    d
}

#[test]
fn type3_without_widths_or_encoding_or_charprocs() {
    extract(vec![text_page(type3(vec![]))]);
}

#[test]
fn type3_widths_length_mismatch() {
    // regression: "Fix Type3 font widths assertion panic on malformed PDFs"
    extract(vec![text_page(type3(vec![
        ("FirstChar", 32.into()), ("LastChar", 126.into()),
        ("Widths", vec![500.into(), 500.into()].into()),
    ]))]);
    extract(vec![text_page(type3(vec![
        ("FirstChar", 65.into()), ("LastChar", 65.into()),
        ("Widths", vec![500.into(); 5].into()),
    ]))]);
}

#[test]
fn type3_widths_not_an_array() {
    extract(vec![text_page(type3(vec![
        ("FirstChar", 32.into()), ("LastChar", 126.into()), ("Widths", "nope".into()),
    ]))]);
}

#[test]
fn type3_character_missing_width() {
    // widths only cover 'A'; the text shows 'Hello' (previously panic!("missing width ..."))
    extract(vec![text_page(type3(vec![
        ("FirstChar", 65.into()), ("LastChar", 65.into()),
        ("Widths", vec![500.into()].into()),
    ]))]);
}

#[test]
fn type3_encoding_unexpected_type() {
    extract(vec![text_page(type3(vec![("Encoding", Object::Integer(7))]))]);
    extract(vec![text_page(type3(vec![("Encoding", Object::Null)]))]);
}

#[test]
fn type3_differences_with_wrong_element_types() {
    let diffs: Vec<Object> = vec![
        65.into(), Object::string_literal("x"), Object::Dictionary(Dictionary::new()), Object::Null,
    ];
    extract(vec![text_page(type3(vec![(
        "Encoding", Object::Dictionary(dictionary! { "Differences" => diffs }),
    )]))]);
}

// --- multi-page / font cache --------------------------------------------------

#[test]
fn same_font_resource_name_reused_across_pages() {
    // Upstream 0.12 caches fonts by resource name; a name reused for a different
    // font on the next page must not pick up the previous page's font.
    let symbol = dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Symbol" };
    let out = extract_text_from_mem(&build(vec![
        page("BT /F1 12 Tf 72 700 Td (Hello) Tj ET", vec![("F1", helvetica())]),
        page("BT /F1 12 Tf 72 700 Td (World) Tj ET", vec![("F1", symbol)]),
        page("BT /F1 12 Tf 72 700 Td (Again) Tj ET", vec![("F1", helvetica())]),
    ])).expect("multi-page extraction");
    assert!(out.contains("Hello"), "got {:?}", out);
    assert!(out.contains("Again"), "got {:?}", out);
}

#[test]
fn bad_page_does_not_prevent_later_pages() {
    let out = extract_text_from_mem(&build(vec![
        page("BT /Nope 12 Tf (x) Tj ET \x00\x01 <<", vec![]),
        text_page(helvetica()),
    ]));
    if let Ok(out) = out {
        assert!(out.contains("Hello"), "got {:?}", out);
    }
}
