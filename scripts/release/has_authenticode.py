#!/usr/bin/env python3
"""Does this Windows artifact carry an Authenticode signature?

Structural, not cryptographic, and deliberately so. The Windows runner
already did the authoritative check — `Get-AuthenticodeSignature` is
literally what Windows does on first launch — and repeating that on Linux
means trusting a second tool's opinion about a Microsoft format.

v0.1.0-test7 is why this exists: `osslsigncode verify` reported both
artifacts UNSIGNED while Windows reported both Valid, and the truth was
that the files carried a 15,720-byte PKCS#7 blob apiece. The guard failed
closed, which was the right failure, but it failed for the wrong reason
and told us nothing about why — it captured the tool's output and threw
it away.

So this asks the one question a publish guard actually needs answered:
is there a signature in here at all? Presence here plus Valid on the
Windows runner is the pair that closes the hole. No dependencies, no
output strings to match against, and it prints what it found.
"""

import struct
import sys

SECURITY_DIRECTORY_INDEX = 4


def pe_signature_size(data: bytes) -> int:
    """Bytes in the PE certificate table, or 0. -1 if not a PE."""
    if len(data) < 0x40 or data[:2] != b"MZ":
        return -1
    pe = struct.unpack_from("<I", data, 0x3C)[0]
    if len(data) < pe + 24 or data[pe : pe + 4] != b"PE\0\0":
        return -1
    magic = struct.unpack_from("<H", data, pe + 24)[0]
    # PE32+ has a wider optional header before the data directory.
    dd = pe + 24 + (112 if magic == 0x20B else 96)
    if len(data) < dd + 8 * (SECURITY_DIRECTORY_INDEX + 1):
        return 0
    _rva, size = struct.unpack_from("<II", data, dd + 8 * SECURITY_DIRECTORY_INDEX)
    return size


def msi_is_signed(data: bytes) -> bool:
    """An MSI is an OLE compound file; a signed one carries a
    \\x05DigitalSignature stream, whose name is stored UTF-16LE."""
    return "\x05DigitalSignature".encode("utf-16-le") in data


def main(paths):
    for path in paths:
        with open(path, "rb") as fh:
            data = fh.read()
        name = path.rsplit("/", 1)[-1]
        size = pe_signature_size(data)
        if size >= 0:
            kind, signed, detail = "PE", size > 0, f"{size} byte certificate table"
        else:
            signed = msi_is_signed(data)
            kind, detail = "MSI", "DigitalSignature stream present"
        if signed:
            print(f"SIGNED    {name}  ({kind}, {detail})")
        else:
            print(f"UNSIGNED  {name}  ({kind}, no signature found)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
