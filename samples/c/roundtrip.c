#include "wavecode.h"
#include <stdint.h>
#include <stdio.h>
#include <string.h>

int main(void) {
    const char *source = "https://music.youtube.com/watch?v=dQw4w9WgXcQ&t=59";
    WaveCodeBuffer symbols = {0}, png = {0}, url = {0}, all = {0};

    if (wavecode_abi_version() != 1 || wavecode_format_version() != 1)
        return 1;

    if (wavecode_v1_encode_url((const uint8_t *)source, strlen(source),
                               WAVECODE_MOMENT_FROM_URL, &symbols) != WAVECODE_OK)
        return 2;
    if (symbols.len != WAVECODE_V1_SYMBOL_COUNT)
        return 3;

    if (wavecode_v1_render_png(symbols.ptr, symbols.len, 900, 240, &png) != WAVECODE_OK)
        return 4;
    if (png.len < 8 || png.ptr[0] != 0x89 || png.ptr[1] != 'P')
        return 5;

    if (wavecode_v1_scan_image(png.ptr, png.len, &url) != WAVECODE_OK)
        return 6;
    if (url.len != strlen(source) || memcmp(url.ptr, source, url.len) != 0)
        return 7;

    if (wavecode_v1_scan_all(png.ptr, png.len, &all) != WAVECODE_OK)
        return 8;
    if (all.len < 10 || all.ptr[0] != '[')
        return 9;

    printf("C ABI roundtrip successful: %.*s\n", (int)url.len, (char *)url.ptr);
    wavecode_free_buffer(symbols);
    wavecode_free_buffer(png);
    wavecode_free_buffer(url);
    wavecode_free_buffer(all);
    return 0;
}
