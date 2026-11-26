"""Fill a Studio shipping template and preview the returned job using Labelize HTTP."""
import argparse
import base64
import json
from pathlib import Path
from urllib.request import Request, urlopen


def post(url, body, content_type):
    request = Request(url, data=body, headers={"Content-Type": content_type})
    with urlopen(request, timeout=30) as response:
        return response.read()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("template", type=Path)
    parser.add_argument("order", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--url", default="http://127.0.0.1:8080")
    args = parser.parse_args()
    language = args.template.suffix[1:].lower()
    base = args.url.rstrip("/")
    request = {
        "language": language,
        "template_base64": base64.b64encode(args.template.read_bytes()).decode("ascii"),
        "values": json.loads(args.order.read_text(encoding="utf-8")),
    }
    job = post(base + "/template/fill", json.dumps(request).encode(), "application/json")
    preview = post(base + "/convert?output=png&dpmm=8&width=100&height=50", job,
                   "application/" + language)
    args.output.mkdir(parents=True, exist_ok=True)
    (args.output / ("order." + language)).write_bytes(job)
    (args.output / "order.png").write_bytes(preview)


if __name__ == "__main__":
    main()
