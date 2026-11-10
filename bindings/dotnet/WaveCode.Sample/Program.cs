using WaveCode.Native;

const string url = "https://music.youtube.com/watch?v=p-LBiCtzXu0&t=59";
var code = WaveCodeApi.Encode(url);
if (code.Length != 34 || WaveCodeApi.Decode(code) != url)
    throw new InvalidOperationException("WaveCode V1 roundtrip failed");

var png = WaveCodeApi.RenderPng(code, 960, 240);
if (WaveCodeApi.Scan(png) != url || WaveCodeApi.ScanAll(png)[0].Url != url)
    throw new InvalidOperationException("WaveCode raster scan failed");

Console.WriteLine($"WaveCode .NET roundtrip OK — {url}");
