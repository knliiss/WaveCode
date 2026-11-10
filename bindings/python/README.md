# Python SDK

WaveCode's Python package is a **thin ctypes wrapper** around the compiled Rust C ABI. There is no reimplementation of the codec.

```bash
cargo build --release -p wavecode-ffi
python -m pip install ./bindings/python

# Normally it locates target/release automatically from a source checkout.
# When deploying elsewhere:
export WAVECODE_LIBRARY=/absolute/path/to/libwavecode_ffi.so
```

On macOS use `libwavecode_ffi.dylib`; on Windows use `wavecode_ffi.dll`.

```python
from wavecode_native import WaveCode

api = WaveCode()
symbols = api.encode("https://music.youtube.com/watch?v=p-LBiCtzXu0&t=59")
png = api.render_png(symbols, 960, 240)
open("code.png", "wb").write(png)
print(api.scan(png))
print(api.scan_all(png))
```

**Distribution:** The Python package does not bundle the native library yet. For deployment, ship or install the correct library for the target OS/architecture and set `WAVECODE_LIBRARY`. Building platform-specific wheels is a later packaging step.
