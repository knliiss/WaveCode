#ifndef WAVECODE_H
#define WAVECODE_H

#include <stddef.h>
#include <stdint.h>

#ifdef _WIN32
  #ifdef WAVECODE_BUILD_SHARED
    #define WAVECODE_API __declspec(dllexport)
  #else
    #define WAVECODE_API
  #endif
#else
  #define WAVECODE_API __attribute__((visibility("default")))
#endif

#ifdef __cplusplus
extern "C" {
#endif

/* ABI version 1. Bump it for any incompatible signature or layout change. */
#define WAVECODE_ABI_VERSION 1u
#define WAVECODE_V1_SYMBOL_COUNT 34u
#define WAVECODE_MOMENT_FROM_URL (-1)

/* The caller owns every output buffer until it calls wavecode_free_buffer.
 * Byte buffers are not NUL terminated. Strings are UTF-8.
 * Out parameters are reset to {NULL, 0} on failure.
 * The caller must pass valid pointers and must not free buffers itself.
 */
typedef struct WaveCodeBuffer {
    uint8_t *ptr;
    size_t len;
} WaveCodeBuffer;

typedef enum WaveCodeStatus {
    WAVECODE_OK = 0,
    WAVECODE_INVALID_ARGUMENT = 1,
    WAVECODE_ENCODE_ERROR = 2,
    WAVECODE_DECODE_ERROR = 3,
    WAVECODE_SCAN_ERROR = 4,
    WAVECODE_INTERNAL_ERROR = 255
} WaveCodeStatus;

WAVECODE_API uint32_t wavecode_abi_version(void);
WAVECODE_API uint32_t wavecode_format_version(void);
WAVECODE_API const char *wavecode_status_message(int32_t status);

/* Moment override = -1 to honor ?t= from URL, otherwise 0..65535 seconds. */
WAVECODE_API WaveCodeStatus wavecode_v1_encode_url(
    const uint8_t *url, size_t url_len, int32_t moment_override,
    WaveCodeBuffer *out_symbols);

/* Symbols: exactly 34 individual 6-bit values, not packed 204-bit data. */
WAVECODE_API WaveCodeStatus wavecode_v1_decode_symbols(
    const uint8_t *symbols, size_t symbols_len, WaveCodeBuffer *out_url);

WAVECODE_API WaveCodeStatus wavecode_v1_render_svg(
    const uint8_t *symbols, size_t symbols_len,
    uint32_t width, uint32_t height, WaveCodeBuffer *out_svg);

WAVECODE_API WaveCodeStatus wavecode_v1_render_png(
    const uint8_t *symbols, size_t symbols_len,
    uint32_t width, uint32_t height, WaveCodeBuffer *out_png);

/* scan_image returns one canonical URL, or a scan error if zero/multiple.
 * scan_all returns a UTF-8 JSON array with url, orientation, corrected_symbols,
 * and bounds in source-image pixels. Image format: PNG or JPEG.
 */
WAVECODE_API WaveCodeStatus wavecode_v1_scan_image(
    const uint8_t *image, size_t image_len, WaveCodeBuffer *out_url);

WAVECODE_API WaveCodeStatus wavecode_v1_scan_all(
    const uint8_t *image, size_t image_len, WaveCodeBuffer *out_json);

WAVECODE_API void wavecode_free_buffer(WaveCodeBuffer buffer);

#ifdef __cplusplus
}
#endif
#endif
