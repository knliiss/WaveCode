package io.wavecode;

import static org.junit.jupiter.api.Assertions.*;
import org.junit.jupiter.api.Test;

public class WaveCodeTest {
    @Test
    public void nativeRoundTripWithRasterScan() {
        WaveCode api = new WaveCode();
        String url = "https://music.youtube.com/watch?v=dQw4w9WgXcQ&t=59";
        byte[] symbols = api.encode(url);
        assertEquals(1, api.formatVersion());
        assertEquals(34, symbols.length);
        assertEquals(url, api.decode(symbols));
        assertTrue(api.renderSvg(symbols, 960, 240).contains("<svg"));
        byte[] png = api.renderPng(symbols, 960, 240);
        assertEquals((byte) 0x89, png[0]);
        assertEquals(url, api.scan(png));
        assertTrue(api.scanAllJson(png).contains(url));
    }
}
