import fcntl
import os
from pathlib import Path


class HomeInUse(RuntimeError):
    pass


class HomeLock:
    def __init__(self, path, inherited=None):
        self.path = Path(path)
        self.inherited = inherited
        self.fd = None

    def __enter__(self):
        self.path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
        if self.inherited is None:
            descriptor = os.open(self.path, os.O_CREAT | os.O_RDWR | os.O_NOFOLLOW, 0o600)
        else:
            descriptor = os.dup(int(self.inherited))
            held, expected = os.fstat(descriptor), self.path.stat()
            if (held.st_dev, held.st_ino) != (expected.st_dev, expected.st_ino):
                os.close(descriptor)
                raise RuntimeError("Inherited lock belongs to another home")
        try:
            fcntl.flock(descriptor, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            os.close(descriptor)
            raise HomeInUse("A configured home is already in use") from None
        self.fd = descriptor
        return descriptor

    def __exit__(self, *_error):
        if self.fd is not None:
            os.close(self.fd)
            self.fd = None
