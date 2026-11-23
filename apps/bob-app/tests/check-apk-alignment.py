"""Reject incompatible native libraries in the actual packaged APK."""
import struct
import sys
import zipfile

def check(data):
    if data[:6] != b"\x7fELF\x02\x01":
        raise ValueError("expected little-endian ELF64")
    offset = struct.unpack_from("<Q", data, 32)[0]
    size, count = struct.unpack_from("<HH", data, 54)
    loads = 0
    for i in range(count):
        kind, flags, off, addr, phys, files, mem, align = struct.unpack_from("<IIQQQQQQ", data, offset + i * size)
        if kind == 1:
            loads += 1
            if align < 16384 or (addr - off) % 16384:
                raise ValueError(f"LOAD alignment={align} offset={off} address={addr}")
        if kind == 0x6474e552 and (addr + mem) % 16384:
            raise ValueError("RELRO end is not 16 KB aligned")
    if not loads:
        raise ValueError("no LOAD segments")

if __name__ == "__main__":
    with zipfile.ZipFile(sys.argv[1]) as apk:
        libraries = [n for n in apk.namelist() if n.startswith("lib/") and n.endswith(".so")]
        if not libraries:
            raise SystemExit("No native libraries found")
        for name in libraries:
            check(apk.read(name))
            print("16 KB ELF PASS:", name)
