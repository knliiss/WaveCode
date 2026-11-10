# JVM / Kotlin SDK

Thin Java 17 wrapper using JNA over the same Rust C ABI, callable directly from
Java or Kotlin. No Reed–Solomon or image processing is duplicated in JVM code.

```bash
cargo build -p wavecode-ffi
WAVECODE_LIBRARY="$PWD/target/debug/libwavecode_ffi.so" \
  mvn -f bindings/jvm/pom.xml test
```

In Java:

```java
WaveCode sdk = new WaveCode();
byte[] symbols = sdk.encode("https://music.youtube.com/watch?v=dQw4w9WgXcQ&t=59");
byte[] png = sdk.renderPng(symbols, 960, 240);
System.out.println(sdk.scan(png));
```

In Kotlin:

```kotlin
val sdk = io.wavecode.WaveCode()
val code = sdk.encode("https://music.youtube.com/watch?v=dQw4w9WgXcQ&t=59")
println(sdk.scan(sdk.renderPng(code, 960, 240)))
```

The built jar depends on JNA and loads the platform-specific
`wavecode_ffi` library from `WAVECODE_LIBRARY` or the OS library search path.
Android packaging requires bundling the appropriate native `.so` per ABI.
