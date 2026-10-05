"""EE main-RAM access for runtime validation.

Sources:
  pine[:port]        live PCSX2 through the PINE IPC socket (Settings > Advanced > Enable PINE,
                     default slot/port 28011)
  <file>.p2s         PCSX2 savestate (zip; the 32 MB EE RAM is the member eeMemory.bin)
  <file>.bin         raw 32 MB EE RAM image (e.g. written by `ramwalk.py snapshot`)

Addresses are EE virtual addresses: KSEG0/KSEG1 and the 0x2000_0000 uncached mirror are folded onto
physical RAM (mask 0x01ffffff), as the game uses plain 0x00xxxxxx pointers.
"""
import socket
import struct
import zipfile

RAM_SIZE = 32 * 1024 * 1024

MSG_READ64 = 3
MSG_VERSION = 8
MSG_TITLE = 0xB
MSG_ID = 0xC
MSG_GAME_VERSION = 0xE
MSG_STATUS = 0xF


def phys(addr):
    return addr & 0x01FFFFFF


class FileRam:
    def __init__(self, path):
        if path.lower().endswith(".p2s"):
            with zipfile.ZipFile(path) as z:
                self.data = z.read("eeMemory.bin")
        else:
            self.data = open(path, "rb").read()
        if len(self.data) != RAM_SIZE:
            raise ValueError(f"{path}: EE RAM image is {len(self.data)} bytes, expected {RAM_SIZE}")
        self.source = path

    def read(self, addr, n):
        p = phys(addr)
        return self.data[p:p + n]

    def info(self):
        return {"source": self.source}


class PineRam:
    """Minimal PINE client (batched 64-bit reads)."""
    BATCH = 50000  # 5-byte requests / 8-byte replies per command; stays below PINE's 650 kB buffers

    def __init__(self, port=28011, host="127.0.0.1"):
        self.sock = socket.create_connection((host, port), timeout=10)
        self.source = f"pine://{host}:{port}"
        self.cache = {}

    def _call(self, payload, reply_len):
        self.sock.sendall(struct.pack("<I", len(payload) + 4) + payload)
        hdr = self._recv(4)
        (size,) = struct.unpack("<I", hdr)
        body = self._recv(size - 4)
        if body[0] != 0:
            raise IOError("PINE request failed (is a game running?)")
        return body[1:]

    def _recv(self, n):
        buf = bytearray()
        while len(buf) < n:
            chunk = self.sock.recv(n - len(buf))
            if not chunk:
                raise IOError("PINE connection closed")
            buf += chunk
        return bytes(buf)

    def _string(self, op):
        body = self._call(bytes([op]), 0)
        (n,) = struct.unpack_from("<I", body)
        return body[4:4 + n].rstrip(b"\0").decode("utf-8", "replace")

    def info(self):
        out = {"source": self.source}
        for k, op in (("emulator", MSG_VERSION), ("title", MSG_TITLE), ("serial", MSG_ID),
                      ("game_version", MSG_GAME_VERSION)):
            try:
                out[k] = self._string(op)
            except IOError:
                out[k] = "?"
        try:
            out["status"] = {0: "running", 1: "paused", 2: "shutdown"}.get(
                struct.unpack_from("<I", self._call(bytes([MSG_STATUS]), 0))[0], "?")
        except IOError:
            out["status"] = "?"
        return out

    def read_block(self, addr, n):
        """Read n bytes (multiple of 8, addr 8-aligned) with batched Read64."""
        out = bytearray()
        words = n // 8
        for i in range(0, words, self.BATCH):
            cnt = min(self.BATCH, words - i)
            req = b"".join(bytes([MSG_READ64]) + struct.pack("<I", addr + (i + j) * 8) for j in range(cnt))
            out += self._call(req, cnt * 8)
        return bytes(out)

    def read(self, addr, n):
        # page cache (4 KB) so tree walks do not issue one request per field
        out = bytearray()
        a = addr
        end = addr + n
        while a < end:
            page = a & ~0xFFF
            if page not in self.cache:
                self.cache[page] = self.read_block(page, 0x1000)
            off = a - page
            take = min(0x1000 - off, end - a)
            out += self.cache[page][off:off + take]
            a += take
        return bytes(out)


def open_ram(spec):
    if spec.startswith("pine"):
        port = int(spec.split(":", 1)[1]) if ":" in spec else 28011
        return PineRam(port)
    return FileRam(spec)


class Mem:
    def __init__(self, ram):
        self.ram = ram

    def u8(self, a):
        return self.ram.read(a, 1)[0]

    def u16(self, a):
        return struct.unpack("<H", self.ram.read(a, 2))[0]

    def s16(self, a):
        return struct.unpack("<h", self.ram.read(a, 2))[0]

    def u32(self, a):
        return struct.unpack("<I", self.ram.read(a, 4))[0]

    def s8(self, a):
        v = self.u8(a)
        return v - 256 if v > 127 else v

    def cstr(self, a, n=24):
        raw = self.ram.read(a, n).split(b"\0", 1)[0]
        if raw and all(32 <= c < 127 for c in raw):
            return raw.decode()
        return None

    def valid_ptr(self, a):
        return a != 0 and phys(a) < RAM_SIZE and (a & 3) == 0 and (a >> 28) in (0, 2, 3, 8, 0xA)
