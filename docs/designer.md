# Label designer

[README](../README.md) · [Architecture](architecture.md) · [Acceptance](acceptance.md)

The visual designer shipped in **Labelize Studio v0.4.0**. ZPL template export requires a development build with variable binding. Existing Labelize Printer templates remain compatible.

Choose **Workspace → New design**, or open the [showcase template](../examples/designer-showcase.labelize.json). Objects are listed from front to back. Select an object to see the properties appropriate to its type. Preview and PDF/PNG export use the same rendering rules.

## Objects and properties

| Object | Properties |
| --- | --- |
| Text | Multiline content; document, monospace, bold monospace, or condensed bold font; size; manual or automatic wrapping; line spacing; horizontal and vertical alignment. |
| Rectangle | Stroke, outline or solid fill, and corner radius. |
| Ellipse / circle | Stroke and outline or solid fill. Equal width and height produce a circle. |
| Line | Stroke and horizontal, vertical, or either diagonal direction within its bounds. |
| Image | Embedded PNG/JPEG; replace image; contain, cover, or stretch; horizontal/vertical alignment; monochrome threshold. |
| Code 128, Code 39, EAN-13, EAN-8, UPC-A, ITF | Content, module width, quiet zone, horizontal alignment, optional human-readable caption and caption font/size. |
| QR | Content, module width, quiet zone, alignment, and L/M/Q/H error correction. |
| Data Matrix | Content, module width, quiet zone, alignment, and square or rectangular symbol selection. |
| PDF417 | Content, module width, quiet zone, alignment, columns (0 for automatic), and error correction 0–8. |
| Aztec | Content, module width, quiet zone, and alignment. |

Every object has a name, position, size, visibility, lock, clockwise quarter-turn rotation, and background. **Align to label** moves the object to an edge or center of the page. **Content alignment** positions text, images, or symbols within the object's own bounds. Width and height describe the object before rotation; a 90° or 270° turn swaps its occupied width and height. Rotation moves an object inward if necessary, and rejects a rotated object that cannot fit the page.

Transparent backgrounds preserve objects underneath. White backgrounds cover the full object rectangle; black backgrounds invert the object's pixels over an opaque black rectangle. New barcodes use white backgrounds to protect their quiet zones. A hidden object stays editable in the layer list but does not appear in preview or export. A locked object can be selected, hidden, unlocked, or duplicated, but cannot be moved, resized, reordered, deleted, or edited. Duplicating creates an unlocked copy.

## Editing

- Drag an unlocked object to move it; drag its bottom-right handle to resize. The object stays within the label. Resizing changes the bounds, not the text's font size.
- With canvas focus, arrows move 0.1 mm and Shift+arrows move 1 mm. Delete/Backspace removes the selected unlocked object. These keys keep their normal behavior inside text inputs.
- Use Undo/Redo in the canvas toolbar or Cmd/Ctrl+Z and Cmd/Ctrl+Shift+Z outside text inputs. History belongs to the current tab and covers style changes, layer operations, and object edits.
- Use **To front**, **Forward**, **Backward**, or **To back** to change overlap. Ordering follows the visible layer list.
- Below 1000 logical pixels, the toolbar's object-list button opens the tools/layers panel. Below 700, properties open in a separate drawer. Each panel scrolls independently; Page Up/Down scrolls the open drawer and Escape closes it.

Property changes refresh the preview after 500 ms of inactivity. A bad value keeps the last successful preview and shows an error. Local drafts retain unfinished input after one second, including values such as an incomplete minus sign. Saving a template or exporting requires valid geometry and property values. Content that fails rendering, such as an undersized barcode, may be saved for later correction but cannot produce a successful export.

## Fonts, images, and barcode data

**Document font** uses the template's imported TrueType/OpenType font, or the original monospace fallback when none is imported. Import a font containing the required glyphs for Chinese and other scripts; choosing a different built-in face does not add missing glyphs. Images are flattened over white before embedding. Threshold values below 128 produce less black ink; above 128 produce more. Contain preserves the whole image, Cover crops to fill, and Stretch changes its aspect ratio.

Module width `0` chooses the largest whole-dot module that fits. A positive width is rounded to dots at the current 8 or 12 dots/mm resolution. Too-small or too-large modules are rejected, rather than distorting the symbol. Quiet zone `0` uses the format's minimum; an explicit value must meet that minimum. QR uses four modules on each side, as described in the [DENSO WAVE guide](https://www.qrcode.com/en/howto/code.html). The inspector displays the minimum for each format.

EAN-13, EAN-8, and UPC-A accept the full number or the body without its final check digit. Incorrect supplied check digits are rejected. ITF requires an even number of digits and does not insert padding. Code 39 uses its uppercase/digit/punctuation alphabet. Code 128 and PDF417 currently accept ASCII; use QR or Data Matrix for UTF-8 content. The designer does not automatically add GS1 application identifiers, FNC1, or HIBC prefixes. Physical print quality and scanner settings still affect readability; automated decoder checks do not replace printed-label acceptance.

## Saving and compatibility

Templates embed images and the document font. Studio reads versions 1 through 4. Saving a static design uses version 2. Shared variables and the ZPL device font use version 3. Setting a device profile for another language uses version 4. Text in an existing design, including literal double braces, stays literal until you enable binding for that object.

PDF combines the document pages into one file. PNG saves one image per page. Exports capture the content and options when submitted and continue in the background after a tab or window closes.

### Export a design as ZPL (unreleased)

Select the target printer's **8 dpmm** or **12 dpmm** resolution under **Page settings**. In **Export**, choose **ZPL** and the output folder, then click **Export ZPL**. The completed job contains a single `.zpl` file. To preview it again in Studio, open it as a label file and select the same resolution.

The exporter rounds the outer label dimensions up to whole printer dots and embeds the visible layers as a monochrome graphic. Text, imported fonts, and barcodes are rendered into that graphic. Keep the `.labelize.json` template for editing. The minimum ZPL print width is two dots. [Zebra print-width reference](https://cpws.zebra.com/cpws/docs/zpl/PW_Command.pdf)

### Print business data from an application

1. Open the [shipping design](../examples/print-templates/shipping.labelize.json), or choose **New design**. Select a text object or any of the ten barcodes and enable **Bind variables**. Use your own field names. The shipping design uses `Order: {{order_no}}` for text and `{{order_no}}` for its barcodes. Names are case-sensitive and use ASCII letters, digits and underscores. Start with a letter or underscore.
2. Enter a **Shared sample value** for each name. Reusing a name in another object links both objects to the same value. Use the longest expected record for layout. The canvas and ordinary PDF, PNG and bitmap ZPL exports show the samples. **Save** retains expressions and samples for future editing.
3. Select the printer's **8** or **12 dpmm** resolution and the **Print template language**. Set the device font and encoding for the target printer where needed. Use an installed ZPL font path such as `E:MYFONT.TTF` for variable Chinese text. Import a matching font into Studio for local proofing.
4. Choose **Parameter preview** to try a record. Edit each named field or paste a complete JSON object with string values. The preview and PNG/PDF export use the filled printer job. Switch back to the design to edit its shared samples.
5. Choose **Export print template…** and save the file with its suggested extension. If the target cannot reproduce a visible layer, Studio lists the affected objects before opening the save dialog. Adjust the listed objects or choose another target.
6. Reopen the exported template in Studio to try another record, or bundle the exported file with your application. Use the Labelize CLI, HTTP API or SDK to fill each record and get native printer bytes. [Integration guide, examples and device capabilities](print-templates.md)

Use representative sample values when choosing field widths and alignment. An opaque foreground layer can erase ink from a variable beneath it. Reserve space for the longest value and the barcode quiet zones.

## Rendering quality (unreleased)

In **Export → Rendering**, choose **Raster** for the existing output or **Vector** to retain native text, shape outlines and barcode modules in PDF. Imported bitmap images remain images. Both modes preserve object order, backgrounds, clipping, quarter-turn rotations and physical label dimensions.

**Antialias** retains gray edge pixels in previews and PNG output, and in raster PDF. It is off by default. Vector PDF stores geometry; its edge smoothing is determined by the viewer. Vector PNG offers **1×–4×** resolution without changing the label's physical size. It is a bitmap rendered from the scene, not a vector file.

Rendering choices belong to the tab's local session and export snapshot, rather than the portable template format. They survive tab/session restoration and do not mark the source file as modified. Existing templates and settings keep the original black-and-white raster defaults.
