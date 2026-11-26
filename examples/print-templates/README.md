# Fill and preview a shipping order

Each example takes a Studio template, one JSON order and an output directory.
It writes the native printer job and a PNG preview of the same order. The
bundled templates use 100 × 50 mm stock at 8 dots/mm. Open
`shipping.labelize.json` in Studio to edit the layout, or open a
`shipping.tpl.*` file to try the order in the parameter form.
`shipping-logo.tpl.dpl` includes binary artwork.

Start in the example directory. The two orders exercise leading zeros, printer
command characters and literal braces.

```sh
cd examples/print-templates
```

## CLI

```sh
cargo install labelize --version '=1.13.0' --locked --features serve,vector
sh cli.sh shipping.tpl.tspl first.json ../../output/cli
```

## HTTP

Start the service with `labelize serve --host 127.0.0.1`. In another terminal
run the client with Python 3. It sends the original template bytes to
`/template/fill` and the returned printer job to `/convert`.

```sh
python3 http_example.py shipping.tpl.tspl first.json ../../output/http
```

## Rust SDK

The example uses the `labelize` and `serde_json` crates. An application
processing many records can keep the loaded `PrintTemplate` and call `fill`
for each order. [Rust source](order.rs)

```sh
cargo run --locked -p labelize-printer-core --example preview-order -- \
  shipping.tpl.tspl first.json ../../output/sdk
```
