## pdf-extract

A Rust library for extracting text content from PDF files.

This is a fork of [jrmuizel/pdf-extract](https://github.com/jrmuizel/pdf-extract),
maintained as part of [find-anything](https://github.com/jamietre/find-anything) — a
distributed full-content file indexer. The fork exists because the upstream library
panics on a wide class of malformed-but-real-world PDFs; we'd like to err on the side
of retrieving as much content as possible rather than strict spec validation.

```rust
let bytes = std::fs::read("tests/docs/simple.pdf").unwrap();
let out = pdf_extract::extract_text_from_mem(&bytes).unwrap();
assert!(out.contains("This is a small demonstration"));
```

---

## Differences from upstream

### Resilient extraction

The guiding principle is: **extract as much content as possible; only give up when
there is truly nothing left to salvage.** Bad input in one part of a document should
not prevent extraction from the rest of it.

Upstream panics on a broad range of malformed inputs — unknown glyph names, truncated
font tables, missing CMap entries, malformed colorspace arrays, and more. Each of
these was a hard crash that silenced every other page in the document. This fork
replaces those panics with `warn!` calls and safe fallbacks so extraction continues:

| Area                                                                          | Upstream behaviour              | This fork                                              |
| ----------------------------------------------------------------------------- | ------------------------------- | ------------------------------------------------------ |
| Unknown glyph names (ZapfDingbats, custom fonts)                              | `unwrap()` panic                | Try alternate glyph table; skip silently if not found  |
| Malformed font encoding / CFF parse failure                                   | panic / `.expect()`             | Warn and skip encoding table; use best-effort fallback |
| Type0 / CID font with missing or unknown CMap                                 | panic                           | Fall back to Identity-H encoding                       |
| `ToUnicode` CMap parse failure                                                | panic                           | Warn and continue without the unicode map              |
| Malformed colorspace arrays                                                   | panic / `unwrap()`              | Fall back to `DeviceRGB`                               |
| Font `Widths` array length mismatch                                           | `assert_eq!` panic              | Warn and use available widths                          |
| PDF path operators (`m`, `l`, `c`, `v`, `y`, `re`, `w`) with too few operands | index-out-of-bounds panic       | Warn and skip the operator                             |
| Empty path in `current_point()`                                               | `unwrap()` panic                | Return `(0.0, 0.0)`                                    |
| Content stream decode failure                                                 | panic                           | Warn and skip the affected page                        |
| `show_text` called with no font selected                                      | `unwrap()` panic                | Warn and return                                        |
| UTF-16 decode errors                                                          | panic                           | Lossy decode (invalid sequences → U+FFFD)              |
| Corrupt deflate stream in Type1 font                                          | panic (escaping `catch_unwind`) | Wrapped in `catch_unwind`; warn and continue           |
| Type1 encoding parse failure (`type1-encoding-parser`)                        | `.expect()` panic               | Wrapped in `catch_unwind`; warn and continue           |

Deeply structural failures — a missing `Root` or `Pages` tree, broken object reference
chains — are still allowed to propagate. A document that corrupt has no recoverable
content; the caller's `catch_unwind` is the right place to handle it.

### Integrated logging

The original `dlog!` macro discarded all debug output at compile time, making it
impossible to observe internal behaviour at runtime. This fork replaces it with
`log::debug!`, routing through the standard `log` façade. Consumers that initialise
a logger (e.g. `tracing-subscriber` with the `tracing-log` bridge) will see debug
output when `RUST_LOG=debug` is set; consumers that do not initialise a logger
continue to see nothing — no behaviour change by default.

Warning-level events (bad glyph names, malformed operators, etc.) are emitted via
`log::warn!` and are similarly visible only when a logger is configured.

---

## See also

- https://github.com/elacin/PDFExtract/
- https://github.com/euske/pdfminer / https://github.com/pdfminer/pdfminer.six
- https://gitlab.com/crossref/pdfextract
- https://github.com/VikParuchuri/marker
- https://github.com/kermitt2/pdfalto used by [grobid](https://github.com/kermitt2/grobid/)
- https://github.com/opendatalab/MinerU (uses PyMuPDF and pdfminer.six)

### Not PDF specific

- https://github.com/Layout-Parser/layout-parser
