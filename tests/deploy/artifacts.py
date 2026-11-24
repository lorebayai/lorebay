import platform


ARCHITECTURE = {"arm64": "aarch64", "amd64": "x86_64"}.get(platform.machine(), platform.machine())
TARGET = f"{ARCHITECTURE}-unknown-linux-musl"
NATIVE_ARCHIVES = tuple(f"{name}-{TARGET}.tar.gz" for name in ("lorebay", "lorebayd"))
