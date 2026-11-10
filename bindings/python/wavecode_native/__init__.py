"""Python wrapper over the versioned WaveCode Rust C ABI.

Build the native library first with: cargo build -p wavecode-ffi --release
Pass WAVECODE_LIBRARY=/path/to/libwavecode_ffi.so or rely on target/release.
"""
from __future__ import annotations

import ctypes
import json
import os
import sys
from pathlib import Path
from typing import Optional


class WaveCodeError(RuntimeError):
    """An error reported by the Rust WaveCode native library."""


class _Buffer(ctypes.Structure):
    _fields_ = [("ptr", ctypes.POINTER(ctypes.c_uint8)), ("len", ctypes.c_size_t)]


def _load_library(path: Optional[str]) -> ctypes.CDLL:
    filename = (
        "wavecode_ffi.dll" if sys.platform == "win32"
        else "libwavecode_ffi.dylib" if sys.platform == "darwin"
        else "libwavecode_ffi.so"
    )
    root = Path(__file__).resolve().parents[3]
    candidates = [
        path,
        os.getenv("WAVECODE_LIBRARY"),
        str(root / "target" / "release" / filename),
        str(root / "target" / "debug" / filename),
        filename,
    ]
    for candidate in candidates:
        if candidate and (Path(candidate).is_file() or candidate == filename):
            try:
                return ctypes.CDLL(str(candidate))
            except OSError:
                continue
    raise FileNotFoundError(
        "WaveCode native library not found. Build cargo -p wavecode-ffi "
        "or set WAVECODE_LIBRARY to the compiled shared library."
    )


class WaveCode:
    """Encoding, decoding, rendering and optical scanning in native Rust."""

    def __init__(self, library_path: Optional[str] = None) -> None:
        self._lib = _load_library(library_path)
        ptr = ctypes.POINTER(ctypes.c_uint8)
        size = ctypes.c_size_t
        out = ctypes.POINTER(_Buffer)
        self._lib.wavecode_abi_version.restype = ctypes.c_uint32
        self._lib.wavecode_format_version.restype = ctypes.c_uint32
        self._lib.wavecode_status_message.argtypes = [ctypes.c_int32]
        self._lib.wavecode_status_message.restype = ctypes.c_char_p
        self._lib.wavecode_free_buffer.argtypes = [_Buffer]
        self._lib.wavecode_free_buffer.restype = None
        self._lib.wavecode_v1_encode_url.argtypes = [ptr, size, ctypes.c_int32, out]
        self._lib.wavecode_v1_decode_symbols.argtypes = [ptr, size, out]
        self._lib.wavecode_v1_render_svg.argtypes = [ptr, size, ctypes.c_uint32, ctypes.c_uint32, out]
        self._lib.wavecode_v1_render_png.argtypes = [ptr, size, ctypes.c_uint32, ctypes.c_uint32, out]
        self._lib.wavecode_v1_scan_image.argtypes = [ptr, size, out]
        self._lib.wavecode_v1_scan_all.argtypes = [ptr, size, out]
        for name in (
            "wavecode_v1_encode_url",
            "wavecode_v1_decode_symbols",
            "wavecode_v1_render_svg",
            "wavecode_v1_render_png",
            "wavecode_v1_scan_image",
            "wavecode_v1_scan_all",
        ):
            getattr(self._lib, name).restype = ctypes.c_int32

        if self._lib.wavecode_abi_version() != 1:
            raise WaveCodeError("Unsupported WaveCode native ABI version")

    @property
    def format_version(self) -> int:
        return int(self._lib.wavecode_format_version())

    def _call(self, fn: str, data: bytes, *extra: int) -> bytes:
        # ctypes arrays live until the Rust call returns.
        raw = (ctypes.c_uint8 * len(data)).from_buffer_copy(data)
        result = _Buffer()
        status = getattr(self._lib, fn)(raw, len(data), *extra, ctypes.byref(result))
        if status != 0:
            message = self._lib.wavecode_status_message(status).decode("utf-8")
            raise WaveCodeError(f"{fn} failed: {message} (status {status})")
        try:
            return ctypes.string_at(result.ptr, result.len)
        finally:
            self._lib.wavecode_free_buffer(result)

    def encode(self, url: str, moment: Optional[int] = None) -> bytes:
        if moment is not None and not 0 <= moment <= 65535:
            raise ValueError("moment must be in 0..65535 seconds")
        return self._call(
            "wavecode_v1_encode_url", url.encode("utf-8"),
            -1 if moment is None else moment,
        )

    def decode(self, symbols: bytes) -> str:
        return self._call("wavecode_v1_decode_symbols", symbols).decode("utf-8")

    def render_svg(self, symbols: bytes, width: int = 648, height: int = 180) -> str:
        return self._call("wavecode_v1_render_svg", symbols, width, height).decode("utf-8")

    def render_png(self, symbols: bytes, width: int = 648, height: int = 180) -> bytes:
        return self._call("wavecode_v1_render_png", symbols, width, height)

    def scan(self, image: bytes) -> str:
        return self._call("wavecode_v1_scan_image", image).decode("utf-8")

    def scan_all(self, image: bytes) -> list:
        return json.loads(self._call("wavecode_v1_scan_all", image).decode("utf-8"))
