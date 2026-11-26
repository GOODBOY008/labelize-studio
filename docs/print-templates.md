# Print business labels from an application

Open a design in Studio, choose the target printer language and resolution, and select **Parameter preview**. The initial record uses the design's sample values. Edit individual fields or paste one JSON object to replace the whole record. Each tab keeps its preview values until you close it. Save the design to keep its editable layout and shared samples. In the shipping example, the same order number appears in the title, Code 128 and QR. [Shipping designs and templates](../examples/print-templates)

Choose **Export print template…** to bundle the layout with an application. You can also open a saved `.tpl.zpl`, `.tpl.tspl`, `.tpl.epl`, `.tpl.cpcl` or `.tpl.dpl` file directly in Studio. Set the resolution to the one used for export and supply the stock dimensions when the commands omit them. Enter the values, check the preview and export the filled label as PNG or PDF. A field error identifies the affected object and the printer's requirement.

## Fill a template with Labelize

Studio uses Labelize **1.13.0**. Keep the template and filled job as bytes, including any binary artwork. Supply string values for all declared fields to preserve identifiers such as `000123`. These examples use the bundled 100 × 50 mm shipping labels at 8 dots/mm. [Runnable CLI, HTTP and Rust SDK examples](../examples/print-templates/README.md) · [Labelize SDK and format reference](https://github.com/GOODBOY008/labelize/blob/v1.13.0/docs/PRINT_TEMPLATES.md)

```sh
cd examples/print-templates
labelize template fill shipping.tpl.tspl --data first.json -o order.tspl
labelize template render shipping.tpl.tspl --data first.json --dpmm 8 --width 100 --height 50 -o order.png
labelize template render shipping-logo.tpl.dpl --data second.json --dpmm 8 --width 100 --height 50 -o order.pdf
```

## Target capabilities

The designer has ten barcode types. The linear types are Code 128, Code 39, EAN-13, EAN-8, UPC-A and ITF. The matrix types are QR, Data Matrix, PDF417 and Aztec. Coverage below applies to variable fields. The exporter reports incompatible visible objects by layer number and name.

All five languages export the ten barcode types. The table lists their device-specific options.

| Export | Fixed artwork | Variable text | Barcode options |
| --- | --- | --- | --- |
| ZPL | Embedded monochrome graphic | Device font, optional wrapping, UTF-8 with an installed font | QR uses 0° |
| TSPL/TSC | Binary BITMAP | Resident 1–8, or UTF-8 with a matching Asian bitmap font | Four QR and Aztec rotations. PDF417 and Aztec carry byte counts |
| EPL2 | Binary GW | Resident 1–5, ASCII | QR, Data Matrix and Aztec use 0°. Data Matrix is square |
| CPCL | Binary CG | Resident 0 or 7, or UTF-8 with an installed Chinese font | Barcodes support 0° and 270°. Data Matrix uses square ECC 200 |
| DPL | Native records for simple layouts. Composited monochrome BMP for logos, styled artwork and overlapping fixed layers | Resident 0–2, ASCII | Square Data Matrix. Use the design's media height for bottom-origin positioning |

Use single-line text on TSPL, EPL2, CPCL and DPL. Sample content determines the alignment anchor and module size at export. Reserve space for the longest business value. Retail codes accept the body or its correct check digit. The filler writes the body into the symbol and the complete number into an optional caption.

For variable Chinese text, choose **UTF-8** and an installed device font. TSPL supports the 16- and 24-dot TSS, TST and TTT faces. CPCL supports `GBUNSG16.CPF` and `GBUNSG24.CPF` on compatible printers. Import a matching font to proof the design canvas. Reopening the filled job in Studio uses the engine's preview font for Chinese glyphs and half-width ASCII. Existing TSPL text files using `CODEPAGE 936` and CPCL text files using `COUNTRY GB18030` or `ENCODING GB18030` can be opened as original bytes. Physical glyph shapes depend on the device font and firmware. [Upstream font and text support](https://github.com/GOODBOY008/labelize/pull/166) · [TSC programming manual](https://fs.tscprinters.com/system/files/31-0000001-00_tspl_tspl2_programming_3_0.pdf) · [Zebra mobile printer guide](https://www.zebra.com/content/dam/support-dam/en/documentation/unrestricted/guide/product/zq310plus-zq320plus-ug-en.pdf)

One DPL template can contain the logo, order text and both order-number barcodes. The job downloads the logo to RAM, positions it on the label and releases its image after printing. Variable fields remain native commands. [Editable example](../examples/print-templates/shipping-logo.labelize.json) · [8 dots/mm DPL resource](../examples/print-templates/shipping-logo.tpl.dpl)

DPL Aztec templates use the native `W1f` record with ECI disabled and automatic error correction. Labelize reads the four option digits separately from the business value and preserves any leading zeros in that value. [Honeywell command reference](https://prod-edam.honeywell.com/content/dam/honeywell-edam/sps/ppr/en-ca/public/products/printers/common/documents/sps-ppr-dpl-en-cr.pdf) · [Upstream DPL fix](https://github.com/GOODBOY008/labelize/pull/168)

## File format for other host languages

A ZPL template uses a `^FXLabelize field <kind>^FS` comment before each native field. The expression following `^FH^FD` contains UTF-8 literal bytes as `_HH` and named `{{slots}}`. QR adds an error-correction and input-mode prefix. Labelize applies ZPL field escaping and the native rules for text blocks, barcodes and captions. [ZPL conventions](https://github.com/GOODBOY008/labelize/blob/v1.13.0/docs/PRINT_TEMPLATES.md#zpl-field-markers)

Each other export is a single file. An ASCII directory starts with `LABELIZE-TPL/1 <language> <charset>\n`. Each subsequent line contains the decimal byte offset, byte length, 1-based design object number, kind, codec and parameters, separated by spaces. A blank line ends the directory. Offsets are relative to the first byte of the native job that follows. The parameters carry QR correction, Data Matrix shape or the PDF417 column count and correction level. Other fields use `-`.

Copy the fixed bytes between ranges verbatim and expand each expression once. Apply the command's data encoding before writing the rest of the job. EPL2 escapes quotes and backslashes. TSPL uses the bracketed quote escape in quoted fields. Its PDF417 and Aztec codecs count encoded bytes, including leading commas and embedded line breaks. Aztec carries binary data without an ECI in this mode. Configure the scanner's text encoding to match when interpreting non-ASCII bytes as text. CPCL Data Matrix doubles its underscore escape and uses decimal byte escapes for controls and terminator-like records. DPL counted fields include their options in the four-digit count. The `dpl-aztec` codec prefixes the four default option digits. Treat graphics as opaque bytes throughout. [Native template codecs](https://github.com/GOODBOY008/labelize/blob/v1.13.0/docs/PRINT_TEMPLATES.md#tspltsc-epl2-cpcl-and-dpl-byte-directories)
