# .NET SDK

A managed .NET 8 library using the versioned C ABI. Encoding, Reed–Solomon, SVG/PNG rendering and raster scanning run in **Rust**, not in C#.

```bash
cargo build --release -p wavecode-ffi
dotnet build bindings/dotnet/WaveCode.Native
WAVECODE_LIBRARY="$PWD/target/release/libwavecode_ffi.so" \
  dotnet run --project bindings/dotnet/WaveCode.Sample
```

For macOS use `libwavecode_ffi.dylib`; for Windows use `wavecode_ffi.dll`. The `WAVECODE_LIBRARY` path is optional if the binary can be resolved by the OS loader.

```csharp
using WaveCode.Native;

var symbols = WaveCodeApi.Encode("https://music.youtube.com/watch?v=dQw4w9WgXcQ&t=59");
var svg = WaveCodeApi.RenderSvg(symbols, 960, 240);
var png = WaveCodeApi.RenderPng(symbols, 960, 240);
Console.WriteLine(WaveCodeApi.Scan(png));
```

Native binaries are not embedded in this library; package the correct target-specific binary with your application.
