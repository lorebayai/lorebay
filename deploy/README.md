# Deployment

Install Lorebay on a server host and maintain its HTTPS entry point. Each
installation method has its own guide; shared tools live in `common/`.

## Usage

Follow the [native installation guide](native/README.md) for a systemd service
behind nginx on a supported Linux host.

The shared [nginx policy](common/nginx.conf.in),
[certificate replacement script](common/replace-certificate.py) and
[Python helpers](common/helpers.py) serve the installation and certificate
tools. The certificate script supports native nginx and container modes;
its commands are in the native guide.

Run static Python syntax and nginx configuration checks for all subfolders
from the repository root with Docker, OpenSSL and Python 3.11 or later:

```bash
moon run deploy:check
```

## Docs

Start at the [docs overview](../docs/overview.md).
